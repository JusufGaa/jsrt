use oxc::ast::ast::{
    AssignmentTarget, BindingPattern, Expression, Program, Statement, VariableDeclarationKind,
};
use oxc::ast_visit::{Visit, walk};
use oxc::span::Span;
use oxc::syntax::node::NodeId;

/// Names that could be left on `window` / `globalThis` after the program
/// runs, and how each one got there.
#[derive(Debug, Clone, Copy)]
pub enum GlobalOrigin {
    /// Top-level `var x = ...` (script semantics: `var` leaks to globalThis).
    Var,
    /// Top-level `let` / `const` binding; does not leak to `globalThis` in
    /// script semantics, but is part of the program's top-level state.
    Lexical,
    /// Top-level `function x() {}`.
    Function,
    /// Top-level assignment `x = ...` (implicit global; may also fire for a
    /// declared `var`, check [`GlobalOrigin::Var`] entries when deduplicating).
    Assignment,
    /// Explicit write `window.x = ...` / `globalThis.x = ...` (also `global`,
    /// `self`), anywhere in the program, including inside functions.
    GlobalObject,
}

/// One entry of the global-surface analysis.
#[derive(Debug)]
pub struct GlobalInfo {
    pub name: &'static str,
    pub origin: GlobalOrigin,
    /// Span of the binding site (the identifier, function or target property).
    pub span: Span,
    pub node_id: NodeId,
}

/// Object names treated as the global object for explicit writes.
const GLOBAL_OBJECTS: [&str; 4] = ["window", "globalThis", "global", "self"];

/// Collects everything that (may) end up on the global object.
pub(super) struct GlobalCollector {
    pub(super) globals: Vec<GlobalInfo>,
}

impl GlobalCollector {
    pub(super) fn new() -> Self {
        Self { globals: Vec::new() }
    }

    /// Scans top-level statements for global bindings, then visits the whole
    /// program for explicit `window.x = ...` style writes.
    pub(super) fn visit_top_level(&mut self, program: &Program<'static>) {
        for stmt in &program.body {
            match stmt {
                Statement::VariableDeclaration(decl) => {
                    let origin = match decl.kind {
                        VariableDeclarationKind::Var => GlobalOrigin::Var,
                        VariableDeclarationKind::Let | VariableDeclarationKind::Const => {
                            GlobalOrigin::Lexical
                        }
                        _ => continue,
                    };
                    for d in &decl.declarations {
                        if let BindingPattern::BindingIdentifier(id) = &d.id {
                            self.push(id.name.as_str(), origin, id.span, decl.node_id());
                        }
                    }
                }
                Statement::FunctionDeclaration(func) => {
                    if let Some(id) = &func.id {
                        self.push(
                            id.name.as_str(),
                            GlobalOrigin::Function,
                            id.span,
                            func.node_id(),
                        );
                    }
                }
                Statement::ExpressionStatement(es) => {
                    if let Expression::AssignmentExpression(assign) = &es.expression {
                        if let AssignmentTarget::AssignmentTargetIdentifier(id) = &assign.left {
                            self.push(
                                id.name.as_str(),
                                GlobalOrigin::Assignment,
                                id.span,
                                assign.node_id(),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        self.visit_program(program);
    }

    fn push(&mut self, name: &'static str, origin: GlobalOrigin, span: Span, node_id: NodeId) {
        self.globals.push(GlobalInfo { name, origin, span, node_id });
    }
}

impl Visit<'static> for GlobalCollector {
    fn visit_expression(&mut self, expr: &Expression<'static>) {
        if let Expression::AssignmentExpression(assign) = expr {
            if let Some((name, span)) = global_object_target(&assign.left) {
                self.push(name, GlobalOrigin::GlobalObject, span, assign.node_id());
            }
        }
        walk::walk_expression(self, expr);
    }
}

/// The property name written through a global-object member assignment,
/// `window.x = ...`, `globalThis["x"] = ...` etc.; `None` for other targets.
fn global_object_target(target: &AssignmentTarget<'static>) -> Option<(&'static str, Span)> {
    let (object, name) = match target {
        AssignmentTarget::StaticMemberExpression(m) => {
            (&m.object, (m.property.name.as_str(), m.property.span))
        }
        AssignmentTarget::ComputedMemberExpression(m) => {
            let Expression::StringLiteral(s) = &m.expression else { return None };
            let Some(name) = s.value.as_str() else { return None };
            (&m.object, (name, s.span))
        }
        _ => return None,
    };
    let Expression::Identifier(id) = object else { return None };
    GLOBAL_OBJECTS.contains(&id.name.as_str()).then_some(name)
}
