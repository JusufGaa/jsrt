use std::collections::HashMap;
use std::fmt;
use num_bigint::BigInt;

use super::object::JsObject;
use super::rc::RefCount;
use super::tag::JsTag;

/// Untagged payload of a [`JsValue`].
///
/// Which field is active is determined by the `tag` of the enclosing value.
/// Module-private on purpose: reading the wrong union member is UB, so
/// all access must go through `JsValue`'s checked accessors.
#[repr(C)]
#[derive(Clone, Copy)]
union JsValuePayload {
    int32: i32,
    uint32: u32,
    float64: f64,
    /// Refcounted heap cell (`RefCount<String>`, `RefCount<BigInt>`,
    /// `RefCount<JsObject>`), selected by a negative tag.
    ptr: *mut (),
}

/// A JavaScript value: 16-byte tagged union, mirroring QuickJS `JSValue`.
///
/// `JsValue` is `Copy`, so refcounted payloads are **not** automatically
/// duplicated: use [`JsValue::clone_value`] / [`JsValue::free`] to manage
/// reference counts (QuickJS `JS_DupValue` / `JS_FreeValue`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct JsValue {
    u: JsValuePayload,
    tag: i64,
}

impl JsValue {
    pub const NULL: JsValue = JsValue {
        u: JsValuePayload { int32: 0 },
        tag: JsTag::Null as i64,
    };

    pub const UNDEFINED: JsValue = JsValue {
        u: JsValuePayload { int32: 0 },
        tag: JsTag::Undefined as i64,
    };

    /// Uninitialized slot marker (QuickJS `JS_TAG_UNINITIALIZED`).
    pub const UNINITIALIZED: JsValue = JsValue {
        u: JsValuePayload { int32: 0 },
        tag: JsTag::Uninitialized as i64,
    };

    /// Exception marker with no payload.
    pub const EXCEPTION: JsValue = JsValue {
        u: JsValuePayload { int32: 0 },
        tag: JsTag::Exception as i64,
    };

    pub fn from_i32(v: i32) -> Self {
        JsValue {
            u: JsValuePayload { int32: v },
            tag: JsTag::Int as i64,
        }
    }

    pub fn from_u32(v: u32) -> Self {
        JsValue {
            u: JsValuePayload { uint32: v },
            tag: JsTag::Int as i64,
        }
    }

    pub fn from_bool(b: bool) -> Self {
        JsValue {
            u: JsValuePayload { int32: b as i32 },
            tag: JsTag::Bool as i64,
        }
    }

    pub fn from_f64(v: f64) -> Self {
        JsValue {
            u: JsValuePayload { float64: v },
            tag: JsTag::Float64 as i64,
        }
    }

    /// Box `s` into a refcounted heap cell (`RefCount<String>`).
    pub fn from_string(s: impl Into<String>) -> Self {
        JsValue {
            u: JsValuePayload {
                ptr: RefCount::<String>::new(s.into()) as *mut (),
            },
            tag: JsTag::String as i64,
        }
    }

    /// Box `v` into a refcounted heap cell (`RefCount<BigInt>`).
    pub fn from_bigint(v: BigInt) -> Self {
        JsValue {
            u: JsValuePayload {
                ptr: RefCount::<BigInt>::new(v) as *mut (),
            },
            tag: JsTag::BigInt as i64,
        }
    }

    /// Box `obj` into a refcounted heap cell (`RefCount<JsObject>`).
    pub fn from_object(obj: JsObject) -> Self {
        JsValue {
            u: JsValuePayload {
                ptr: obj.into_cell(),
            },
            tag: JsTag::Object as i64,
        }
    }

    /// Create an array object holding `items`.
    pub fn from_array(items: Vec<JsValue>) -> Self {
        Self::from_object(JsObject::array(items))
    }

    /// Read the tag.
    ///
    /// # Safety
    /// The stored tag must always come from `JsTag as i64`.
    #[inline]
    pub(crate) fn tag(&self) -> JsTag {
        unsafe { std::mem::transmute(self.tag) }
    }

    /// True when the payload is a refcounted pointer, i.e. a negative tag.
    #[inline]
    pub fn is_refcounted(&self) -> bool {
        self.tag < 0
    }

    pub fn is_undefined(&self) -> bool {
        self.tag == JsTag::Undefined as i64
    }

    pub fn is_null(&self) -> bool {
        self.tag == JsTag::Null as i64
    }

    pub fn is_boolean(&self) -> bool {
        self.tag == JsTag::Bool as i64
    }

    /// True for both `Int` and `Float64` tags.
    pub fn is_number(&self) -> bool {
        self.tag == JsTag::Int as i64 || self.tag == JsTag::Float64 as i64
    }

    pub fn is_string(&self) -> bool {
        self.tag == JsTag::String as i64
    }

    pub fn is_bigint(&self) -> bool {
        self.tag == JsTag::BigInt as i64
    }

    pub fn is_object(&self) -> bool {
        self.tag == JsTag::Object as i64
    }

