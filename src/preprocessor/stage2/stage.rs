use std::collections::{BTreeMap, HashMap};

use oxc::ast::ast::{Expression, Program, Statement};
use oxc::ast_visit::Visit;
use oxc::span::GetSpan;
use oxc::syntax::node::NodeId;

use super::function::{FunctionCollector, analyze_purity};
use super::globals::GlobalCollector;
use super::FunctionInfo;
use super::GlobalInfo;
use crate::preprocessor::stage1::Stage1;
use crate::preprocessor::Stage;
use crate::utils::Indent;

pub struct Stage2 {
    program: Program<'static>,
    /// All functions in the program, keyed by name.
    ///
    /// Named functions are keyed by their identifier; anonymous ones get a
    /// generated key `<anonymous:N>` (N is the visit order), so their
    /// [`FunctionInfo::name`] is [`None`].
    functions: HashMap<&'static str, FunctionInfo>,
    /// Names left on `window` / `globalThis` after the program runs, with
    /// how each one got there; in visit order.
    globals: Vec<GlobalInfo>,
}

impl From<Stage1> for Stage2 {
    fn from(stage1: Stage1) -> Self {
        let program = stage1.program;

        let mut collector = FunctionCollector::new();
        collector.visit_program(&program);

        let mut globals_collector = GlobalCollector::new();
        globals_collector.visit_top_level(&program);
        let globals = globals_collector.globals;

        let mut functions = HashMap::new();
        for (i, info) in collector.functions.into_iter().enumerate() {
            let key = match info.name {
                Some(name) => name,
                None => Box::leak(format!("<anonymous:{i}>").into_boxed_str()),
            };
            functions.insert(key, info);
        }
        analyze_purity(&mut functions);

        Self { program, functions, globals }
    }
}

