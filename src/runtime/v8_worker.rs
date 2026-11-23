use std::path::{Path, PathBuf};
use v8::{
    Global, OwnedIsolate, PinScope, Context,
    Local, Value as V8Value, Array, GetPropertyNamesArgs,
    KeyCollectionMode,
};
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use crate::js_value::{JsObject, JsValue};

pub struct V8Worker {
    file_name: Option<String>,
    cached_code: Vec<String>,
    isolate: OwnedIsolate,
    context: Global<Context>,
    snapshots: Vec<JsonValue>,
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

impl V8Worker {
    pub fn new() -> Self {
        let mut isolate = v8::Isolate::new(Default::default());
    
        let context = {
            let mut pinned =
                std::pin::pin!(v8::HandleScope::<Context>::new(&mut isolate));
            let scope = pinned.as_mut().init();

            let ctx = v8::Context::new(&scope, Default::default());
            Global::new(&scope, ctx)
        };

        V8Worker {
            file_name: None,
            cached_code: Vec::new(),
            isolate,
            context,
            snapshots: Vec::new(),
        }
    }

    fn eval_with_origin(&mut self, code: &str, origin: impl Into<Option<V8ScriptOrigin>>) -> Option<JsValue> {
        let isolate = &mut self.isolate;
        let context = &self.context;

        let mut pinned =
                std::pin::pin!(v8::HandleScope::new(isolate));
        let mut scope = pinned.as_mut().init();

        let ctx = Global::<Context>::new(&scope, context);
        let ctx = Local::new(&scope, ctx);

        let mut ctx_scope = v8::ContextScope::new(&mut scope, ctx);
        let mut pinned2 = std::pin::pin!(v8::HandleScope::new(&mut ctx_scope));
        let scope2 = pinned2.as_mut().init();
        let resource_name = if let Some(ref file_name) = self.file_name {
            v8::String::new(&scope2, &*file_name)
        } else {
            v8::String::new(&scope2, "repl")
        };
        match resource_name {
            None => return  None,
            _ => {}
        }
        if let Some(code) = v8::String::new(&scope2, code) {
            if let Some(script) = match origin.into() {
                Some(origin) => {
                    let origin = v8::ScriptOrigin::new(&scope2,
                        resource_name.unwrap().into(), 
                        origin.resource_line_offset, 
                        origin.resource_column_offset, 
                        origin.resource_is_shared_cross_origin, 
                        origin.script_id, 
                        None, 
                        origin.resource_is_opaque, 
                        origin.is_wasm, 
                        origin.is_module, 
                        None);
                    v8::Script::compile(&scope2, code, Some(&origin))
                },
                _ => v8::Script::compile(&scope2, code, None)
            } {
                if let Some(result) = script.run(&scope2) {
                    return Some(v8_value_to_value(&scope2, result.into()))
                }
            }
        }
        None
    }
}

impl super::Worker for V8Worker {
    fn save(&mut self) {
        let isolate = &mut self.isolate;
        let context = &self.context;

        let mut pinned =
            std::pin::pin!(v8::HandleScope::new(isolate));
        let mut scope = pinned.as_mut().init();

        let ctx = Local::new(&scope, context);
        let mut ctx_scope = v8::ContextScope::new(&mut scope, ctx);
        let mut pinned2 = std::pin::pin!(v8::HandleScope::new(&mut ctx_scope));
        let scope2 = pinned2.as_mut().init();

        let global = ctx.global(&scope2);
        let snapshot = v8_value_to_json(&scope2, global.into());

        self.snapshots.push(snapshot);
    }

    fn restore(&mut self) {
        let context = {
            let isolate = &mut self.isolate;
            let mut pinned =
                std::pin::pin!(v8::HandleScope::<Context>::new(isolate));
            let scope = pinned.as_mut().init();

            let ctx = v8::Context::new(&scope, Default::default());
            Global::new(&scope, ctx)
        };
        self.context = context;

        let t = self.snapshots.pop();
        if let Some(snap) = t {
            let code = snap.as_object().unwrap().iter().map(|(key, value)| {
                let key = serde_json::to_string(key).unwrap();
                format!("globalThis[{}] = {};", key, value)
            }).reduce(|a, b| format!("{}\n{}", a, b)).unwrap();

            self.eval_with_origin(&*code, None);
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
        self.eval_with_origin(&code, V8ScriptOrigin {
            resource_line_offset: 0,
            resource_column_offset: 0,
            resource_is_shared_cross_origin: false,
            script_id: -1,
            resource_is_opaque: true,
            is_wasm: false,
            is_module: false,
        }).unwrap_or(JsValue::NULL)
    }

    fn import(&mut self, module_name: impl AsRef<Path>) -> JsValue {
        let path = self.resolve_module_path(module_name.as_ref());
        let code = match std::fs::read_to_string(&path) {
            Ok(code) => code,
            Err(_) => return JsValue::NULL,
        };

        let prev_file_name = self.file_name.take();
        self.file_name = Some(path.display().to_string());
        let result = self.eval_with_origin(&code, V8ScriptOrigin {
            resource_line_offset: 0,
            resource_column_offset: 0,
            resource_is_shared_cross_origin: false,
            script_id: -1,
            resource_is_opaque: true,
            is_wasm: false,
            is_module: false,
        });
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

fn v8_value_to_json(
    scope: &PinScope<'_, '_>, 
    value: Local<'_, V8Value>,
) -> JsonValue {
    if let Some(str) = v8::json::stringify(scope, value) {
        let str = str.to_rust_string_lossy(scope);
        if let Ok(result) = serde_json::from_str(&*str) {
            return result;
        }
    }

    JsonValue::Null
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
        let mut props = HashMap::<String, JsValue>::new();
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
                    key_rust,
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
