//! `RegExp` built-in class.

pub(crate) const GLOBAL: u16 = 1 << 0; // g
pub(crate) const IGNORE_CASE: u16 = 1 << 1; // i
pub(crate) const MULTILINE: u16 = 1 << 2; // m
pub(crate) const DOT_ALL: u16 = 1 << 3; // s
pub(crate) const UNICODE: u16 = 1 << 4; // u
pub(crate) const STICKY: u16 = 1 << 5; // y
pub(crate) const HAS_INDICES: u16 = 1 << 6; // d
pub(crate) const UNICODE_SETS: u16 = 1 << 7; // v

/// Private data of a RegExp object (`JsInternal::RegExp`).
pub(crate) struct RegExpData {
    pub source: String,
    pub flags: u16,
}

impl RegExpData {
    /// Creates from pattern text and a flags string, e.g. `("abc", "gi")`.
    ///
    /// Returns `None` on an unknown or duplicate flag character — the JS
    /// semantics is a SyntaxError, the caller maps `None` accordingly.
    pub(crate) fn new(source: impl Into<String>, flags: &str) -> Option<Self> {
        let mut mask = 0u16;
        for c in flags.chars() {
            let bit = match c {
                'g' => GLOBAL,
                'i' => IGNORE_CASE,
                'm' => MULTILINE,
                's' => DOT_ALL,
                'u' => UNICODE,
                'y' => STICKY,
                'd' => HAS_INDICES,
                'v' => UNICODE_SETS,
                _ => return None,
            };
            if mask & bit != 0 {
                return None; // duplicate flag
            }
            mask |= bit;
        }
        Some(Self {
            source: source.into(),
            flags: mask,
        })
    }
}
