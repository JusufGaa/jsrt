pub use crate::value;
pub mod global_this;

use std::path::Path;

pub trait Worker {
    fn save(&mut self);
    fn restore(&mut self);
    fn load(&mut self, code: impl AsRef<str>);
    fn run(&mut self) -> value::Value;
    fn import(&mut self, module_name: impl AsRef<Path>) -> value::Value;
    fn eval(&mut self, code: impl AsRef<str>) -> value::Value {
        self.load(code);
        self.run()
    }
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
pub mod v8_worker;

#[macro_export]
macro_rules! init {
    () => {
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
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
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        {
            $crate::runtime::v8_worker::V8Worker::new()
        }
    }};
}