impl Stage2 {
    pub fn functions(&self) -> &HashMap<&'static str, FunctionInfo> {
        &self.functions
    }

    /// Names left on `window` / `globalThis` after the program runs, in
    /// visit order.
    pub fn globals(&self) -> &[GlobalInfo] {
        &self.globals
    }

    /// All functions as a table keyed by their AST [`NodeId`], in node id order.
    pub fn functions_by_node_id(&self) -> BTreeMap<NodeId, &FunctionInfo> {
        self.functions.values().map(|info| (info.node_id, info)).collect()
    }

    pub fn program(&self) -> &Program<'static> {
        &self.program
    }

    /// Debug representation of the program with `body`, `functions` and
    /// `globals` expanded; the line holding `Program {` is at `indent`, its
    /// fields one level deeper.
    pub fn debug<I, S>(&self, indent: &Indent, expand: I) -> String
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let expanded: Vec<String> =
            expand.into_iter().map(|s| s.as_ref().to_string()).collect();
        let is_expanded = |name: &str| expanded.iter().any(|s| s == name);

        let field = indent.next();
        let pad = field.pad();
        let pad2 = field.next().pad();

        let mut out = String::from("Program {\n");
        let write_field = |out: &mut String, name: &str, value: &dyn std::fmt::Debug| {
            if is_expanded(name) {
                out.push_str(&format!("{pad}{name}: {value:?},\n"));
            } else {
                out.push_str(&format!("{pad}{name}: ...,\n"));
            }
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
            out.push_str(&format!("{pad}body: [\n"));
            for stmt in &p.body {
                if self.creates_global(stmt) {
                    out.push_str(&format!(
                        "{pad2}{} Global({:?}),\n",
                        Self::statement_kind_name(stmt),
                        stmt.node_id()
                    ));
                } else {
                    out.push_str(&format!("{pad2}{stmt:?},\n"));
                }
            }
            out.push_str(&format!("{pad}],\n"));
        } else {
            out.push_str(&format!("{pad}body: ...,\n"));
        }

        if is_expanded("functions") {
            out.push_str(&format!("{pad}functions: {{\n"));
            for (name, info) in &self.functions {
                out.push_str(&format!(
                    "{pad2}{name:?}: {},\n",
                    info.debug(Some(&self.program), &field.next())
                ));
            }
            out.push_str(&format!("{pad}}},\n"));
        } else {
            out.push_str(&format!("{pad}functions: ...,\n"));
        }

        if is_expanded("globals") {
            out.push_str(&format!("{pad}globals: [\n"));
            for info in &self.globals {
                out.push_str(&format!("{pad2}{info:?},\n"));
            }
            out.push_str(&format!("{pad}],\n"));
        } else {
            out.push_str(&format!("{pad}globals: ...,\n"));
        }

        write_field(&mut out, "source_type", &p.source_type);

        out.push_str(&format!("{indent}}}"));
        out
    }

    /// `true` for top-level statements that are bare anonymous functions
    /// (e.g. `function() {}` as an expression statement): such a function is
    /// never referenced anywhere, so it is dead code.
    fn is_uncalled_anonymous_function(
        stmt: &Statement<'static>,
        functions: &BTreeMap<NodeId, &FunctionInfo>,
    ) -> bool {
        let Statement::ExpressionStatement(expr_stmt) = stmt else { return false };
        let info = match &expr_stmt.expression {
            Expression::FunctionExpression(func) if func.id.is_none() => {
                functions.get(&func.node_id())
            }
            Expression::ArrowFunctionExpression(arrow) => functions.get(&arrow.node_id()),
            _ => return false,
        };
        info.is_some_and(|info| info.name.is_none())
    }

    /// `true` for top-level statements whose source contains the binding
    /// site of one of the collected [`GlobalInfo`] entries, i.e. statements
    /// that create a global (declaration, function declaration, assignment
    /// to a bare identifier, `window.x = ...`).
    fn creates_global(&self, stmt: &Statement<'static>) -> bool {
        let span = stmt.span();
        self.globals
            .iter()
            .any(|g| g.span.start >= span.start && g.span.end <= span.end)
    }

    /// Variant name of a top-level statement, for the compact
    /// `<Kind> Global(NodeId)` body listing.
    fn statement_kind_name(stmt: &Statement<'static>) -> &'static str {
        match stmt {
            Statement::BlockStatement(_) => "BlockStatement",
            Statement::BreakStatement(_) => "BreakStatement",
            Statement::ContinueStatement(_) => "ContinueStatement",
            Statement::DebuggerStatement(_) => "DebuggerStatement",
            Statement::DoWhileStatement(_) => "DoWhileStatement",
            Statement::EmptyStatement(_) => "EmptyStatement",
            Statement::ExpressionStatement(_) => "ExpressionStatement",
            Statement::ForInStatement(_) => "ForInStatement",
            Statement::ForOfStatement(_) => "ForOfStatement",
            Statement::ForStatement(_) => "ForStatement",
            Statement::IfStatement(_) => "IfStatement",
            Statement::LabeledStatement(_) => "LabeledStatement",
            Statement::ReturnStatement(_) => "ReturnStatement",
            Statement::SwitchStatement(_) => "SwitchStatement",
            Statement::ThrowStatement(_) => "ThrowStatement",
            Statement::TryStatement(_) => "TryStatement",
            Statement::WhileStatement(_) => "WhileStatement",
            Statement::WithStatement(_) => "WithStatement",
            Statement::VariableDeclaration(_) => "VariableDeclaration",
            Statement::FunctionDeclaration(_) => "FunctionDeclaration",
            Statement::ClassDeclaration(_) => "ClassDeclaration",
            Statement::ImportDeclaration(_) => "ImportDeclaration",
            Statement::ExportAllDeclaration(_) => "ExportAllDeclaration",
            Statement::ExportDefaultDeclaration(_) => "ExportDefaultDeclaration",
            Statement::ExportDeclaration(_) => "ExportDeclaration",
            Statement::ExportNamedDeclaration(_) => "ExportNamedDeclaration",
            Statement::ExportFromDeclaration(_) => "ExportFromDeclaration",
            Statement::TSExportAssignment(_) => "TSExportAssignment",
            Statement::TSNamespaceExportDeclaration(_) => "TSNamespaceExportDeclaration",
            Statement::TSTypeAliasDeclaration(_) => "TSTypeAliasDeclaration",
            Statement::TSInterfaceDeclaration(_) => "TSInterfaceDeclaration",
            Statement::TSEnumDeclaration(_) => "TSEnumDeclaration",
            Statement::TSExternalModuleDeclaration(_) => "TSExternalModuleDeclaration",
            Statement::TSNamespaceDeclaration(_) => "TSNamespaceDeclaration",
            Statement::TSGlobalDeclaration(_) => "TSGlobalDeclaration",
            Statement::TSImportEqualsDeclaration(_) => "TSImportEqualsDeclaration",
        }
    }
}

/// Debug representation with the default indent ([`Indent::default`]).
impl std::fmt::Debug for Stage2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.debug(&Indent::default(), ["body", "functions", "globals"]))
    }
}

impl Stage for Stage2 {
    fn stage(&self) -> u8 {
        2
    }

    fn get_code(&self) -> impl Iterator<Item = &str> {
        let source = self.program.source_text;
        let functions = self.functions_by_node_id();
        self.program
            .body
            .iter()
            .filter(move |stmt| !Self::is_uncalled_anonymous_function(stmt, &functions))
            .map(move |stmt| {
                let span = stmt.span();
                &source[span.start as usize..span.end as usize]
            })
    }
}
