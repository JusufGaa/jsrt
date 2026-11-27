/// Stage 2: builds on the stage-1 [`Program`] (analysis of functions,
/// purity and globals).
mod function;
mod globals;
mod stage;

pub use function::{Callee, FunctionInfo, Purity};
pub use globals::GlobalInfo;
pub use stage::Stage2;
