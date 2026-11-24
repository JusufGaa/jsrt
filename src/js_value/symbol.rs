//! ECMAScript Symbol primitive (ECMA-262 6.1.5).
//!
//! Symbols are interned for the whole program lifetime (`Box::leak`),
//! so identity is simply the heap address: `Symbol("x") !==
//! Symbol("x")`, while the same `&JsSymbol` value is always equal to
//! itself. Because symbols are immortal, no refcounting is involved and
//! they can be compared/hashed directly by address. This mirrors the
//! original jsrt implementation.

use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};

/// `Symbol.for(key)` global registry: description -> interned symbol.
static SYMBOL_REGISTRY: OnceLock<Mutex<HashMap<String, &'static JsSymbol>>> =
    OnceLock::new();

/// Keeps every interned symbol alive (they are leaked anyway); useful
/// for iteration/debugging.
static SYMBOL_POOL: OnceLock<Mutex<Vec<&'static JsSymbol>>> = OnceLock::new();

pub struct JsSymbol {
    description: Option<String>,
}

impl JsSymbol {
    /// Anonymous symbol (`Symbol()`).
    #[allow(clippy::new_without_default)]
    pub fn new() -> &'static Self {
        Self::intern(None)
    }

    /// `Symbol(description)`.
    pub fn new_with_description(description: impl Into<String>) -> &'static Self {
        Self::intern(Some(description.into()))
    }

    /// `Symbol.for(key)`: global registry keyed by description; always
    /// returns the same symbol for the same key.
    pub fn r#for(key: impl Into<String>) -> &'static Self {
        let key = key.into();
        let registry = SYMBOL_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()));
        let mut registry = registry.lock().unwrap();
        if let Some(&symbol) = registry.get(&key) {
            return symbol;
        }
        let symbol: &'static Self =
            Box::leak(Box::new(Self { description: Some(key.clone()) }));
        registry.insert(key, symbol);
        symbol
    }

    /// Returns the registry key if this symbol is the `Symbol.for`
    /// singleton registered for its description.
    pub fn key_for(self: &'static Self) -> Option<String> {
        if self.description.is_none() {
            return None;
        }
        let registry = SYMBOL_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()));
        let registry = registry.lock().unwrap();
        let desc = self.description.as_ref().unwrap();
        if let Some(sym) = registry.get(desc) {
            if std::ptr::eq(*sym, self) {
                return Some(desc.clone());
            }
        }
        None
    }

    fn intern(description: Option<String>) -> &'static Self {
        let pool = SYMBOL_POOL.get_or_init(|| Mutex::new(Vec::new()));
        let mut pool = pool.lock().unwrap();
        let symbol: &'static Self = Box::leak(Box::new(Self { description }));
        pool.push(symbol);
        symbol
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
}

impl fmt::Debug for JsSymbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.description {
            Some(d) => write!(f, "Symbol({})", d),
            None => f.write_str("Symbol()"),
        }
    }
}

impl fmt::Display for JsSymbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// Symbols are only ever compared by identity (heap address); the
/// description is *not* part of equality (`Symbol("x") !== Symbol("x")`).
impl PartialEq for JsSymbol {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for JsSymbol {}

impl Hash for JsSymbol {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (std::ptr::from_ref::<JsSymbol>(self) as usize).hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_internings_are_never_equal() {
        let a = JsSymbol::new_with_description("x");
        let b = JsSymbol::new_with_description("x");
        assert_ne!(a, b);
        assert_ne!(a, JsSymbol::new());
    }

    #[test]
    fn same_reference_is_equal_and_hashes_alike() {
        let a = JsSymbol::new_with_description("x");
        assert_eq!(a, a);
        assert_eq!(hash_of(a), hash_of(a));
    }

    #[test]
    fn symbol_for_is_a_singleton_per_key() {
        let a = JsSymbol::r#for("iter");
        let b = JsSymbol::r#for("iter");
        assert_eq!(a, b);
        assert_eq!(a.key_for(), Some("iter".to_string()));
        // r#for result is distinct from a plain description symbol.
        assert_ne!(a, JsSymbol::new_with_description("iter"));
        assert_eq!(JsSymbol::new().key_for(), None);
    }

    fn hash_of(s: &JsSymbol) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        s.hash(&mut h);
        std::hash::Hasher::finish(&h)
    }
}
