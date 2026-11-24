pub use crate::js_value;
pub mod global_this;

use std::path::Path;

use crate::js_value::JsValue;
use crate::preprocessor::Compiled;

pub trait Worker {
    fn save(&mut self);
    fn restore(&mut self);
    fn compile(&mut self, code: impl AsRef<str>);
    fn run(&mut self) -> JsValue;
    fn import(&mut self, module_name: impl AsRef<Path>) -> JsValue;
    fn load_compiled(&mut self, code: impl Compiled) {
        code.load(self);
    }
    fn eval(&mut self, code: impl AsRef<str>) -> JsValue {
        self.compile(code);
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
