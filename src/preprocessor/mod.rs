use crate::runtime::Worker;

pub trait Compiled {
    /// get 阶段，返回阶段标识。
    fn get_stage(&self) -> u8;

    /// load 阶段：将已编译的代码交给 worker 处理。
    fn load<W: Worker + ?Sized>(&self, worker: &mut W);
}

impl<T: AsRef<str>> Compiled for T {
    /// blanket impl 的默认实现：总是返回 0。
    fn get_stage(&self) -> u8 {
        0
    }

    fn load<W: Worker + ?Sized>(&self, worker: &mut W) {
        worker.compile(self);
    }
}
