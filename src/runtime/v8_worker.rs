use std::path::{Path, PathBuf};
use v8::{
    Global, OwnedIsolate, PinScope, Context,
    Local, Value as V8Value, Array, GetPropertyNamesArgs,
    KeyCollectionMode,
};
use std::collections::HashMap;
use crate::js_value::{JsAtom, JsObject, JsValue};
use crate::preprocessor::Stage;

use super::global_this::{
    GlobalThis, GlobalThisSnapshot,
    graph_replay, replay_state, replay_statements, resolve_snapshot_index,
};

pub struct V8Worker {
    file_name: Option<String>,
    cached_code: Vec<Box<dyn Stage>>,
    isolate: OwnedIsolate,
    context: Global<Context>,
    /// History of snapshots (one per `save`).
    snapshots: Vec<GlobalThisSnapshot>,
    /// Tracked current state of `globalThis`.
    global_this: GlobalThis,
    /// Last error message (exception message + stack) produced by a
    /// failed eval.
    last_error: Option<String>,
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
            last_error: None,
        }
    }

    /// Last error message (exception message + stack) produced by a failed eval.
    pub fn take_error(&mut self) -> Option<String> {
        self.last_error.take()
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
        let origin = origin.into();

        let (result, error) = self.with_scope(|scope, _| {
            // Capture exceptions in a TryCatch so they are swallowed silently
            // instead of being reported as uncaught errors by V8. The error
            // is recorded on the worker (`take_error`) instead of being lost.
            v8::tc_scope!(let tc, scope);

            let resource_name =
                v8::String::new(&tc, file_name.as_deref().unwrap_or("repl"));
            let Some(resource_name) = resource_name else {
                return (None, None);
            };
            if let Some(code) = v8::String::new(&tc, code) {
                if let Some(script) = match origin {
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
                        return (Some(v8_value_to_value(&tc, result.into())), None)
                    }
                }
            }
            (None, format_tc_exception(&tc))
        });

        self.last_error = error;
        result
    }
}

/// Format the exception caught by a [`v8::TryCatch`] into a message
/// (exception message + stack), mirroring `format_caught_error` in the
/// QuickJS worker.
fn format_tc_exception(
    tc: &v8::PinnedRef<'_, v8::TryCatch<'_, '_, v8::HandleScope<'_>>>,
) -> Option<String> {
    let exception = tc.exception()?;
    let message = exception
        .to_string(tc)
        .map(|s| s.to_rust_string_lossy(tc))
        .unwrap_or_else(|| "Uncaught exception".to_string());
    let stack = tc
        .stack_trace()
        .and_then(|s| s.to_string(tc))
        .map(|s| s.to_rust_string_lossy(tc))
        .filter(|s| !s.is_empty());
    Some(match stack {
        Some(stack) => format!("{message}\n{stack}"),
        None => message,
    })
}

