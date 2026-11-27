use jsrt::preprocessor::stage1::Stage1;
use oxc::span::Span;

/// 1-based line of a byte offset in `source`.
fn line(offset: u32, source: &str) -> usize {
    1 + source[..offset as usize].bytes().filter(|b| *b == b'\n').count()
}

fn main() {
    let code = r#"
        // top-level comment
        function Add(a, b) {
            // comment inside function
            let sum = a + b;
            return sum; // trailing comment
        }

        /* block
           comment */
        const f = () => {
            { let inner = 1; } // block body
        };

        const g = () => {
            eval("1"); // dynamic: taints this scope and its children
        };

        for (let i = 0; i < 3; ++i) {
            i;
        }
    "#;
    let stage1 = Stage1::try_from(code).unwrap();
    let source = stage1.program().source_text;

    println!("== scopes (id, line start..end, parent id, analyzable, code) ==");
    for scope in stage1.scopes() {
        let code: Vec<String> = scope
            .code
            .iter()
            .map(|item| {
                let s = item.span();
                format!("{}..{}", line(s.start, source), line(s.end, source))
            })
            .collect();
        println!(
            "{:>2}: {:>2}..{:<2} parent: {:<2} analyzable: {:<5} code: [{}]",
            scope.id,
            line(scope.span.start, source),
            line(scope.span.end, source),
            scope.parent,
            scope.analyzable,
            code.join(", "),
        );
    }

    println!("\n== code chunks (comments stripped) ==");
    let chunks = stage1.get_code_lines();
    let scopes = stage1.scopes();
    let mut max_no = 0;
    for chunk in &chunks {
        for (no, ..) in chunk {
            max_no = max_no.max(*no);
        }
    }
    if max_no > 0 {
        let width = max_no.to_string().len().max(4);
        // Indentation per line (same rule as `ScopeInfo::format`): the
        // number of non-root scopes that strictly enclose the line's
        // code, so opening (`... {`) and closing (`}` / `};`) lines stay
        // at the parent's depth. A scope sharing its closing brace with
        // its (non-root) parent scope (e.g. the body block of a `for`
        // statement) does not add a level.
        let indent_of = |span: Span, text: &str| -> String {
            let content_start = span.start + (text.len() - text.trim_start().len()) as u32;
            let depth = scopes
                .iter()
                .filter(|s| {
                    let shared_end = s.parent >= 0
                        && scopes[s.parent as usize].parent >= 0
                        && scopes[s.parent as usize].span.end == s.span.end;
                    s.id != 0
                        && !shared_end
                        && s.span.start < content_start
                        && s.span.end > span.end
                })
                .count();
            "    ".repeat(depth)
        };
        for (i, chunk) in chunks.into_iter().enumerate() {
            if i > 0 {
                println!("{: <width$} |", "");
            }
            for (no, span, line) in chunk {
                println!("{no:>width$} | {}{}", indent_of(span, &line), line.trim_start());
            }
        }
    }
}
