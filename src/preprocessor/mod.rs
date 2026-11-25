// use crate::runtime::Worker;
pub mod stage1;

pub trait Stage {
    /// Returns the stage identifier.
    fn stage(&self) -> u8;

    // /// The load stage: hands the compiled code over to a worker for processing.
    // fn load<W: Worker + ?Sized>(&self, worker: &mut W);
}

impl<T: AsRef<str>> Stage for T {
    /// Default implementation of the blanket impl: always returns 0.
    fn stage(&self) -> u8 {
        0
    }
}
