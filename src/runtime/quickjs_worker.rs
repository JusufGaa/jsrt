use std::collections::HashMap;
use std::path::{Path, PathBuf};

use num_bigint::BigInt;
use rquickjs::context::EvalOptions;
use rquickjs::{
    CatchResultExt, CaughtError, Context, Ctx, Runtime, Type, Value,
};

use crate::js_value::{JsAtom, JsObject, JsValue};
use crate::preprocessor::Stage;

use super::global_this::{
    GlobalThis, GlobalThisSnapshot,
    graph_replay, replay_state, replay_statements, resolve_snapshot_index,
};
use super::Worker;

/// QuickJS (rquickjs) implementation of [`Worker`].
pub struct QuickJsWorker {
    file_name: Option<String>,
    /// Code chunks taken at `load` time, evaluated at `run` time.
    code: Vec<String>,
    runtime: Runtime,
    context: Context,
    /// History of snapshots (one per `save`).
    snapshots: Vec<GlobalThisSnapshot>,
    /// Tracked current state of `globalThis`.
    global_this: GlobalThis,
    last_error: Option<String>,
}

impl QuickJsWorker {
    pub fn new() -> Self {
        let runtime = Runtime::new().expect("failed to create QuickJS runtime");
        let context = Context::full(&runtime).expect("failed to create QuickJS context");

        QuickJsWorker {
            file_name: None,
            code: Vec::new(),
            runtime,
            context,
            snapshots: Vec::new(),
            global_this: GlobalThis::new(),
            last_error: None,
        }
    }

    /// Evaluate code in the global scope, converting the result to a [`JsValue`].
    fn eval_code(&mut self, code: &str, filename: Option<&str>) -> Option<JsValue> {
        let Self {
            context,
            file_name,
            last_error,
            ..
        } = self;

        let filename = filename
            .map(|f| f.to_string())
            .or_else(|| file_name.clone())
            .unwrap_or_else(|| "repl".to_string());

        let mut options = EvalOptions::default();
        options.filename = Some(filename);
        // Match V8 (non-strict) semantics like the v8_worker does.
        options.strict = false;

        let (result, error) = context.with(|ctx| match ctx.eval_with_options::<Value, _>(code, options).catch(&ctx) {
            Ok(value) => (Some(rq_value_to_value(&ctx, value)), None),
            Err(err) => (None, Some(format_caught_error(&ctx, err))),
        });

        *last_error = error;
        result
    }

    /// Last error message (exception message + stack) produced by a failed eval.
    pub fn take_error(&mut self) -> Option<String> {
        self.last_error.take()
    }

