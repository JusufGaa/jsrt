//! QuickJS-inspired JavaScript value representation.
//!
//! A [`JsValue`] is a 16-byte tagged union: immediates (int, bool, null,
//! undefined, float64) are stored inline in the payload, while refcounted
//! heap values (strings, bigints, objects) store a pointer whose active
//! payload is selected by a negative [`JsTag`].

pub mod rc;
pub mod tag;
pub mod object;
pub mod value;

pub use object::JsObject;
pub use rc::RefCount;
pub use tag::JsTag;
pub use value::{JsValue, JsValuePayload};
