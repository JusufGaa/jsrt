/// Stage 1: parses the source code into an AST [`Program`] and maps out
/// its scopes (each with its own span and its parent scope's span).
mod scope;
mod stage;

pub use scope::{CodeItem, ScopeInfo};
pub use stage::Stage1;
