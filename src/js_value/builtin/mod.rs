//! Built-in objects (Date, RegExp, Map, ...).

mod regexp;

use regexp::RegExpData;
use super::value::JsValue;

/// Private data for built-in classes, stored in `JsObject` via the
/// `internal` member of its payload union. Which variant is active is
/// decided by `JsClass`.
pub(crate) enum JsInternal {
    /// `Date`: millisecond timestamp (NaN = Invalid Date)
    Date(f64),
    /// `RegExp`: pattern + flags
    RegExp(RegExpData),
    /// `Map`: key-value pairs, insertion order
    Map(Vec<(JsValue, JsValue)>),
    /// `Set`: elements, insertion order
    Set(Vec<JsValue>),
    /// `ArrayBuffer` / `SharedArrayBuffer`: byte buffer
    ArrayBuffer(Vec<u8>),
    /// Host extension
    Opaque(*mut std::ffi::c_void),
}

impl JsInternal {
    /// Frees the held `JsValue`s (explicit free semantics, see `value.rs`).
    pub(crate) fn free_payload(&mut self) {
        match self {
            JsInternal::Map(entries) => {
                for (k, v) in entries.drain(..) {
                    k.free();
                    v.free();
                }
            }
            JsInternal::Set(items) => {
                for v in items.drain(..) {
                    v.free();
                }
            }
            _ => {}
        }
    }
}
