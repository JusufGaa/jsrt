use std::convert::{Infallible, TryFrom};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use oxc::allocator::Allocator;
use oxc::ast::ast::Program;
use oxc::ast_visit::Visit;
use oxc::parser::Parser;
use oxc::span::{SourceType, Span};

use crate::preprocessor::Stage;
use crate::utils::Indent;

use super::scope::{ScopeCollector, ScopeInfo, effective_lines, propagate_unanalyzable};

/// Stage 1: parses the source code into an AST [`Program`] and maps out
/// its scopes.
pub struct Stage1 {
    pub(crate) program: Program<'static>,
    /// All scopes in the program, including the root scope (id 0, whose
    /// parent is -1), outer before inner (pre-order).
    scopes: Vec<ScopeInfo>,
}

impl Stage1 {
    pub fn new(code: String) -> Self {
        let code = Box::leak(code.into_boxed_str());
        let allocator = Box::leak(Box::new(Allocator::default()));
        let program = Parser::new(allocator, code, SourceType::mjs()).parse().program;

        let mut scope_collector = ScopeCollector::new();
        scope_collector.visit_program(&program);
        let mut scopes = scope_collector.scopes;
        propagate_unanalyzable(&mut scopes);

        // Record the original 1-based start/end line of every scope; the
        // span itself is the byte range in the source fed to the parser,
        // which is the original code at this stage.
        let source = program.source_text;
        let line_of = |offset: u32| {
            1 + source[..offset as usize].bytes().filter(|b| *b == b'\n').count() as u32
        };
        for scope in &mut scopes {
            scope.start_line = line_of(scope.span.start);
            scope.end_line = line_of(scope.span.end);
        }

        Self { program, scopes }
    }

    /// The parsed program.
    pub fn program(&self) -> &Program<'static> {
        &self.program
    }

    /// All scopes in the program, including the root scope (id 0, whose
    /// parent is -1), outer before inner (pre-order); each entry holds
    /// its own id, its [`Span`] and its parent scope's id.
    pub fn scopes(&self) -> &[ScopeInfo] {
        &self.scopes
    }
}

/// The formatted program: comments stripped, one line of code per output
/// line, no line numbers, indented by scope nesting (default indent).
impl fmt::Display for Stage1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.scopes[0].format(&self.program, &self.scopes, &Indent::default()))
    }
}

/// Builds from source code directly; never fails.
impl TryFrom<&str> for Stage1 {
    type Error = Infallible;

    fn try_from(code: &str) -> Result<Self, Self::Error> {
        Self::try_from(code.to_string())
    }
}

impl TryFrom<String> for Stage1 {
    type Error = Infallible;

    fn try_from(code: String) -> Result<Self, Self::Error> {
        Ok(Self::new(code))
    }
}

/// Reads the code from a file.
impl TryFrom<&Path> for Stage1 {
    type Error = io::Error;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        let code = fs::read_to_string(path)?;
        Ok(Self::new(code))
    }
}

impl TryFrom<PathBuf> for Stage1 {
    type Error = io::Error;

    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        Self::try_from(path.as_path())
    }
}

impl Stage for Stage1 {
    fn stage(&self) -> u8 {
        1
    }

    fn get_code(&self) -> impl Iterator<Item = &str> {
        let source = self.program.source_text;
        let comments = &self.program.comments;
        self.scopes[0]
            .code
            .iter()
            .map(move |item| {
                let text = effective_lines(item.span(), source, comments)
                    .into_iter()
                    .map(|(.., line)| line)
                    .collect::<Vec<_>>()
                    .join("\n");
                let leaked: &'static str = Box::leak(text.into_boxed_str());
                leaked
            })
    }
}

impl Stage1 {
    /// Each code chunk (in root scope `code` order) as its effective
    /// lines: triples of the 1-based source line number, the line's
    /// [`Span`] (byte offsets of the line text, line terminator excluded)
    /// and the line text, with comments spliced out and comment-only
    /// lines dropped.
    pub fn get_code_lines(&self) -> Vec<Vec<(u32, Span, String)>> {
        let source = self.program.source_text;
        let comments = &self.program.comments;
        self.scopes[0]
            .code
            .iter()
            .map(|item| effective_lines(item.span(), source, comments))
            .collect()
    }
}
