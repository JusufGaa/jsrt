use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, OnceLock};

static SYMBOL_REGISTRY: OnceLock<Mutex<HashMap<String, &'static Symbol>>> = OnceLock::new();
static SYMBOL_POOL: OnceLock<Mutex<Vec<&'static Symbol>>> = OnceLock::new();

pub struct Symbol {
    description: Option<String>,
}

impl Symbol {
    pub fn new() -> &'static Symbol {
        Self::intern(None)
    }

    pub fn new_with_description(description: impl Into<String>) -> &'static Symbol {
        Self::intern(Some(description.into()))
    }

    pub fn r#for(key: impl Into<String>) -> &'static Symbol {
        let key = key.into();
        let registry = SYMBOL_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()));
        let mut registry = registry.lock().unwrap();
        if let Some(&symbol) = registry.get(&key) {
            return symbol;
        }
        let symbol: &'static Symbol = Box::leak(Box::new(Symbol {
            description: Some(key.clone()),
        }));
        registry.insert(key, symbol);
        symbol
    }

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

    fn intern(description: Option<String>) -> &'static Symbol {
        let pool = SYMBOL_POOL.get_or_init(|| Mutex::new(Vec::new()));
        let mut pool = pool.lock().unwrap();
        let symbol: &'static Symbol = Box::leak(Box::new(Symbol { description }));
        pool.push(symbol);
        symbol
    }
}

impl fmt::Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.description {
            Some(d) => write!(f, "Symbol({})", d),
            None => f.write_str("Symbol()"),
        }
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl PartialEq for Symbol {
    fn eq(&self, other: &Symbol) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for Symbol {}
