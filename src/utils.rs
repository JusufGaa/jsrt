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
