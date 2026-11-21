use serde_json::{
    Number as JsonNumber,
    Value as JsonValue,
};
use std::fmt;

use super::symbol::Symbol;

#[derive(Clone)]
pub enum PrimitiveValue {
    Bigint(num_bigint::BigInt),
    Boolean(bool),
    Null,
    Number(JsonNumber),
    String(String),
    Symbol(&'static Symbol),
    Undefined,
}

impl From<JsonValue> for PrimitiveValue {
    fn from(value: JsonValue) -> Self {
        match value {
            JsonValue::Null => PrimitiveValue::Null,
            JsonValue::Bool(b) => PrimitiveValue::Boolean(b),
            JsonValue::Number(n) => PrimitiveValue::Number(n),
            JsonValue::String(s) => PrimitiveValue::String(s),
            _ => PrimitiveValue::Undefined,
        }
    }
}

impl fmt::Display for PrimitiveValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrimitiveValue::Bigint(n) => write!(f, "{}", n),
            PrimitiveValue::Boolean(b) => write!(f, "{}", b),
            PrimitiveValue::Null => f.write_str("null"),
            PrimitiveValue::Number(n) => write!(f, "{}", n),
            PrimitiveValue::String(s) => f.write_str(&crate::utils::format_string(s, None)),
            PrimitiveValue::Symbol(s) => write!(f, "{}", s),
            PrimitiveValue::Undefined => f.write_str("undefined"),
        }
    }
}

impl PrimitiveValue {
    pub(crate) fn fmt_json(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrimitiveValue::String(s) => f.write_str(&crate::utils::format_string(s, '"')),
            other => fmt::Display::fmt(other, f),
        }
    }
}