impl super::Worker for V8Worker {
    fn save(&mut self) {
        let global_value = self.with_scope(|scope, ctx| {
            let global = ctx.global(scope);
            v8_value_to_value(scope, global.into())
        });

        // Current `globalThis` state: variable name -> value. `dup_value`
        // detaches the entries from the `global_value` tree, which is
        // dropped (and frees its children) when this function returns.
        let mut current = HashMap::<String, JsValue>::new();
        if let Some(obj) = global_value.as_object() {
            for (key, value) in obj.props() {
                if let Some(name) = key.as_str() {
                    current.insert(name.to_string(), value.dup_value());
                }
            }
        }

        // Diff against the tracked state to build a snapshot of operations.
        let mut snapshot = GlobalThisSnapshot::new();
        for (name, value) in &current {
            match self.global_this.get(name) {
                None => snapshot.insert(name.clone(), *value),
                Some(old) if old.to_source() != value.to_source() => {
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

    fn clean(&mut self) {
        self.context = Self::new_context(&mut self.isolate);
    }

    fn load(&mut self, code: impl Stage + 'static) {
        self.cached_code.push(Box::new(code));
    }

    fn run(&mut self) -> JsValue {
        let code = std::mem::take(&mut self.cached_code)
            .into_iter()
            .map(|stage| (&*stage).get_code())
            .collect::<Vec<_>>()
            .join("\n");
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

    fn write_prelude(&mut self, name: impl AsRef<str>, value: JsValue) {
        debug_assert!(
            !value.is_object() && !value.is_symbol(),
            "write_prelude only accepts literal values \
             (undefined / null / boolean / number / string / bigint); \
             use replay_prelude for non-literal source"
        );

        // NaN / ±Infinity degrade to a replay of their source form
        // (Number.NaN / Number.POSITIVE_INFINITY / Number.NEGATIVE_INFINITY).
        if value.is_number() && !value.as_number().unwrap_or(0.0).is_finite() {
            self.replay_prelude(name, value.to_source());
            return;
        }

        self.with_scope(|scope, ctx| {
            let key = v8::String::new(scope, name.as_ref()).unwrap();
            let native = js_value_to_v8(scope, value);
            ctx.global(scope).set(scope, key.into(), native).unwrap();
        });
        self.global_this.insert(name.as_ref().to_string(), value);
    }

    fn replay_prelude(&mut self, name: impl AsRef<str>, source: impl AsRef<str>) {
        let key = serde_json::to_string(name.as_ref()).unwrap();
        let code = format!("globalThis[{}] = ({});", key, source.as_ref());
        if let Some(value) = self.eval_with_origin(&code, None) {
            if !value.is_undefined() {
                self.global_this.insert(name.as_ref().to_string(), value);
            }
        }
    }

    fn read_global(&mut self, name: impl AsRef<str>) -> JsValue {
        self.with_scope(|scope, ctx| {
            let global = ctx.global(scope);
            let key = v8::String::new(scope, name.as_ref()).unwrap();
            let value = global.get(scope, key.into()).unwrap();
            v8_value_to_value(scope, value)
        })
    }

    fn replay_graph(&mut self, entries: &[(String, JsValue)]) {
        let code = graph_replay(entries);
        if !code.is_empty() {
            self.eval_with_origin(&code, V8ScriptOrigin::default());
        }
    }

    fn snapshot_go(&mut self, index: isize) -> GlobalThis {
        let Some(end) = resolve_snapshot_index(self.snapshots.len(), index) else {
            // Out of range: leave everything untouched, return a copy
            // of the current tracked state.
            let mut state = GlobalThis::new();
            for (name, value) in self.global_this.iter() {
                state.insert(name.clone(), value.dup_value());
            }
            return state;
        };

        // Fresh context, then replay the history up to the target snapshot.
        self.clean();
        let code = replay_statements(&self.snapshots, end);
        if !code.is_empty() {
            self.eval_with_origin(&code, None);
        }

        // History is truncated at the target; later snapshots are dropped.
        let state = replay_state(&self.snapshots, end);
        self.snapshots.truncate(end + 1);

        let mut tracked = GlobalThis::new();
        for (name, value) in state.iter() {
            tracked.insert(name.clone(), value.dup_value());
        }
        self.global_this = tracked;
        state
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
    let mut cache = HashMap::new();
    v8_value_to_value_impl(scope, value, &mut cache)
}

/// Convert a settable literal [`JsValue`] (undefined / null / boolean /
/// finite number / string / bigint) into a native V8 value. NaN /
/// ±Infinity numbers and non-literals are degraded to replays by the
/// caller before reaching this function.
fn js_value_to_v8<'s>(scope: &mut PinScope<'s, '_>, value: JsValue) -> Local<'s, V8Value> {
    if value.is_null() {
        return v8::null(scope).into();
    }
    if value.is_undefined() {
        return v8::undefined(scope).into();
    }
    if value.is_boolean() {
        return v8::Boolean::new(scope, value.as_bool().unwrap_or(false)).into();
    }
    if value.is_number() {
        return v8::Number::new(scope, value.as_number().unwrap_or(0.0)).into();
    }
    if value.is_string() {
        return v8::String::new(scope, value.as_string().unwrap_or("")).unwrap().into();
    }
    if value.is_bigint() {
        // V8 supports arbitrary-precision BigInts via sign + 64-bit words.
        let big = value.as_bigint().unwrap();
        let (sign, digits) = big.to_u64_digits();
        return v8::BigInt::new_from_words(scope, sign == num_bigint::Sign::Minus, &digits)
            .unwrap()
            .into();
    }
    v8::undefined(scope).into()
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


/// Convert a V8 value into a [`JsValue`], preserving object identity:
/// the same engine object — including cyclic and shared references —
/// always maps to the same [`JsValue`], tracked in `cache` keyed by the
/// engine identity hash.
fn v8_value_to_value_impl(
    scope: &PinScope<'_, '_>,
    value: Local<'_, V8Value>,
    cache: &mut HashMap<usize, JsValue>,
) -> JsValue {
    if value.is_object() {
        let obj = value.to_object(scope).unwrap();
        let identity = obj.get_identity_hash().get() as usize;

        // Shared or cyclic reference: reuse the first conversion.
        if let Some(&shared) = cache.get(&identity) {
            return shared;
        }

        // Arrays: register before walking the contents so cycles
        // resolve to the same value.
        if value.is_array() {
            let arr: Local<'_, Array> = value.try_into().unwrap();
            let len = arr.length() as usize;
            let shared = JsValue::from_array(Vec::new());
            cache.insert(identity, shared);
            let mut vec = Vec::with_capacity(len);
            for i in 0..len {
                let elem = arr.get_index(scope, i as u32).unwrap();
                vec.push(v8_value_to_value_impl(scope, elem, cache));
            }
            unsafe {
                *shared.as_object_mut().unwrap().elements_mut() = vec;
            }
            return shared;
        }

        // Ordinary objects
        let shared = JsValue::from_object(JsObject::new());
        cache.insert(identity, shared);
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
                    v8_value_to_value_impl(scope, val, cache),
                );
            }
        }
        unsafe {
            shared.as_object_mut().unwrap().set_props(props);
        }
        shared
    } else {
        v8_value_to_primitive_value(scope, value)
    }
}
