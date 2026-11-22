use std::collections::HashMap;

use super::rc::RefCount;
use super::value::JsValue;

/// Heap representation of objects and arrays.
///
/// Arrays keep indexed elements in `elements` (like QuickJS array exotic
/// objects); named properties live in `props`.
pub struct JsObject {
    /// Whether the object was created as an array (`[]`).
    pub is_array: bool,
    /// Indexed elements, only meaningful for arrays.
    pub elements: Vec<JsValue>,
    /// Named properties, keyed by string.
    pub props: HashMap<String, JsValue>,
}

impl JsObject {
    pub fn new() -> Self {
        Self {
            is_array: false,
            elements: Vec::new(),
            props: HashMap::new(),
        }
    }

    pub fn array(items: Vec<JsValue>) -> Self {
        Self {
            is_array: true,
            elements: items,
            props: HashMap::new(),
        }
    }

    /// Box into a refcounted heap cell ready to be stored in a
    /// [`JsValue`](super::value::JsValue).
    pub fn into_cell(self) -> *mut () {
        RefCount::new(self) as *mut _
    }
}

impl Default for JsObject {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for JsObject {
    fn drop(&mut self) {
        // Free owned child values recursively.
        for v in self.elements.drain(..) {
            v.free();
        }
        for (_, v) in self.props.drain() {
            v.free();
        }
    }
}
