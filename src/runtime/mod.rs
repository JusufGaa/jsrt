pub use crate::value;

pub(crate) trait WorkerPrivate {
    fn set_file_name(&mut self, file_name: impl Into<String>) ;
}

pub trait Worker: WorkerPrivate {
    fn save(&mut self);
    fn restore(&mut self);
    fn eval(&mut self, code: impl Into<String>) -> value::Value;
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
    ($file:expr $(,)?) => {{ 
        let worker = worker!();
        worker.eval(include_str!($file));
        worker.save();
        worker
    }};
}
