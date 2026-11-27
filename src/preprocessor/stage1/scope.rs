use oxc::ast::ast::{
    ArrowFunctionExpression, BlockStatement, CallExpression, CatchClause, Comment, Expression,
    ForInStatement, ForOfStatement, ForStatement, Function, Program, Statement, WithStatement,
};
use oxc::ast_visit::{Visit, walk};
use oxc::span::{GetSpan, Span};
use oxc::syntax::scope::ScopeFlags;

use crate::utils::Indent;

/// One scope in the program, in pre-order: an outer scope always comes
/// before every scope nested in it.
///
/// The root (program) scope is id 0, its parent is -1.
#[derive(Debug, Clone)]
pub struct ScopeInfo {
    /// Id of this scope, assigned in pre-order starting at 0; 0 is the
    /// root (program) scope.
    pub id: usize,
    /// Source range of this scope: where it starts and where it ends.
    pub span: Span,
    /// Id of the parent scope; -1 for the root (program) scope, which
    /// has no parent.
    pub parent: isize,
    /// Whether this scope can still be analyzed statically; `false` when
    /// it contains `eval` or `with`. Their dynamic bindings are visible
    /// through the parent chain, so every scope nested in it is tainted
    /// as well.
    pub analyzable: bool,
    /// 1-based line in the original source where this scope starts.
    pub start_line: u32,
    /// 1-based line in the original source where this scope ends.
    pub end_line: u32,
    /// Effective code of this scope: one entry per direct statement, in
    /// source order. Comments are not AST nodes, so they never appear;
    /// a statement that opens a nested scope is recorded as a
    /// [`CodeItem::ChildScope`] and its inner statements live on the
    /// nested scope.
    pub code: Vec<CodeItem>,
}

/// One entry of a scope's effective code.
#[derive(Debug, Clone)]
pub enum CodeItem {
    /// A statement that does not open a nested scope.
    Statement(Span),
    /// A statement (or expression) that opens the nested scope `scope`;
    /// `span` covers the whole statement, which may contain further
    /// sibling scopes (e.g. the blocks of `if` / `else`, or
    /// `try` / `catch` / `finally`).
    ChildScope { span: Span, scope: usize },
}

impl CodeItem {
    /// The span of the underlying statement, whatever kind it is.
    pub fn span(&self) -> Span {
        match self {
            Self::Statement(span) => *span,
            Self::ChildScope { span, .. } => *span,
        }
    }
}

/// The effective lines of `span`: triples of (1-based line number,
/// the line's [`Span`], the line text). The span covers the byte
/// offsets of the line's text within the source, excluding the line
/// terminator (`\n` or `\r\n`). Comments are spliced out of the text
/// (the span is not); lines that end up blank because of that are
/// dropped, lines without comments are kept verbatim (blank lines
/// included, so template literals keep their content).
pub(crate) fn effective_lines(
    span: Span,
    source: &str,
    comments: &[Comment],
) -> Vec<(u32, Span, String)> {
    let start = span.start as usize;
    let end = span.end as usize;
    let inside: Vec<&Comment> = comments
        .iter()
        .filter(|c| c.span.start >= span.start && c.span.end <= span.end)
        .collect();

    let mut lines = Vec::new();
    let mut line_start = start;
    let mut line_no = 1 + source[..start].bytes().filter(|b| *b == b'\n').count() as u32;
    while line_start < end {
        let raw_end = source[line_start..end].find('\n').map_or(end, |i| line_start + i);
        let line_end = if raw_end > line_start && source.as_bytes()[raw_end - 1] == b'\r' {
            raw_end - 1
        } else {
            raw_end
        };
        let mut line_text = String::new();
        let mut cursor = line_start;
        let mut had_comment = false;
        for c in &inside {
            let (cs, ce) = (c.span.start as usize, c.span.end as usize);
            if ce <= line_start || cs >= line_end {
                continue;
            }
            had_comment = true;
            let cs = cs.max(cursor);
            if cs > cursor {
                line_text.push_str(&source[cursor..cs]);
            }
            cursor = cursor.max(ce).min(line_end);
        }
        if cursor < line_end {
            line_text.push_str(&source[cursor..line_end]);
        }

        let is_blank = line_text.trim().is_empty();
        if !had_comment || !is_blank {
            let line = if had_comment { line_text.trim_end().to_string() } else { line_text };
            lines.push((line_no, Span::new(line_start as u32, line_end as u32), line));
        }

        line_no += 1;
        line_start = raw_end + 1;
    }
    lines
}

