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
                    k.free_value();
                    v.free_value();
                }
            }
            JsInternal::Set(items) => {
                for v in items.drain(..) {
                    v.free_value();
                }
            }
            _ => {}
        }
    }

    /// Replay source: serialize the private data back into equivalent JS
    /// source, so the built-in object can be replayed when restoring a
    /// snapshot.
    ///
    /// Returns `None` when there is no faithful source form (`Opaque`
    /// host data); the caller decides how to fall back.
    pub(crate) fn to_source(&self) -> Option<String> {
        match self {
            JsInternal::Date(ms) => Some(format!("new Date({})", ms)),

            JsInternal::RegExp(data) => {
                Some(format!("/{}/{}", data.source, data.flags_string()))
            }

            JsInternal::Map(entries) => {
                let pairs: Vec<String> = entries
                    .iter()
                    .map(|(k, v)| format!("[{}, {}]", k.to_source(), v.to_source()))
                    .collect();
                Some(format!("new Map([{}])", pairs.join(", ")))
            }

            JsInternal::Set(items) => {
                let elems: Vec<String> = items.iter().map(JsValue::to_source).collect();
                Some(format!("new Set([{}])", elems.join(", ")))
            }

            // Replay via `Uint8Array` to keep the byte contents; take
            // `.buffer` so the class matches the original `ArrayBuffer`.
            JsInternal::ArrayBuffer(bytes) => {
                let elems: Vec<String> = bytes.iter().map(|b| b.to_string()).collect();
                Some(format!("new Uint8Array([{}]).buffer", elems.join(",")))
            }

            JsInternal::Opaque(_) => None,
        }
    }
}
