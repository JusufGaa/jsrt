use std::fmt;

use super::name::Name;
use crate::value::symbol::Symbol;

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Index {
    String(String),
    Symbol(&'static Symbol),
    Index(usize),
}

impl fmt::Display for Index {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Index::String(s) => f.write_str(&crate::utils::format_string(s, '"')),
            Index::Symbol(s) => write!(f, "{}", s),
            Index::Index(i) => write!(f, "{}", i),
        }
    }
}

impl From<Name> for Index {
    fn from(name: Name) -> Index {
        match name {
            Name::String(s) => Index::String(s),
            Name::Symbol(s) => Index::Symbol(s),
        }
    }
}
