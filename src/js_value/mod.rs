//! QuickJS-inspired JavaScript value representation.
//!
//! A [`JsValue`] is a 16-byte tagged union: immediates (int, bool, null,
//! undefined, float64) are stored inline in the payload, while refcounted
//! heap values (strings, bigints, objects) store a pointer whose active
//! payload is selected by a negative tag.

mod tag;
mod builtin;
mod atom;
mod object;
mod rc;
mod symbol;
pub mod value;

pub use atom::JsAtom;
pub use object::*;
pub use symbol::JsSymbol;
pub use value::JsValue;