    fn resolve_module_path(&self, module_name: &Path) -> PathBuf {
        if module_name.is_absolute() {
            return module_name.to_path_buf();
        }
        let base = self
            .file_name
            .as_ref()
            .and_then(|f| Path::new(f).parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        base.join(module_name)
    }
}

impl Default for QuickJsWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl Worker for QuickJsWorker {
    fn save(&mut self) {
        let global_value = self.context.with(|ctx| {
            let global = ctx.globals();
            rq_value_to_value(&ctx, global.as_value().clone())
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
        let context = Context::full(&self.runtime).expect("failed to create QuickJS context");
        self.context = context;
    }

    fn load(&mut self, code: impl Stage) {
        self.code.extend(code.get_code().map(str::to_string));
    }

    fn run(&mut self) -> JsValue {
        let chunks = std::mem::take(&mut self.code);
        let mut last = JsValue::NULL;
        for chunk in chunks {
            match self.eval_code(&chunk, None) {
                Some(value) => last = value,
                // A runtime error stops the remaining chunks.
                None => break,
            }
        }
        last
    }

    fn import(&mut self, module_name: impl AsRef<Path>) -> JsValue {
        let path = self.resolve_module_path(module_name.as_ref());
        let code = match std::fs::read_to_string(&path) {
            Ok(code) => code,
            Err(_) => return JsValue::NULL,
        };

        let prev_file_name = self.file_name.take();
        self.file_name = Some(path.display().to_string());
        let result = self.eval_code(&code, Some(path.to_string_lossy().as_ref()));
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

        // Degrade to a replay of the source form when the engine cannot
        // set the literal directly in a lossless way:
        // - NaN / ±Infinity numbers (Number.NaN / Number.POSITIVE_INFINITY /
        //   Number.NEGATIVE_INFINITY);
        // - any bigint (rquickjs only offers 64-bit constructors).
        if value.is_number() && !value.as_number().unwrap_or(0.0).is_finite() {
            self.replay_prelude(name, value.to_source());
            return;
        }
        if value.is_bigint() {
            self.replay_prelude(name, value.to_source());
            return;
        }

        self.context.with(|ctx| {
            let native = js_value_to_rquickjs(&ctx, value);
            ctx.globals().set(name.as_ref(), native).unwrap();
        });
        self.global_this.insert(name.as_ref().to_string(), value);
    }

    fn replay_prelude(&mut self, name: impl AsRef<str>, source: impl AsRef<str>) {
        let key = serde_json::to_string(name.as_ref()).unwrap();
        let code = format!("globalThis[{}] = ({});", key, source.as_ref());
        if let Some(value) = self.eval_code(&code, None) {
            if !value.is_undefined() {
                self.global_this.insert(name.as_ref().to_string(), value);
            }
        }
    }

    fn read_global(&mut self, name: impl AsRef<str>) -> JsValue {
        self.context.with(|ctx| {
            let global = ctx.globals();
            let value: Value = global.get(name.as_ref()).unwrap();
            rq_value_to_value(&ctx, value)
        })
    }

    fn replay_graph(&mut self, entries: &[(String, JsValue)]) {
        let code = graph_replay(entries);
        if !code.is_empty() {
            self.eval_code(&code, None);
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
            self.eval_code(&code, None);
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

/// Raw QuickJS pointer of a value, used for cycle detection by identity.
fn value_identity(value: &Value) -> usize {
    unsafe { value.as_raw().u.ptr as usize }
}

/// Convert a directly settable literal [`JsValue`] (undefined / null /
/// boolean / number / string) into a native QuickJS value.
fn js_value_to_rquickjs<'js>(ctx: &Ctx<'js>, value: JsValue) -> Value<'js> {
    if value.is_null() {
        return Value::new_null(ctx.clone());
    }
    if value.is_undefined() {
        return Value::new_undefined(ctx.clone());
    }
    if value.is_boolean() {
        return Value::new_bool(ctx.clone(), value.as_bool().unwrap_or(false));
    }
    if let Some(i) = value.as_i32() {
        return Value::new_int(ctx.clone(), i);
    }
    if let Some(n) = value.as_f64() {
        return Value::new_float(ctx.clone(), n);
    }
    if value.is_string() {
        return rquickjs::String::from_str(ctx.clone(), value.as_string().unwrap_or(""))
            .map(|s| s.into_value())
            .unwrap_or_else(|_| Value::new_undefined(ctx.clone()));
    }
    Value::new_undefined(ctx.clone())
}

/// Convert a rquickjs value into a [`JsValue`], preserving object
/// identity: the same engine object — including cyclic and shared
/// references — always maps to the same [`JsValue`].
fn rq_value_to_value(ctx: &Ctx, value: Value) -> JsValue {
    let mut cache = HashMap::new();
    rq_value_to_value_impl(ctx, value, &mut cache)
}

/// `cache` maps engine object identity (raw pointer) to the first
/// [`JsValue`] conversion, so shared / cyclic references resolve to the
/// same value.
fn rq_value_to_value_impl(ctx: &Ctx, value: Value, cache: &mut HashMap<usize, JsValue>) -> JsValue {
    match value.type_of() {
        Type::Undefined | Type::Uninitialized => JsValue::UNDEFINED,
        Type::Null => JsValue::NULL,
        Type::Bool => JsValue::from_bool(value.get::<bool>().unwrap_or(false)),
        Type::Int => JsValue::from_i32(value.get::<i32>().unwrap_or(0)),
        Type::Float => {
            let n = value.get::<f64>().unwrap_or(0.0);
            if n.is_finite() && n.fract() == 0.0 && n >= i32::MIN as f64 && n <= i32::MAX as f64 {
                JsValue::from_i32(n as i32)
            } else {
                JsValue::from_f64(n)
            }
        }
        Type::String => JsValue::from_string(value.get::<String>().unwrap_or_default()),
        // Truncated to 64-bit, matching the number of bits a QuickJS bigint
        // fits into an i64 for this simple wrapper.
        Type::BigInt => {
            let n: Option<i64> = value
                .into_big_int()
                .and_then(|b| b.to_i64().ok());
            JsValue::from_bigint(n.map(BigInt::from).unwrap_or_default())
        }
        Type::Array => {
            let identity = value_identity(&value);
            if let Some(&shared) = cache.get(&identity) {
                return shared;
            }

            // Register before walking the contents so cycles resolve
            // to the same value.
            let shared = JsValue::from_array(Vec::new());
            cache.insert(identity, shared);
            let mut items = Vec::new();
            if let Some(arr) = value.as_array() {
                for i in 0..arr.len() {
                    match arr.get::<Value>(i) {
                        Ok(item) => items.push(rq_value_to_value_impl(ctx, item, cache)),
                        Err(_) => items.push(JsValue::NULL),
                    }
                }
            }
            unsafe {
                *shared.as_object_mut().unwrap().elements_mut() = items;
            }
            shared
        }
        // Everything else (plain objects, functions, promises, ...) is
        // flattened into an object of its own enumerable string properties.
        _ => {
            let identity = value_identity(&value);
            if let Some(&shared) = cache.get(&identity) {
                return shared;
            }

            let shared = JsValue::from_object(JsObject::new());
            cache.insert(identity, shared);
            let mut props = HashMap::<JsAtom, JsValue>::new();
            if let Some(obj) = value.as_object() {
                for key in obj.keys::<String>().flatten() {
                    if let Ok(item) = obj.get::<_, Value>(&key) {
                        props.insert(
                            JsAtom::from_str(&key),
                            rq_value_to_value_impl(ctx, item, cache),
                        );
                    }
                }
            }
            unsafe {
                shared.as_object_mut().unwrap().set_props(props);
            }
            shared
        }
    }
}

/// Format a caught error (including thrown JS exceptions) into a message.
fn format_caught_error(ctx: &Ctx, err: CaughtError) -> String {
    match err {
        CaughtError::Exception(e) => {
            let message = e.message().unwrap_or_default();
            let stack = e.stack().unwrap_or_default();
            if stack.is_empty() {
                message
            } else {
                format!("{message}\n{stack}")
            }
        }
        CaughtError::Value(v) => rq_value_to_value(ctx, v).to_string(),
        CaughtError::Error(e) => e.to_string(),
    }
}