    pub fn as_i32(&self) -> Option<i32> {
        if self.tag == JsTag::Int as i64 {
            Some(unsafe { self.u.int32 })
        } else {
            None
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        if self.tag == JsTag::Int as i64 {
            Some(unsafe { self.u.uint32 })
        } else {
            None
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        if self.tag == JsTag::Float64 as i64 {
            Some(unsafe { self.u.float64 })
        } else {
            None
        }
    }

    /// Numeric view of any number tag (`Int` widens to f64).
    pub fn as_number(&self) -> Option<f64> {
        match self.tag() {
            JsTag::Int => self.as_i32().map(|v| v as f64),
            JsTag::Float64 => self.as_f64(),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        if self.tag == JsTag::Bool as i64 {
            Some(unsafe { self.u.int32 } != 0)
        } else {
            None
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        if self.tag == JsTag::String as i64 {
            Some(unsafe { RefCount::<String>::data(self.u.ptr as *mut _) })
        } else {
            None
        }
    }

    pub fn as_bigint(&self) -> Option<&BigInt> {
        if self.tag == JsTag::BigInt as i64 {
            Some(unsafe { RefCount::<BigInt>::data(self.u.ptr as *mut _) })
        } else {
            None
        }
    }

    pub fn as_object(&self) -> Option<&JsObject> {
        if self.tag == JsTag::Object as i64 {
            Some(unsafe { RefCount::<JsObject>::data(self.u.ptr as *mut _) })
        } else {
            None
        }
    }

    /// Mutable object access.
    ///
    /// # Safety
    /// The caller must ensure no aliasing borrow of the same object.
    pub unsafe fn as_object_mut(&self) -> Option<&mut JsObject> {
        if self.tag == JsTag::Object as i64 {
            Some(unsafe { RefCount::<JsObject>::data_mut(self.u.ptr as *mut _) })
        } else {
            None
        }
    }

    /// Duplicate the value, incrementing the refcount of refcounted
    /// payloads (QuickJS `JS_DupValue`).
    pub fn clone_value(&self) -> Self {
        if self.is_refcounted() {
            unsafe {
                match self.tag() {
                    JsTag::String => RefCount::<String>::inc(self.u.ptr as *mut _),
                    JsTag::BigInt => RefCount::<BigInt>::inc(self.u.ptr as *mut _),
                    JsTag::Object => RefCount::<JsObject>::inc(self.u.ptr as *mut _),
                    _ => {}
                }
            }
        }
        *self
    }

    /// Release the value, decrementing the refcount and freeing the
    /// payload when it reaches zero (QuickJS `JS_FreeValue`).
    pub fn free(self) {
        if !self.is_refcounted() {
            return;
        }
        unsafe {
            match self.tag() {
                JsTag::String => RefCount::<String>::dec(self.u.ptr as *mut _),
                JsTag::BigInt => RefCount::<BigInt>::dec(self.u.ptr as *mut _),
                JsTag::Object => RefCount::<JsObject>::dec(self.u.ptr as *mut _),
                _ => {}
            }
        }
    }
}

impl fmt::Display for JsValue {
    /// JS-like stringification (same rules as `String(v)`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.tag() {
            JsTag::Int => write!(f, "{}", unsafe { self.u.int32 }),
            JsTag::Bool => f.write_str(if unsafe { self.u.int32 } != 0 { "true" } else { "false" }),
            JsTag::Null => f.write_str("null"),
            JsTag::Undefined | JsTag::Uninitialized => f.write_str("undefined"),
            JsTag::Float64 => {
                let n = unsafe { self.u.float64 };
                if n.is_nan() {
                    f.write_str("NaN")
                } else if n.is_infinite() {
                    f.write_str(if n > 0.0 { "Infinity" } else { "-Infinity" })
                } else {
                    write!(f, "{}", n)
                }
            }
            JsTag::String => f.write_str(self.as_string().unwrap_or("")),
            JsTag::BigInt => write!(f, "{}", self.as_bigint().map_or_else(String::new, |b| b.to_string())),
            JsTag::Object => match self.as_object() {
                Some(obj) if obj.is_array() => {
                    f.write_str("[")?;
                    for (i, item) in obj.elements().iter().enumerate() {
                        if i > 0 {
                            f.write_str(",")?;
                        }
                        // `Array.prototype.join` prints null/undefined as empty.
                        match item.tag() {
                            JsTag::Null | JsTag::Undefined | JsTag::Uninitialized => {}
                            _ => fmt::Display::fmt(item, f)?,
                        }
                    }
                    f.write_str("]")
                }
                _ => f.write_str("[object Object]"),
            },
            JsTag::Exception => f.write_str("[exception]"),
            _ => f.write_str("<unsupported tag>"),
        }
    }
}

impl fmt::Debug for JsValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.tag() {
            JsTag::String => write!(
                f,
                "String({})",
                crate::utils::format_string(self.as_string().unwrap_or(""), '"')
            ),
            JsTag::Object => match self.as_object() {
                Some(obj) if obj.is_array() => {
                    f.debug_list().entries(obj.elements().iter()).finish()
                }
                _ => f.write_str(&self.to_string()),
            },
            JsTag::Symbol => f.write_str("Symbol()"),
            _ => fmt::Display::fmt(self, f),
        }
    }
}

/// Convenience constructor mirroring the old JSON-tree API: build an
/// object directly from string-keyed properties.
impl From<HashMap<String, JsValue>> for JsValue {
    fn from(props: HashMap<String, JsValue>) -> Self {
        let mut obj = JsObject::new();
        obj.set_props(props);
        JsValue::from_object(obj)
    }
}
