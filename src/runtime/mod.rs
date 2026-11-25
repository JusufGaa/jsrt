pub use crate::js_value;
pub mod global_this;

use std::path::Path;

use self::global_this::GlobalThis;
use crate::js_value::JsValue;
use crate::preprocessor::Stage;

pub trait Worker {
    fn save(&mut self);
    /// Reset the engine to a brand-new context: all current script
    /// state is discarded (snapshots / tracked state are untouched).
    fn clean(&mut self);
    fn load(&mut self, code: impl Stage + 'static);
    fn run(&mut self) -> JsValue;
    fn import(&mut self, module_name: impl AsRef<Path>) -> JsValue;

    fn write_prelude(&mut self, name: impl AsRef<str>, value: JsValue);
    fn replay_prelude(&mut self, name: impl AsRef<str>, source: impl AsRef<str>);

    /// Replays a batch of values into the context as one object graph,
    /// preserving shared references and cycles. Must bypass the pending
    /// `load` cache and evaluate immediately.
    fn replay_graph(&mut self, entries: &[(String, JsValue)]);

    /// Reads the value bound to `name` on `globalThis` from the
    /// underlying context. Returns [`JsValue::UNDEFINED`] when the
    /// binding does not exist.
    fn read_global(&mut self, name: impl AsRef<str>) -> JsValue;

    /// Snapshot go: jump to the snapshot at `index` by replaying the
    /// history up to that point (history is truncated at the target).
    ///
    /// Positive `index` counts from the front, negative counts back from
    /// the end (`snapshots.len() + index`, Python-style). Returns the
    /// tracked [`GlobalThis`] state at that position; an out-of-range
    /// index leaves the worker untouched and returns the current state.
    fn snapshot_go(&mut self, index: isize) -> GlobalThis;

    /// Restore the latest snapshot: jump back with `snapshot_go(-1)`
    /// to pop the history and get the tracked state, then re-inject
    /// it into a fresh context: literals (undefined / null / booleans /
    /// numbers / strings / bigint) go through `write_prelude` — each
    /// worker degrades to an internal `replay_prelude` when a value
    /// cannot be set directly in a lossless way (NaN / ±Infinity on
    /// both engines; bigint on QuickJS) — everything else (objects,
    /// symbols) through `replay_graph` as one batch, so shared
    /// references and cycles survive the restore.
    fn restore(&mut self) {
        let state = self.snapshot_go(-1);
        self.clean();
        let mut graph = Vec::new();
        for (name, value) in state.iter() {
            if value.is_number()
                || value.is_string()
                || value.is_boolean()
                || value.is_null()
                || value.is_undefined()
                || value.is_bigint()
            {
                self.write_prelude(name, *value);
            } else {
                graph.push((name.clone(), *value));
            }
        }
        if !graph.is_empty() {
            self.replay_graph(&graph);
        }
    }

    fn eval(&mut self, code: impl AsRef<str> + 'static) -> JsValue {
        self.load(code);
        self.run()
    }
}

/// V8 worker, the default runtime wherever `v8` is a dependency
/// and the `quickjs` feature is not enabled.
#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "android",
    all(target_os = "windows", target_env = "gnu"),
    feature = "quickjs"
)))]
pub mod v8_worker;

/// QuickJS worker, only compiled when the `quickjs` feature is enabled
/// and `rquickjs` is a dependency.
#[cfg(all(feature = "quickjs", not(any(target_os = "macos", target_os = "ios"))))]
pub mod quickjs_worker;

#[macro_export]
macro_rules! init {
    () => {
        #[cfg(not(any(target_os = "macos", target_os = "ios", all(target_os = "windows", target_env = "gnu"), feature = "quickjs")))]
        {
            let platform = v8::new_default_platform(0, false).make_shared();
            v8::V8::initialize_platform(platform.clone());
            v8::V8::initialize();
        }
    }
}

#[macro_export]
macro_rules! worker {
    () => {{
        #[cfg(not(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "android",
            all(target_os = "windows", target_env = "gnu"),
            feature = "quickjs"
        )))]
        {
            $crate::runtime::v8_worker::V8Worker::new()
        }
        // Only when the `quickjs` feature is enabled and `rquickjs` is
        // available does the QuickJS worker get compiled.
        #[cfg(all(feature = "quickjs", not(any(target_os = "macos", target_os = "ios"))))]
        {
            $crate::runtime::quickjs_worker::QuickJsWorker::new()
        }
    }};
}
