use std::collections::HashMap;

use super::builtin::JsInternal;
use super::rc::RefCount;
use super::value::JsValue;

/// Class tag (mirrors QuickJS `JS_CLASS_x`).
//
// repr(u8): 1 byte; struct header = 1 (class) + 7 (padding) + 8 (union).
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum JsClass {
    Object = 0,
    Array,
    Error,
    Date,
    RegExp,
    Map,
    Set,
    ArrayBuffer,
    /// Host extension, data in `JsInternal::Opaque`
    Opaque,
}

// Mutually exclusive per-class payload, like `JSObject.u` in QuickJS.
// `JsObject::class` decides which member is active. Rust union members
// must be `Copy`, so `Vec`/`HashMap` are carried as raw pointers into
// Box allocations. Only accessed inside this module.
#[derive(Clone, Copy)]
#[repr(C)]
union JsObjectU {
    /// `JsClass::Array`: indexed elements
    elements: *mut Vec<JsValue>,
    /// `JsClass::Object` / `JsClass::Error`: named properties
    props: *mut HashMap<String, JsValue>,
    /// Other built-in classes: private data (null = not set)
    internal: *mut JsInternal,
}

/// Heap representation of objects and arrays.
pub struct JsObject {
    class: JsClass,
    u: JsObjectU,
}

impl JsObject {
    pub(crate) fn new() -> Self {
        Self::with_class(JsClass::Object)
    }

    fn with_class(class: JsClass) -> Self {
        let u = match class {
            JsClass::Array => JsObjectU {
                elements: Box::into_raw(Box::new(Vec::new())),
            },
            JsClass::Object | JsClass::Error => JsObjectU {
                props: Box::into_raw(Box::new(HashMap::new())),
            },
            _ => JsObjectU {
                internal: std::ptr::null_mut(),
            },
        };
        Self { class, u }
    }

    pub(crate) fn array(items: Vec<JsValue>) -> Self {
        let mut obj = Self::with_class(JsClass::Array);
        *obj.elements_mut() = items;
        obj
    }

    pub(crate) fn is_array(&self) -> bool {
        self.class == JsClass::Array
    }

    // -- array elements (JsClass::Array) ------------------------------------

    /// # Panics
    /// Panics if the class is not `JsClass::Array`.
    pub(crate) fn elements(&self) -> &[JsValue] {
        assert!(self.is_array(), "elements are only valid for Array");
        unsafe { self.u.elements.as_ref().unwrap() }
    }

    /// # Panics
    /// Panics if the class is not `JsClass::Array`.
    pub(crate) fn elements_mut(&mut self) -> &mut Vec<JsValue> {
        assert!(self.is_array(), "elements are only valid for Array");
        unsafe { self.u.elements.as_mut().unwrap() }
    }

    // -- named properties (JsClass::Object / Error) -------------------------

    /// # Panics
    /// Panics if the class is not `Object` / `Error`.
    pub(crate) fn props(&self) -> &HashMap<String, JsValue> {
        assert!(
            matches!(self.class, JsClass::Object | JsClass::Error),
            "props are only valid for Object/Error"
        );
        unsafe { self.u.props.as_ref().unwrap() }
    }

    /// # Panics
    /// Panics if the class is not `Object` / `Error`.
    pub(crate) fn props_mut(&mut self) -> &mut HashMap<String, JsValue> {
        assert!(
            matches!(self.class, JsClass::Object | JsClass::Error),
            "props are only valid for Object/Error"
        );
        unsafe { self.u.props.as_mut().unwrap() }
    }

    /// Replaces the whole property table.
    pub(crate) fn set_props(&mut self, props: HashMap<String, JsValue>) {
        *self.props_mut() = props;
    }

    // -- built-in private data ----------------------------------------------

    /// # Panics
    /// Panics if the class is `Array` / `Object` / `Error`.
    pub(crate) fn internal(&self) -> Option<&JsInternal> {
        assert!(
            !matches!(
                self.class,
                JsClass::Array | JsClass::Object | JsClass::Error
            ),
            "internal data is only valid for built-in classes"
        );
        unsafe { self.u.internal.as_ref() }
    }

    /// Sets the private data, freeing the old one.
    ///
    /// # Panics
    /// Panics if the class is `Array` / `Object` / `Error`.
    pub(crate) fn set_internal(&mut self, internal: JsInternal) {
        assert!(
            !matches!(
                self.class,
                JsClass::Array | JsClass::Object | JsClass::Error
            ),
            "internal data is only valid for built-in classes"
        );
        unsafe {
            if let Some(old) = self.u.internal.as_mut() {
                old.free_payload();
            }
            self.u.internal = Box::into_raw(Box::new(internal));
        }
    }

    /// Box into a refcounted heap cell ready to be stored in a
    /// [`JsValue`](super::value::JsValue).
    pub(crate) fn into_cell(self) -> *mut () {
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
        unsafe {
            match self.class {
                JsClass::Array => {
                    // Free owned child values recursively.
                    let mut elements = Box::from_raw(self.u.elements);
                    for v in elements.drain(..) {
                        v.free();
                    }
                }
                JsClass::Object | JsClass::Error => {
                    let mut props = Box::from_raw(self.u.props);
                    for (_, v) in props.drain() {
                        v.free();
                    }
                }
                _ => {
                    if let Some(data) = self.u.internal.as_mut() {
                        data.free_payload();
                    }
                }
            }
        }
    }
}
