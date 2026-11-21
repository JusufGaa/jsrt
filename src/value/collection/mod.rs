use std::collections::HashMap;

use self::pri_pt::PriPt;
use super::value_or_point::ValueOrPoint;

mod pri_pt;
mod utils;

/// A heap-allocated collection type.
///
/// [`PriPt`] packs the four primitive/pointer storage flavors into one
/// manually tagged union; the mixed flavors stay plain enum variants.
pub(crate) enum Collection {
    PriPt(PriPt),
    Object(HashMap<String, ValueOrPoint>),
    Array(Vec<ValueOrPoint>),
}
