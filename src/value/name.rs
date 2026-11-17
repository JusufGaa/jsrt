use std::fmt;

use super::symbol::Symbol;

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Name {
    String(String),
    Symbol(&'static Symbol),
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Name::String(s) => f.write_str(&super::primitive_value::format_string(s, Some('"'))),
            Name::Symbol(s) => write!(f, "{}", s),
        }
    }
}
