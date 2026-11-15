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
            PrimitiveValue::String(s) => f.write_str(&format_string(s, None)),
            PrimitiveValue::Symbol(s) => write!(f, "{}", s),
            PrimitiveValue::Undefined => f.write_str("undefined"),
        }
    }
}

impl PrimitiveValue {
    pub(crate) fn fmt_json(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrimitiveValue::String(s) => f.write_str(&format_string(s, Some('"'))),
            other => fmt::Display::fmt(other, f),
        }
    }
}

pub(crate) fn format_string(s: &str, quote: Option<char>) -> String {
    debug_assert!(
        matches!(quote, None | Some('\'') | Some('"')),
        "quote must be None, single quote, or double quote"
    );
    let quote = quote.unwrap_or_else(|| {
        let has_single = s.contains('\'');
        let has_double = s.contains('"');
        if has_single && !has_double { '"' } else { '\'' }
    });

    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}
