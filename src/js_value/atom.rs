//! Interned property keys, mirroring QuickJS `JSAtom`.
//!
//! ECMA-262 4.3.30: an object property key is either a **String** or a
//! **Symbol**. QuickJS interns both kinds into a global atom table and
//! references them through a 32-bit `JSAtom` handle, so property lookup
//! and key comparison reduce to a single `u32` compare and equal string
//! keys (`obj.a` / `obj["a"]`) share one entry. This module mirrors that
//! design.
//!
//! Simplification: QuickJS reclaims table slots when an atom's refcount
//! drops to zero; here entries are interned permanently for the thread
//! lifetime, which keeps `as_str` / `as_symbol` returning stable
//! references without refcount juggling between the table and `JsValue`.
//! Symbols are additionally immortal themselves (`Box::leak`, see
//! `symbol.rs`), so interning them needs no extra bookkeeping.

use std::cell::RefCell;
use std::collections::HashMap;

use super::symbol::JsSymbol;
use super::tag::JsTag;
use super::value::JsValue;

/// Interned property key: a handle into the thread-local atom table.
///
/// `Copy` + `Eq` + `Hash` all reduce to the inner `u32`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct JsAtom(u32);

/// One interned key.
///
/// `Str` keeps the interned text; `Sym` keeps the interned symbol,
/// which lives for the whole program (`Box::leak`), so the address is
/// a stable identity key.
enum AtomEntry {
    Str(Box<str>),
    Sym(&'static JsSymbol),
}

#[derive(Default)]
struct AtomTable {
    /// `Box`ed entries: addresses stay stable while the vector grows.
    entries: Vec<Box<AtomEntry>>,
    /// string content -> atom (interning by value)
    by_str: HashMap<Box<str>, u32>,
    /// symbol address -> atom (interning by identity)
    by_sym: HashMap<usize, u32>,
    next: u32,
}

thread_local! {
    /// One table per thread: atoms never cross threads, mirroring one
    /// QuickJS runtime context per thread.
    static ATOM_TABLE: RefCell<AtomTable> = RefCell::new(AtomTable::default());
}

impl JsAtom {
    /// Intern a string key by content.
    pub fn from_str(s: &str) -> Self {
        ATOM_TABLE.with(|t| {
            let mut t = t.borrow_mut();
            if let Some(&id) = t.by_str.get(s) {
                return JsAtom(id);
            }
            let id = t.next;
            t.next += 1;
            let key: Box<str> = s.into();
            t.by_str.insert(key.clone(), id);
            t.entries.push(Box::new(AtomEntry::Str(key)));
            JsAtom(id)
        })
    }

    /// Intern a symbol key by its identity (heap address). Two symbols
    /// are the same atom iff they are the same symbol value
    /// (ECMA-262 6.1.5).
    pub(crate) fn from_symbol(sym: &'static JsSymbol) -> Self {
        ATOM_TABLE.with(|t| {
            let mut t = t.borrow_mut();
            let key = std::ptr::from_ref::<JsSymbol>(sym) as usize;
            if let Some(&id) = t.by_sym.get(&key) {
                return JsAtom(id);
            }
            let id = t.next;
            t.next += 1;
            t.by_sym.insert(key, id);
            t.entries.push(Box::new(AtomEntry::Sym(sym)));
            JsAtom(id)
        })
    }

    /// String content when this is a string key, `None` for symbol keys.
    ///
    /// # Safety
    /// Interned entries are `Box`ed inside the thread-local table and
    /// never mutated or removed afterwards, so the returned `&'static str`
    /// is valid for the rest of the thread lifetime.
    pub fn as_str(&self) -> Option<&'static str> {
        ATOM_TABLE.with(|t| {
            let t = t.borrow();
            match t.entries.get(self.0 as usize)?.as_ref() {
                AtomEntry::Str(s) => Some(unsafe { &*(s.as_ref() as *const str) }),
                AtomEntry::Sym(_) => None,
            }
        })
    }

    /// Symbol when this is a symbol key, `None` for string keys.
    pub fn as_symbol(&self) -> Option<&'static JsSymbol> {
        ATOM_TABLE.with(|t| {
            let t = t.borrow();
            match t.entries.get(self.0 as usize)?.as_ref() {
                AtomEntry::Sym(sym) => Some(*sym),
                AtomEntry::Str(_) => None,
            }
        })
    }

    /// True when this atom was created from a [`JsValue`] symbol.
    pub fn is_symbol(&self) -> bool {
        ATOM_TABLE.with(|t| {
            let t = t.borrow();
            matches!(
                t.entries.get(self.0 as usize).map(|e| e.as_ref()),
                Some(AtomEntry::Sym(_))
            )
        })
    }
}

/// ECMA-262 7.1.14 `ToPropertyKey` for primitive values.
///
/// Numbers, booleans, `null`, `undefined` and bigints are stringified
/// first (`obj[1]` stores the key `"1"`); strings pass through; symbols
/// are interned by identity. Objects would require `ToPrimitive` /
/// `ToString` with side effects, so the caller receives `None` and must
/// raise `TypeError` (same as QuickJS `JS_ValueToAtom` failing).
pub(crate) fn to_property_atom(v: &JsValue) -> Option<JsAtom> {
    match v.tag() {
        JsTag::String => Some(JsAtom::from_str(v.as_string()?)),
        JsTag::Symbol => Some(JsAtom::from_symbol(v.as_symbol()?)),
        JsTag::Int | JsTag::Bool | JsTag::Null | JsTag::Undefined | JsTag::Float64
        | JsTag::BigInt => Some(JsAtom::from_str(&v.to_string())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_atoms_are_interned_by_content() {
        let a = JsAtom::from_str("hello");
        let b = JsAtom::from_str("hello");
        let c = JsAtom::from_str("world");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.as_str(), Some("hello"));
        assert!(a.as_symbol().is_none());
        assert!(!a.is_symbol());
    }

    #[test]
    fn symbol_atoms_are_interned_by_identity() {
        let s1 = JsSymbol::new_with_description("k");
        let s2 = JsSymbol::new_with_description("k");
        let a1 = JsAtom::from_symbol(s1);
        let a1_again = JsAtom::from_symbol(s1);
        let a2 = JsAtom::from_symbol(s2);
        assert_eq!(a1, a1_again);
        assert_ne!(a1, a2);
        assert!(a1.is_symbol());
        assert_eq!(a1.as_symbol().unwrap(), s1);
        assert!(a1.as_str().is_none());
    }

    #[test]
    fn to_property_key_stringifies_numbers() {
        let atom = to_property_atom(&JsValue::from_i32(1)).unwrap();
        assert_eq!(atom, JsAtom::from_str("1"));
    }

    #[test]
    fn to_property_key_interns_symbols() {
        let sym = JsSymbol::new_with_description("iter");
        let atom = to_property_atom(&JsValue::from_symbol(sym)).unwrap();
        assert_eq!(atom, JsAtom::from_symbol(sym));
        assert_eq!(atom.as_symbol(), Some(sym));
    }
}
