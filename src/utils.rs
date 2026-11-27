use std::fmt;

/// Indentation state for formatted output: how many spaces per level
/// (`width`) and how many levels are already applied (`depth`).
pub struct Indent {
    /// Spaces per indentation level.
    pub width: usize,
    /// Number of levels already indented.
    pub depth: usize,
}

impl Default for Indent {
    /// `Indent { width: 4, depth: 0 }`.
    fn default() -> Self {
        Self { width: 4, depth: 0 }
    }
}

impl Indent {
    pub fn new(width: usize, depth: usize) -> Self {
        Self { width, depth }
    }

    /// The leading spaces for the current level.
    pub fn pad(&self) -> String {
        " ".repeat(self.width * self.depth)
    }

    /// One level deeper, same width.
    pub fn next(&self) -> Self {
        Self { width: self.width, depth: self.depth + 1 }
    }
}

impl fmt::Display for Indent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.pad())
    }
}

pub(crate) fn format_string(s: &str, quote: impl Into<Option<char>>) -> String {
    let quote = quote.into();
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
