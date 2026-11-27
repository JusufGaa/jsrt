// use crate::runtime::Worker;
pub mod stage1;
pub mod stage2;
pub mod stage3;

pub trait Stage {
    /// Returns the stage identifier.
    fn stage(&self) -> u8;

    // /// The load stage: hands the compiled code over to a worker for processing.
    // fn load<W: Worker + ?Sized>(&self, worker: &mut W);
    /// The code chunks this stage contributes; each chunk is compiled
    /// (and later run) separately.
    fn get_code(&self) -> impl Iterator<Item = &str>;
}

impl<T: AsRef<str>> Stage for T {
    /// Default implementation of the blanket impl: always returns 0.
    fn stage(&self) -> u8 {
        0
    }

    fn get_code(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.as_ref())
    }
}
