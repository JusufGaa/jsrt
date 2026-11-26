use std::collections::HashMap;
use std::convert::{Infallible, TryFrom};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use oxc::allocator::Allocator;
use oxc::ast::ast::{
    ArrowFunctionExpression, BindingPattern, Function, FunctionType, Program, VariableDeclarator,
};
use oxc::ast_visit::{Visit, walk};
use oxc::parser::Parser;
use oxc::span::{GetSpan, Span, SourceType};
use oxc::syntax::scope::ScopeFlags;

use crate::preprocessor::Stage;
use crate::runtime::Worker;

pub struct Stage1 {
    program: Program<'static>,
    /// All functions in the program, keyed by name.
    ///
    /// Named functions are keyed by their identifier; anonymous ones get a
    /// generated key `<anonymous:N>` (N is the visit order), so their
    /// [`FunctionInfo::name`] is [`None`].
    functions: HashMap<&'static str, FunctionInfo>,
}

/// Summary of a function found in the program.
#[derive(Debug)]
pub struct FunctionInfo {
    /// The function identifier; [`None`] for anonymous functions.
    ///
    /// Arrow functions and function expressions inherit the name of the
    /// variable they are directly assigned to (if any).
    pub name: Option<&'static str>,
    pub span: Span,
    pub r#type: FunctionType,
    pub generator: bool,
    /// `true` for arrow functions (`ArrowFunctionExpression`).
    pub arrow: bool,
}

/// Collects every [`Function`] and arrow function in the program.
struct FunctionCollector {
    functions: Vec<FunctionInfo>,
    /// Name of the binding the visited initializer is assigned to.
    declarator_name: Option<&'static str>,
}

impl Visit<'static> for FunctionCollector {
    fn visit_variable_declarator(&mut self, decl: &VariableDeclarator<'static>) {
        let prev = self.declarator_name.take();
        self.declarator_name =
            if let BindingPattern::BindingIdentifier(id) = &decl.id { Some(id.name.as_str()) } else { None };
        walk::walk_variable_declarator(self, decl);
        self.declarator_name = prev;
    }

    fn visit_function(&mut self, func: &Function<'static>, flags: ScopeFlags) {
        self.functions.push(FunctionInfo {
            name: func
                .id
                .as_ref()
                .map(|id| id.name.as_str())
                .or(self.declarator_name),
            span: func.span,
            r#type: func.r#type,
            generator: func.generator,
            arrow: false,
        });
        walk::walk_function(self, func, flags);
    }

    fn visit_arrow_function_expression(&mut self, arrow: &ArrowFunctionExpression<'static>) {
        self.functions.push(FunctionInfo {
            name: self.declarator_name,
            span: arrow.span,
            r#type: FunctionType::FunctionExpression,
            generator: false,
            arrow: true,
        });
        walk::walk_arrow_function_expression(self, arrow);
    }
}

impl Stage1 {
    pub fn new(code: String) -> Self {
        let code = Box::leak(code.into_boxed_str());
        let allocator = Box::leak(Box::new(Allocator::default()));
        let program = Parser::new(allocator, code, SourceType::mjs()).parse().program;

        let mut collector = FunctionCollector { functions: Vec::new(), declarator_name: None };
        collector.visit_program(&program);

        let mut functions = HashMap::new();
        for (i, info) in collector.functions.into_iter().enumerate() {
            let key = match info.name {
                Some(name) => name,
                None => Box::leak(format!("<anonymous:{i}>").into_boxed_str()),
            };
            functions.insert(key, info);
        }

        Self { program, functions }
    }

    pub fn functions(&self) -> &HashMap<&'static str, FunctionInfo> {
        &self.functions
    }

    pub fn program(&self) -> &Program<'static> {
        &self.program
    }

    /// Debug representation of the program, with only `body` and `functions` expanded.
    pub fn debug(&self) -> String {
        self.debug_with(["body", "functions"])
    }

    /// Debug representation of the program.
    ///
    /// Fields listed in `expand` are printed in full; all other fields are
    /// printed as `name: ...`.
    pub fn debug_with<I, S>(&self, expand: I) -> String
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let expanded: Vec<S> = expand.into_iter().collect();
        let is_expanded = |name: &str| expanded.iter().any(|s| s.as_ref() == name);

        let mut out = String::from("Program {\n");
        let write_field = |out: &mut String, name: &str, value: &dyn std::fmt::Debug| {
            if is_expanded(name) {
                out.push_str(&format!("    {}: {:?},\n", name, value));
            } else {
                out.push_str(&format!("    {}: ...,\n", name));
            }
        };
        let write_body = |out: &mut String| {
            out.push_str("    body: [\n");
            for stmt in &self.program.body {
                out.push_str(&format!("        {:?},\n", stmt));
            }
            out.push_str("    ],\n");
        };
        let write_functions = |out: &mut String| {
            out.push_str("    functions: {\n");
            for (name, info) in &self.functions {
                out.push_str(&format!("        {name:?}: {:?},\n", info));
            }
            out.push_str("    },\n");
        };

        let p = &self.program;
        write_field(&mut out, "span", &p.span);
        write_field(&mut out, "node_id", &p.node_id);
        write_field(&mut out, "scope_id", &p.scope_id);
        write_field(&mut out, "source_text", &p.source_text);
        write_field(&mut out, "comments", &p.comments);
        write_field(&mut out, "hashbang", &p.hashbang);
        write_field(&mut out, "directives", &p.directives);
        if is_expanded("body") {
            write_body(&mut out);
        } else {
            out.push_str("    body: ...,\n");
        }
        if is_expanded("functions") {
            write_functions(&mut out);
        } else {
            out.push_str("    functions: ...,\n");
        }
        write_field(&mut out, "source_type", &p.source_type);

        out.push('}');
        out
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
        self.program
            .body
            .iter()
            .map(move |stmt| {
                let span = stmt.span();
                &source[span.start as usize..span.end as usize]
            })
    }
}
