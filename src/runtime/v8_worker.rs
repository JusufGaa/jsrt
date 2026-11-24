use std::path::{Path, PathBuf};
use v8::{
    Global, OwnedIsolate, PinScope, Context,
    Local, Value as V8Value, Array, GetPropertyNamesArgs,
    KeyCollectionMode,
};
use std::collections::{HashMap, HashSet};
use crate::js_value::{JsAtom, JsObject, JsValue};

use super::global_this::{
    GlobalThis, GlobalThisSnapshot, js_value_to_source, operation_statements,
};

pub struct V8Worker {
    file_name: Option<String>,
    cached_code: Vec<String>,
    isolate: OwnedIsolate,
    context: Global<Context>,
    /// History of snapshots (one per `save`).
    snapshots: Vec<GlobalThisSnapshot>,
    /// Tracked current state of `globalThis`.
    global_this: GlobalThis,
}

pub struct V8ScriptOrigin {
    resource_line_offset: i32,
    resource_column_offset: i32,
    resource_is_shared_cross_origin: bool,
    script_id: i32,
    resource_is_opaque: bool,
    is_wasm: bool,
    is_module: bool,
}

impl Default for V8ScriptOrigin {
    fn default() -> Self {
        Self {
            resource_line_offset: 0,
            resource_column_offset: 0,
            resource_is_shared_cross_origin: false,
            script_id: -1,
            resource_is_opaque: true,
            is_wasm: false,
            is_module: false,
        }
    }
}

impl V8Worker {
    pub fn new() -> Self {
        let mut isolate = v8::Isolate::new(Default::default());
        let context = Self::new_context(&mut isolate);

        V8Worker {
            file_name: None,
            cached_code: Vec::new(),
            isolate,
            context,
            snapshots: Vec::new(),
            global_this: GlobalThis::new(),
        }
    }

    /// Create a brand new `Context` inside a fresh `HandleScope`.
    fn new_context(isolate: &mut OwnedIsolate) -> Global<Context> {
        v8::scope!(let scope, isolate);
        let ctx = v8::Context::new(scope, Default::default());
        Global::new(scope, ctx)
    }

    /// Enter the isolate with the worker's current context and run `f`.
    ///
    /// A scope must stay pinned on the stack and borrows its parent, so it
    /// cannot be returned from a method directly; a closure is used instead.
    fn with_scope<R>(
        &mut self,
        f: impl FnOnce(&mut PinScope<'_, '_>, Local<'_, Context>) -> R,
    ) -> R {
        v8::scope_with_context!(let scope, &mut self.isolate, &self.context);
        let ctx = Local::new(scope, &self.context);
        f(scope, ctx)
    }

    fn eval_with_origin(&mut self, code: &str, origin: impl Into<Option<V8ScriptOrigin>>) -> Option<JsValue> {
        let file_name = self.file_name.clone();

        self.with_scope(|scope, _| {
            // Capture exceptions in a TryCatch so they are swallowed silently
            // instead of being reported as uncaught errors by V8.
            v8::tc_scope!(let tc, scope);

            let resource_name =
                v8::String::new(&tc, file_name.as_deref().unwrap_or("repl"));
            let Some(resource_name) = resource_name else {
                return None;
            };
            if let Some(code) = v8::String::new(&tc, code) {
                if let Some(script) = match origin.into() {
                    Some(origin) => {
                        let origin = v8::ScriptOrigin::new(&tc,
                            resource_name.into(),
                            origin.resource_line_offset,
                            origin.resource_column_offset,
                            origin.resource_is_shared_cross_origin,
                            origin.script_id,
                            None,
                            origin.resource_is_opaque,
                            origin.is_wasm,
                            origin.is_module,
                            None);
                        v8::Script::compile(&tc, code, Some(&origin))
                    },
                    _ => v8::Script::compile(&tc, code, None)
                } {
                    if let Some(result) = script.run(&tc) {
                        return Some(v8_value_to_value(&tc, result.into()))
                    }
                }
            }
            None
        })
    }
}

impl super::Worker for V8Worker {
    fn save(&mut self) {
        let global_value = self.with_scope(|scope, ctx| {
            let global = ctx.global(scope);
            v8_value_to_value(scope, global.into())
        });

        // Current `globalThis` state: variable name -> value.
        let mut current = HashMap::<String, JsValue>::new();
        if let Some(obj) = global_value.as_object() {
            for (key, value) in obj.props() {
                if let Some(name) = key.as_str() {
                    current.insert(name.to_string(), *value);
                }
            }
        }

        // Diff against the tracked state to build a snapshot of operations.
        let mut snapshot = GlobalThisSnapshot::new();
        for (name, value) in &current {
            match self.global_this.get(name) {
                None => snapshot.insert(name.clone(), *value),
                Some(old) if js_value_to_source(old) != js_value_to_source(value) => {
                    snapshot.update(name.clone(), *value)
                }
                _ => {}
            }
        }
        for (name, _) in self.global_this.iter() {
            if !current.contains_key(name) {
                snapshot.delete(name.clone());
            }
        }

        let mut tracked = GlobalThis::new();
        for (name, value) in current {
            tracked.insert(name, value);
        }
        self.global_this = tracked;
        self.snapshots.push(snapshot);
    }

