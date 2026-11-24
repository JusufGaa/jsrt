use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use num_bigint::BigInt;
use rquickjs::context::EvalOptions;
use rquickjs::{
    CatchResultExt, CaughtError, Context, Ctx, Runtime, Type, Value,
};

use crate::js_value::{JsAtom, JsObject, JsValue};

use super::global_this::{GlobalThis, GlobalThisSnapshot, js_value_to_source, operation_statements};
use super::Worker;

/// QuickJS (rquickjs) implementation of [`Worker`].
pub struct QuickJsWorker {
    file_name: Option<String>,
    cached_code: Vec<String>,
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
            cached_code: Vec::new(),
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
        // Reset to a fresh context, then replay the latest snapshot.
        let context = Context::full(&self.runtime).expect("failed to create QuickJS context");
        self.context = context;

        if let Some(popped) = self.snapshots.pop() {
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
                self.eval_code(&code, None);
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
        self.eval_code(&code, None).unwrap_or(JsValue::NULL)
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
}

/// Raw QuickJS pointer of a value, used for cycle detection by identity.
fn value_identity(value: &Value) -> usize {
    unsafe { value.as_raw().u.ptr as usize }
}

/// Convert a rquickjs value into a [`JsValue`], with cycle detection.
fn rq_value_to_value(ctx: &Ctx, value: Value) -> JsValue {
    let mut visited = HashSet::new();
    rq_value_to_value_impl(ctx, value, &mut visited)
}

fn rq_value_to_value_impl(ctx: &Ctx, value: Value, visited: &mut HashSet<usize>) -> JsValue {
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
            if visited.contains(&identity) {
                return JsValue::NULL;
            }
            visited.insert(identity);

            let mut items = Vec::new();
            if let Some(arr) = value.as_array() {
                for i in 0..arr.len() {
                    match arr.get::<Value>(i) {
                        Ok(item) => items.push(rq_value_to_value_impl(ctx, item, visited)),
                        Err(_) => items.push(JsValue::NULL),
                    }
                }
            }
            visited.remove(&identity);
            JsValue::from_array(items)
        }
        // Everything else (plain objects, functions, promises, ...) is
        // flattened into an object of its own enumerable string properties.
        _ => {
            let identity = value_identity(&value);
            if visited.contains(&identity) {
                return JsValue::NULL;
            }
            visited.insert(identity);

            let mut props = HashMap::<JsAtom, JsValue>::new();
            if let Some(obj) = value.as_object() {
                for key in obj.keys::<String>().flatten() {
                    if let Ok(item) = obj.get::<_, Value>(&key) {
                        props.insert(
                            JsAtom::from_str(&key),
                            rq_value_to_value_impl(ctx, item, visited),
                        );
                    }
                }
            }
            visited.remove(&identity);
            let mut object = JsObject::new();
            object.set_props(props);
            JsValue::from_object(object)
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