/// Comments stripped, one line of code per output line, no line numbers,
/// indented by scope nesting: a line sits at the depth of the scopes
/// rooted at `self` that strictly enclose it, so `self`'s own code is at
/// depth 0 and every nested scope one level deeper. Opening lines (`... {`)
/// and closing lines (`}` / `};`) stay at the parent's depth because
/// their scope opens on or after the line's first code and closes on or
/// before its last character. A scope sharing its closing brace with its
/// parent scope (e.g. the body block of a `for` statement) does not add
/// a level.
impl ScopeInfo {
    pub fn format(&self, program: &Program<'_>, scopes: &[ScopeInfo], indent: &Indent) -> String {
        let source = program.source_text;
        let comments = &program.comments;
        let (width, base) = (indent.width, indent.depth);

        // The scopes rooted at `self` (self included): the indentation
        // depth of a line is the number of these that strictly enclose
        // it, so `self`'s own code sits at depth 0 and every nested
        // scope one level deeper.
        let subtree: Vec<&ScopeInfo> = scopes
            .iter()
            .filter(|s| {
                if s.id == self.id {
                    return true;
                }
                let mut p = s.parent;
                while p >= 0 && p as usize != self.id {
                    p = scopes[p as usize].parent;
                }
                p >= 0
            })
            .collect();

        // `true` when the scope ends exactly where its (non-root) parent
        // scope ends: the two share the closing brace (e.g. the body
        // block of a `for` statement inside the `for` scope's span), so
        // the child adds no indentation level of its own.
        let shares_parent_end =
            |s: &ScopeInfo| -> bool {
                let p = s.parent;
                p >= 0
                    && scopes[p as usize].parent >= 0
                    && scopes[p as usize].span.end == s.span.end
            };

        let lines = effective_lines(self.span, source, comments);
        let mut out: Vec<String> = Vec::with_capacity(lines.len());
        for (_, span, text) in lines {
            if text.trim().is_empty() {
                out.push(String::new());
                continue;
            }
            let content_start = span.start + (text.len() - text.trim_start().len()) as u32;
            let depth = subtree
                .iter()
                .filter(|s| {
                    s.id != self.id
                        && s.span.start < content_start
                        && s.span.end > span.end
                        && !shares_parent_end(s)
                })
                .count();
            out.push(format!("{}{}", " ".repeat(width * (base + depth)), text.trim_start()));
        }
        out.join("\n")
    }
}

/// Collects every scope in the program, including the root (program)
/// scope: functions (including arrows), blocks, `for` / `for-in` /
/// `for-of` loops and `catch` clauses. Not tracked (yet): `switch`,
/// class bodies, static blocks.
pub(super) struct ScopeCollector {
    /// Ids of the scopes currently being traversed (the ancestor chain);
    /// the top is the parent of the next scope entered.
    stack: Vec<isize>,
    pub(super) scopes: Vec<ScopeInfo>,
    /// Set while walking a `catch` clause: its body block is the catch
    /// scope itself and must not be recorded a second time.
    suppress_block: bool,
}

impl ScopeCollector {
    pub(super) fn new() -> Self {
        Self { stack: Vec::new(), scopes: Vec::new(), suppress_block: false }
    }

    fn enter(&mut self, span: Span) {
        let parent = self.stack.last().copied().unwrap_or(-1);
        let id = self.scopes.len() as isize;
        self.scopes.push(ScopeInfo {
            id: id as usize,
            span,
            parent,
            analyzable: true,
            start_line: 0,
            end_line: 0,
            code: Vec::new(),
        });
        // Link the scope into its parent's code: the statement that opens
        // it is either the parent's last plain statement (convert it), or
        // already a scope-opening statement with further sibling scopes
        // (append, e.g. the `catch` block after the `try` block).
        if parent >= 0 {
            let code = &mut self.scopes[parent as usize].code;
            let contains = code.last().is_some_and(|item| {
                let s = item.span();
                s.start <= span.start && span.end <= s.end
            });
            if contains {
                let statement = code.last().unwrap().span();
                match code.last_mut().unwrap() {
                    CodeItem::Statement(_) => {
                        *code.last_mut().unwrap() =
                            CodeItem::ChildScope { span: statement, scope: id as usize };
                    }
                    CodeItem::ChildScope { .. } => {
                        code.push(CodeItem::ChildScope { span: statement, scope: id as usize });
                    }
                }
            }
        }
        self.stack.push(id);
    }

