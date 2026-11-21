use std::collections::HashMap;
use std::mem::ManuallyDrop;

use super::utils::{
    TAG_MIX_PRI_OBJECT, TAG_MIX_PT_OBJECT, TAG_PRI_ARRAY, TAG_PT_ARRAY, TAG_STR_PRI_OBJECT,
    TAG_STR_PT_OBJECT, TAG_SYM_PRI_OBJECT, TAG_SYM_PT_OBJECT,
};
use crate::value::primitive_value::PrimitiveValue;
use crate::value::symbol::Symbol;

/// Union payload of [`PriPt`]; only the field selected by the tag is active.
#[repr(C)]
union PriPtData {
    /// Active when the tag is [`TAG_MIX_PRI_OBJECT`]: object with mixed
    /// string/symbol keys, primitive values.
    mix_pri_object:
        ManuallyDrop<(HashMap<String, PrimitiveValue>, HashMap<&'static Symbol, PrimitiveValue>)>,
    /// Active when the tag is [`TAG_MIX_PT_OBJECT`]: object with mixed
    /// string/symbol keys, pointer slots.
    mix_pt_object: ManuallyDrop<(HashMap<String, usize>, HashMap<&'static Symbol, usize>)>,
    /// Active when the tag is [`TAG_STR_PRI_OBJECT`]: object with string
    /// keys only, primitive values.
    str_pri_object: ManuallyDrop<HashMap<String, PrimitiveValue>>,
    /// Active when the tag is [`TAG_STR_PT_OBJECT`]: object with string
    /// keys only, pointer slots.
    str_pt_object: ManuallyDrop<HashMap<String, usize>>,
    /// Active when the tag is [`TAG_SYM_PRI_OBJECT`]: object with symbol
    /// keys only, primitive values.
    sym_pri_object: ManuallyDrop<HashMap<&'static Symbol, PrimitiveValue>>,
    /// Active when the tag is [`TAG_SYM_PT_OBJECT`]: object with symbol
    /// keys only, pointer slots.
    sym_pt_object: ManuallyDrop<HashMap<&'static Symbol, usize>>,
    /// Active when the tag is [`TAG_PRI_ARRAY`].
    pri_array: ManuallyDrop<Vec<PrimitiveValue>>,
    /// Active when the tag is [`TAG_PT_ARRAY`].
    pt_array: ManuallyDrop<Vec<usize>>,
}

/// A heap collection in one of four pure storage flavors: primitive
/// objects/arrays (`pri_*`) hold only raw primitive values, pointer
/// objects/arrays (`pt_*`) hold only pointer slots.
pub(crate) struct PriPt {
    /// One of the `TAG_*` constants above; always matches the active field.
    tag: u8,
    data: PriPtData,
}

impl PriPt {
    /// Returns the tag masked with the low four bits (`00001111`).
    #[inline]
    fn get_tag(&self) -> u8 {
        self.tag & 0b00001111
    }

    /// Whether the active flavor stores pointer slots (`pt_*` flavors).
    pub(crate) fn is_point(&self) -> bool {
        matches!(
            self.get_tag(),
            TAG_MIX_PT_OBJECT | TAG_STR_PT_OBJECT | TAG_SYM_PT_OBJECT | TAG_PT_ARRAY
        )
    }

    /// Whether the active flavor stores raw primitive values (`pri_*`
    /// flavors).
    pub(crate) fn is_primitive_value(&self) -> bool {
        matches!(
            self.get_tag(),
            TAG_MIX_PRI_OBJECT | TAG_STR_PRI_OBJECT | TAG_SYM_PRI_OBJECT | TAG_PRI_ARRAY
        )
    }
}

impl From<(HashMap<String, PrimitiveValue>, HashMap<&'static Symbol, PrimitiveValue>)> for PriPt {
    fn from(
        (strings, symbols): (
            HashMap<String, PrimitiveValue>,
            HashMap<&'static Symbol, PrimitiveValue>,
        ),
    ) -> Self {
        Self {
            tag: TAG_MIX_PRI_OBJECT,
            data: PriPtData {
                mix_pri_object: ManuallyDrop::new((strings, symbols)),
            },
        }
    }
}

impl From<(HashMap<String, usize>, HashMap<&'static Symbol, usize>)> for PriPt {
    fn from(
        (strings, symbols): (HashMap<String, usize>, HashMap<&'static Symbol, usize>),
    ) -> Self {
        Self {
            tag: TAG_MIX_PT_OBJECT,
            data: PriPtData {
                mix_pt_object: ManuallyDrop::new((strings, symbols)),
            },
        }
    }
}

impl From<HashMap<String, PrimitiveValue>> for PriPt {
    fn from(map: HashMap<String, PrimitiveValue>) -> Self {
        Self {
            tag: TAG_STR_PRI_OBJECT,
            data: PriPtData {
                str_pri_object: ManuallyDrop::new(map),
            },
        }
    }
}

impl From<HashMap<String, usize>> for PriPt {
    fn from(map: HashMap<String, usize>) -> Self {
        Self {
            tag: TAG_STR_PT_OBJECT,
            data: PriPtData {
                str_pt_object: ManuallyDrop::new(map),
            },
        }
    }
}

impl From<HashMap<&'static Symbol, PrimitiveValue>> for PriPt {
    fn from(map: HashMap<&'static Symbol, PrimitiveValue>) -> Self {
        Self {
            tag: TAG_SYM_PRI_OBJECT,
            data: PriPtData {
                sym_pri_object: ManuallyDrop::new(map),
            },
        }
    }
}

impl From<HashMap<&'static Symbol, usize>> for PriPt {
    fn from(map: HashMap<&'static Symbol, usize>) -> Self {
        Self {
            tag: TAG_SYM_PT_OBJECT,
            data: PriPtData {
                sym_pt_object: ManuallyDrop::new(map),
            },
        }
    }
}

impl From<Vec<PrimitiveValue>> for PriPt {
    fn from(items: Vec<PrimitiveValue>) -> Self {
        Self {
            tag: TAG_PRI_ARRAY,
            data: PriPtData {
                pri_array: ManuallyDrop::new(items),
            },
        }
    }
}

impl From<Vec<usize>> for PriPt {
    fn from(items: Vec<usize>) -> Self {
        Self {
            tag: TAG_PT_ARRAY,
            data: PriPtData {
                pt_array: ManuallyDrop::new(items),
            },
        }
    }
}

impl Drop for PriPt {
    fn drop(&mut self) {
        // SAFETY: only the field selected by the tag owns heap data, and
        // dropping the struct consumes it, so this runs exactly once.
        unsafe {
            match self.get_tag() {
                TAG_MIX_PRI_OBJECT => ManuallyDrop::drop(&mut self.data.mix_pri_object),
                TAG_MIX_PT_OBJECT => ManuallyDrop::drop(&mut self.data.mix_pt_object),
                TAG_STR_PRI_OBJECT => ManuallyDrop::drop(&mut self.data.str_pri_object),
                TAG_STR_PT_OBJECT => ManuallyDrop::drop(&mut self.data.str_pt_object),
                TAG_SYM_PRI_OBJECT => ManuallyDrop::drop(&mut self.data.sym_pri_object),
                TAG_SYM_PT_OBJECT => ManuallyDrop::drop(&mut self.data.sym_pt_object),
                TAG_PRI_ARRAY => ManuallyDrop::drop(&mut self.data.pri_array),
                TAG_PT_ARRAY => ManuallyDrop::drop(&mut self.data.pt_array),
            }
        }
    }
}