    fn restore(&mut self) {
        self.context = Self::new_context(&mut self.isolate);

        let t = self.snapshots.pop();
        if let Some(popped) = t {
            // Snapshots are deltas, so replay every snapshot in order,
            // ending with the popped one, to rebuild the state of its
            // save point.
            let mut statements = Vec::new();
            for snapshot in &self.snapshots {
                statements.extend(operation_statements(snapshot.operations()));
            }
            statements.extend(operation_statements(popped.operations()));

            let code = statements.join("\n");
            if !code.is_empty() {
                self.eval_with_origin(&code, None);
            }
        }
    }

    fn compile(&mut self, code: impl AsRef<str>) {
        self.cached_code.push(code.as_ref().to_string());
    }

    fn run(&mut self) -> JsValue {
        let code = std::mem::take(&mut self.cached_code).join("\n");
        if code.is_empty() {
            return JsValue::NULL;
        }
        self.eval_with_origin(&code, V8ScriptOrigin::default())
            .unwrap_or(JsValue::NULL)
    }

    fn import(&mut self, module_name: impl AsRef<Path>) -> JsValue {
        let path = self.resolve_module_path(module_name.as_ref());
        let code = match std::fs::read_to_string(&path) {
            Ok(code) => code,
            Err(_) => return JsValue::NULL,
        };

        let prev_file_name = self.file_name.take();
        self.file_name = Some(path.display().to_string());
        let result = self.eval_with_origin(&code, V8ScriptOrigin::default());
        self.file_name = prev_file_name;

        result.unwrap_or(JsValue::NULL)
    }
}

impl V8Worker {
    fn resolve_module_path(&self, module_name: &Path) -> PathBuf {
        if module_name.is_absolute() {
            return module_name.to_path_buf();
        }
        let base = self.file_name.as_ref()
            .and_then(|f| Path::new(f).parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        base.join(module_name)
    }
}

fn v8_value_to_value(
    scope: &PinScope<'_, '_>,
    value: Local<'_, V8Value>,
) -> JsValue {
    let mut visited = HashSet::new();
    v8_value_to_value_impl(scope, value, &mut visited)
}

/// Convert an immediate (non-object) V8 value into a [`JsValue`].
fn v8_value_to_primitive_value(
    scope: &PinScope<'_, '_>,
    value: Local<'_, V8Value>,
) -> JsValue {
    if value.is_undefined() {
        return JsValue::UNDEFINED;
    }
    if value.is_null() {
        return JsValue::NULL;
    }
    if value.is_boolean() {
        let b = value.boolean_value(scope);
        return JsValue::from_bool(b);
    }
    if value.is_number() {
        let n = value.number_value(scope).unwrap_or(0.0);
        // Normalize integral numbers to the Int tag, like QuickJS does.
        if n.is_finite() && n.fract() == 0.0 && n >= i32::MIN as f64 && n <= i32::MAX as f64 {
            return JsValue::from_i32(n as i32);
        }
        return JsValue::from_f64(n);
    }
    if value.is_big_int() {
        let bi = value.to_big_int(scope).unwrap();
        let s = bi.to_string(scope).unwrap();
        let s = s.to_rust_string_lossy(scope);
        let parsed: num_bigint::BigInt = s.parse().unwrap_or_default();
        return JsValue::from_bigint(parsed);
    }
    if value.is_string() {
        let s = value.to_string(scope).unwrap();
        let s = s.to_rust_string_lossy(scope);
        return JsValue::from_string(s);
    }

    JsValue::UNDEFINED
}


fn v8_value_to_value_impl(
    scope: &PinScope<'_, '_>,
    value: Local<'_, V8Value>,
    visited: &mut HashSet<usize>, // dedup by object identity (pointer hash)
) -> JsValue {
    if value.is_object() {
        let obj = value.to_object(scope).unwrap();
        let identity = obj.get_identity_hash().get() as usize;

        // Cyclic reference: fall back to null.
        if visited.contains(&identity) {
            return JsValue::NULL;
        }
        visited.insert(identity);

        // Arrays
        if value.is_array() {
            let arr: Local<'_, Array> = value.try_into().unwrap();
            let len = arr.length() as usize;
            let mut vec = Vec::with_capacity(len);
            for i in 0..len {
                let elem = arr.get_index(scope, i as u32).unwrap();
                vec.push(v8_value_to_value_impl(scope, elem, visited));
            }
            visited.remove(&identity);
            return JsValue::from_array(vec);
        }

        // Ordinary objects
        let mut props = HashMap::<JsAtom, JsValue>::new();
        let own_keys = obj
            .get_own_property_names(
                scope,
                GetPropertyNamesArgs {
                    mode: KeyCollectionMode::OwnOnly,
                    ..Default::default()
                },
            )
            .unwrap();
        let key_len = own_keys.length() as usize;
        for i in 0..key_len {
            let key = own_keys.get_index(scope, i as u32).unwrap();
            if let Some(key_str) = key.to_string(scope) {
                let key_rust = key_str.to_rust_string_lossy(scope);
                let val = obj.get(scope, key).unwrap();
                props.insert(
                    JsAtom::from_str(&key_rust),
                    v8_value_to_value_impl(scope, val, visited),
                );
            }
        }
        visited.remove(&identity);
        let mut object = JsObject::new();
        object.set_props(props);
        JsValue::from_object(object)
    } else {
        v8_value_to_primitive_value(scope, value)
    }
}