    fn leave(&mut self) {
        self.stack.pop();
    }

    /// Marks the scope currently being traversed as unanalyzable.
    fn taint(&mut self) {
        if let Some(&id) = self.stack.last() {
            self.scopes[id as usize].analyzable = false;
        }
    }
}

/// Marks every scope nested in an unanalyzable one as unanalyzable too:
/// bindings introduced by `eval` are visible through the parent chain,
/// and `with` object members shadow lookups in the whole nested subtree.
pub(super) fn propagate_unanalyzable(scopes: &mut [ScopeInfo]) {
    loop {
        let mut changed = false;
        for i in 0..scopes.len() {
            let p = scopes[i].parent;
            if p >= 0 && !scopes[p as usize].analyzable && scopes[i].analyzable {
                scopes[i].analyzable = false;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

impl Visit<'static> for ScopeCollector {
    fn visit_program(&mut self, program: &Program<'static>) {
        self.enter(program.span);
        walk::walk_program(self, program);
        self.leave();
    }

    fn visit_statement(&mut self, stmt: &Statement<'static>) {
        // Every statement belongs to the innermost enclosing scope; a
        // statement that is itself a scope (block / loop body) is recorded
        // here before its scope is entered, and its inner statements are
        // recorded on the nested scope.
        if let Some(&id) = self.stack.last() {
            self.scopes[id as usize].code.push(CodeItem::Statement(stmt.span()));
        }
        walk::walk_statement(self, stmt);
    }

    fn visit_function(&mut self, func: &Function<'static>, flags: ScopeFlags) {
        self.enter(func.span);
        walk::walk_function(self, func, flags);
        self.leave();
    }

    fn visit_arrow_function_expression(&mut self, arrow: &ArrowFunctionExpression<'static>) {
        self.enter(arrow.span);
        walk::walk_arrow_function_expression(self, arrow);
        self.leave();
    }

    fn visit_block_statement(&mut self, stmt: &BlockStatement<'static>) {
        if self.suppress_block {
            self.suppress_block = false;
            walk::walk_block_statement(self, stmt);
        } else {
            self.enter(stmt.span);
            walk::walk_block_statement(self, stmt);
            self.leave();
        }
    }

    fn visit_for_statement(&mut self, stmt: &ForStatement<'static>) {
        self.enter(stmt.span);
        walk::walk_for_statement(self, stmt);
        self.leave();
    }

    fn visit_for_in_statement(&mut self, stmt: &ForInStatement<'static>) {
        self.enter(stmt.span);
        walk::walk_for_in_statement(self, stmt);
        self.leave();
    }

    fn visit_for_of_statement(&mut self, stmt: &ForOfStatement<'static>) {
        self.enter(stmt.span);
        walk::walk_for_of_statement(self, stmt);
        self.leave();
    }

    fn visit_catch_clause(&mut self, clause: &CatchClause<'static>) {
        self.enter(clause.span);
        self.suppress_block = true;
        walk::walk_catch_clause(self, clause);
        self.suppress_block = false;
        self.leave();
    }

    fn visit_call_expression(&mut self, expr: &CallExpression<'static>) {
        let tainted = match &expr.callee {
            Expression::Identifier(id) => id.name == "eval",
            Expression::StaticMemberExpression(member) => member.property.name == "eval",
            _ => false,
        };
        if tainted {
            self.taint();
        }
        walk::walk_call_expression(self, expr);
    }

    fn visit_with_statement(&mut self, stmt: &WithStatement<'static>) {
        self.taint();
        walk::walk_with_statement(self, stmt);
    }
}
