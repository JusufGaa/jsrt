/// Tag constants of the [`PriPt`](super::pri_pt::PriPt) union; each value
/// identifies exactly one active union field. Kept here so the whole
/// `collection` module tree can share them.

/// Active field is `mix_pri_object` (`(HashMap<String, PrimitiveValue>,
/// HashMap<&'static Symbol, PrimitiveValue>)`): object with mixed
/// string/symbol keys, primitive values.
pub(crate) const TAG_MIX_PRI_OBJECT: u8 = 0;
/// Active field is `mix_pt_object` (`(HashMap<String, usize>,
/// HashMap<&'static Symbol, usize>)`): object with mixed string/symbol
/// keys, pointer slots.
pub(crate) const TAG_MIX_PT_OBJECT: u8 = 1;
/// Active field is `str_pri_object` (`HashMap<String, PrimitiveValue>`):
/// object with string keys only, primitive values.
pub(crate) const TAG_STR_PRI_OBJECT: u8 = 2;
/// Active field is `str_pt_object` (`HashMap<String, usize>`): object with
/// string keys only, pointer slots.
pub(crate) const TAG_STR_PT_OBJECT: u8 = 3;
/// Active field is `sym_pri_object` (`HashMap<&'static Symbol,
/// PrimitiveValue>`): object with symbol keys only, primitive values.
pub(crate) const TAG_SYM_PRI_OBJECT: u8 = 4;
/// Active field is `sym_pt_object` (`HashMap<&'static Symbol, usize>`):
/// object with symbol keys only, pointer slots.
pub(crate) const TAG_SYM_PT_OBJECT: u8 = 5;
/// Active field is `pri_array` (`Vec<PrimitiveValue>`).
pub(crate) const TAG_PRI_ARRAY: u8 = 6;
/// Active field is `pt_array` (`Vec<usize>`).
pub(crate) const TAG_PT_ARRAY: u8 = 7;
