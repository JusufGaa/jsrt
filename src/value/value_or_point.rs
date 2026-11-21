use super::primitive_value::PrimitiveValue;

/// An element stored inside a
/// [`Collection`](crate::value::collection::Collection): either a plain
/// primitive value or a pointer to another heap slot.
#[derive(Clone)]
pub(crate) enum ValueOrPoint {
    Value(PrimitiveValue),
    Point(usize),
}

impl From<PrimitiveValue> for ValueOrPoint {
    fn from(value: PrimitiveValue) -> Self {
        Self::Value(value)
    }
}

impl From<usize> for ValueOrPoint {
    fn from(point: usize) -> Self {
        Self::Point(point)
    }
}
