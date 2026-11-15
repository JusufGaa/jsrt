use std::fmt;
use std::sync::{Mutex, OnceLock};

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

impl PartialOrd for Symbol {
    fn partial_cmp(&self, other: &Symbol) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Symbol {
    fn cmp(&self, other: &Symbol) -> std::cmp::Ordering {
        std::ptr::from_ref::<Symbol>(self).cmp(&std::ptr::from_ref::<Symbol>(other))
    }
}

impl PartialEq for Symbol {
    fn eq(&self, other: &Symbol) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for Symbol {}
