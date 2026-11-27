use oxc::ast::AstKind;
use oxc::ast::ast::{
    ArrowFunctionExpression, AssignmentExpression, AssignmentTarget, BindingIdentifier,
    BindingPattern, CallExpression, Expression, ForStatementLeft, Function, FunctionType,
    IdentifierReference, NewExpression, Program, SimpleAssignmentTarget, UpdateExpression,
    VariableDeclarator,
};
use oxc::ast_visit::{Visit, walk};
use oxc::span::{GetSpan, Span};
use oxc::syntax::node::NodeId;
use oxc::syntax::scope::ScopeFlags;

use crate::utils::Indent;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::fmt::Write as _;

/// Summary of a function found in the program.
pub struct FunctionInfo {
    /// The function identifier; [`None`] for anonymous functions.
    ///
    /// Arrow functions and function expressions inherit the name of the
    /// variable they are directly assigned to (if any).
    pub name: Option<&'static str>,
    pub span: Span,
    pub r#type: FunctionType,
    /// `true` for generator functions (`function*`).
    pub generator: bool,
    /// `true` for arrow functions (`ArrowFunctionExpression`).
    pub arrow: bool,
    /// [`NodeId`] of this function node; unique within the parsed AST
    /// (assigned by the parser in construction order, may have gaps).
    pub node_id: NodeId,
    /// [`NodeId`] of the immediate parent node;
    /// [`NodeId::ROOT`] (0, the `Program`) for top-level functions.
    pub parent_node_id: NodeId,
    /// Call targets appearing in this function's own body, in visit order.
    ///
    /// Calls made by nested functions are recorded on the nested function,
    /// not here. Top-level calls (outside every function) are not recorded.
    pub callees: Vec<Callee>,
    /// Bindings declared outside of the function that it assigns to or
    /// updates (an outer local, a top-level global), including writes into a
    /// non-local object's properties (`outer.x = 1` records `outer`).
    pub writes_outer: BTreeSet<&'static str>,
    /// Bindings declared outside of the function that it reads (an outer
    /// local, a top-level binding, or an undeclared global). Calling another
    /// function by name also reads its binding, so every function that
    /// calls by name has the callee's name here.
    pub reads_outer: BTreeSet<&'static str>,
    /// Purity classification; computed by [`analyze_purity`] after
    /// collection. Defaults to [`Purity::Depends`] for freshly collected
    /// functions.
    pub purity: Purity,
}

/// Three-state purity classification of a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purity {
    /// Self-contained: no reads or writes of outer bindings and no calls at
    /// all; its result is determined by its arguments alone.
    Pure,
    /// Definitely not pure: writes to an outer binding, or calls an impure
    /// or unresolvable callee (member calls, builtins, unknown globals).
    Impure,
    /// Its purity depends on its environment: it reads outer bindings or
    /// calls other in-program functions (all of which are pure or themselves
    /// dependent). Treat conservatively as impure when a binary decision is
    /// required.
    Depends,
}

/// A call target found inside a function body.
#[derive(Debug)]
pub enum Callee {
    /// Direct call to an identifier: `foo()`.
    Identifier(&'static str),
    /// Call through a static member access with a named object:
    /// `obj.method()`, `console.log()`.
    Member { object: &'static str, property: &'static str },
    /// Any other callee shape: `this.x()`, `a[i]()`, `f()()`, `new (f())()`.
    Other,
}

impl fmt::Display for Callee {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Callee::Identifier(name) => f.write_str(name),
            Callee::Member { object, property } => write!(f, "{object}.{property}"),
            Callee::Other => f.write_str("<other>"),
        }
    }
}

/// Computes [`FunctionInfo::purity`] for every function via fixpoint
/// iteration over the call graph.
///
/// Rules:
/// - a function writing to an outer binding is [`Purity::Impure`];
/// - a function with an impure or unresolvable callee is [`Purity::Impure`]
///   (conservative: member calls, builtins and unknown globals are treated
///   as impure sources);
/// - a function that reads outer bindings or calls in-program functions
///   (all pure or themselves dependent) is [`Purity::Depends`]: its purity
///   is decided by its environment, not locally;
/// - only a fully self-contained function is [`Purity::Pure`].
///
/// Known limitation: callees are resolved by name through `functions`, so
/// same-name functions in different scopes cannot be distinguished.
pub(super) fn analyze_purity(functions: &mut HashMap<&'static str, FunctionInfo>) {
    loop {
        let snapshot: HashMap<&'static str, Purity> =
            functions.iter().map(|(name, f)| (*name, f.purity)).collect();
        let mut changed = false;
        for f in functions.values_mut() {
            if f.purity != Purity::Depends {
                continue;
            }
            if !f.writes_outer.is_empty() {
                f.purity = Purity::Impure;
                changed = true;
                continue;
            }
            let mut resolved = Purity::Pure;
            for callee in &f.callees {
                let purity = match callee {
                    Callee::Identifier(callee_name) => snapshot.get(callee_name).copied(),
                    Callee::Member { .. } | Callee::Other => None,
                };
                match purity {
                    // Calling another function makes the result depend on it,
                    // even when that function is currently proven pure.
                    Some(Purity::Pure) | Some(Purity::Depends) => resolved = Purity::Depends,
                    Some(Purity::Impure) | None => {
                        resolved = Purity::Impure;
                        break;
                    }
                }
            }
            if resolved == Purity::Pure && !f.reads_outer.is_empty() {
                resolved = Purity::Depends;
            }
            if resolved != f.purity {
                f.purity = resolved;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

impl FunctionInfo {
    /// Debug summary of this function, one field per line; the line holding
    /// `FunctionInfo {` is at `indent`, fields one level deeper.
    ///
    /// When `program` is [`Some`], a `code` field is included, computed on
    /// the fly by slicing `program.source_text` with `self.span`; the source
    /// is printed line by line, indented.
    pub fn debug(&self, program: Option<&Program<'_>>, indent: &Indent) -> String {
        let field = indent.next();
        let mut out = String::from("FunctionInfo {\n");
        let _ = writeln!(out, "{field}type: {:?},", self.r#type);
        let _ = writeln!(out, "{field}generator: {},", self.generator);
        let _ = writeln!(out, "{field}arrow: {},", self.arrow);
        let _ = writeln!(out, "{field}node_id: {},", usize::from(self.node_id));
        let _ = writeln!(out, "{field}parent_node_id: {},", usize::from(self.parent_node_id));
        let _ = writeln!(out, "{field}purity: {:?},", self.purity);
        let _ = writeln!(out, "{field}writes_outer: [{}],", self.writes_outer.iter().copied().collect::<Vec<_>>().join(", "));
        let _ = writeln!(out, "{field}reads_outer: [{}],", self.reads_outer.iter().copied().collect::<Vec<_>>().join(", "));
        let _ = writeln!(
            out,
            "{field}callees: [{}],",
            self.callees.iter().map(Callee::to_string).collect::<Vec<_>>().join(", ")
        );
        if let Some(program) = program {
            let code = program
                .source_text
                .get(self.span.start as usize..self.span.end as usize)
                .unwrap_or_default();
            let lines: Vec<&str> = dedent_lines(code).collect();
            if lines.len() == 1 {
                let _ = writeln!(out, "{field}code: {},", lines[0]);
            } else if lines.len() > 1 {
                let _ = writeln!(out, "{field}code:");
                let code_field = field.next();
                for line in &lines {
                    let _ = writeln!(out, "{code_field}{line}");
                }
            }
        }
        out.push_str(&format!("{indent}}}"));
        out
    }
}

impl fmt::Debug for FunctionInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.debug(None, &Indent::default()))
    }
}

/// Yields the lines of `code`. A single line is returned as-is. For multi-line
/// code ending in a line of only whitespace plus a final `}`, that trailing
/// indentation is stripped as a common prefix from every line.
fn dedent_lines(code: &str) -> impl Iterator<Item = &str> {
    let last = code.lines().last().unwrap_or_default();
    let body = last.trim_start();
    let indent = (code.lines().count() > 1 && body == "}")
        .then(|| &last[..last.len() - body.len()]);
    code.lines().map(move |line| match indent {
        Some(prefix) => line.strip_prefix(prefix).unwrap_or(line),
        None => line,
    })
}

/// Collects every [`Function`] and arrow function in the program.
pub(super) struct FunctionCollector {
    pub(super) functions: Vec<FunctionInfo>,
    /// Name of the binding the visited initializer is assigned to.
    declarator_name: Option<&'static str>,
    /// Span of the initializer expression of the visited declarator; a
    /// function/arrow whose span matches inherits `declarator_name`.
    init_span: Option<Span>,
    /// [`NodeId`]s of the nodes currently being traversed (the ancestor
    /// chain); the top is the parent of the next node visited.
    node_stack: Vec<NodeId>,
    /// Indices into `functions` of the functions currently being traversed;
    /// the top receives every callee recorded while traversing.
    function_stack: Vec<usize>,
    /// Names of the bindings declared inside the functions currently being
    /// traversed (parameters, local declarations, nested function names);
    /// parallel to `function_stack`. Used to distinguish writes to local
    /// bindings from writes to outer bindings.
    locals: Vec<HashSet<&'static str>>,
}

impl FunctionCollector {
    pub(super) fn new() -> Self {
        Self {
            functions: Vec::new(),
            declarator_name: None,
            init_span: None,
            node_stack: Vec::new(),
            function_stack: Vec::new(),
            locals: Vec::new(),
        }
    }

    /// Records a write to the binding `name` in the innermost traversed
    /// function, unless `name` is one of its local bindings.
    fn mark_write(&mut self, name: &'static str) {
        let Some(locals) = self.locals.last_mut() else { return };
        if !locals.contains(&name) {
            let current = self.functions.last_mut().expect("locals non-empty implies functions");
            current.writes_outer.insert(name);
        }
    }

    /// Records a read of the binding `name` in the innermost traversed
    /// function, unless `name` is one of its local bindings.
    fn mark_read(&mut self, name: &'static str) {
        let Some(locals) = self.locals.last_mut() else { return };
        if !locals.contains(&name) {
            let current = self.functions.last_mut().expect("locals non-empty implies functions");
            current.reads_outer.insert(name);
        }
    }

    /// Checks an assignment target for writes to non-local bindings.
    fn check_simple_target(&mut self, target: &SimpleAssignmentTarget<'static>) {
        match target {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
                self.mark_write(id.name.as_str());
            }
            SimpleAssignmentTarget::StaticMemberExpression(member) => {
                if let Expression::Identifier(obj) = &member.object {
                    self.mark_write(obj.name.as_str());
                }
            }
            _ => {}
        }
    }

    fn check_assignment_target(&mut self, target: &AssignmentTarget<'static>) {
        match target {
            AssignmentTarget::AssignmentTargetIdentifier(id) => self.mark_write(id.name.as_str()),
            AssignmentTarget::StaticMemberExpression(member) => {
                if let Expression::Identifier(obj) = &member.object {
                    self.mark_write(obj.name.as_str());
                }
            }
            _ => {} // destructuring and other complex targets are not tracked
        }
    }
}

impl Visit<'static> for FunctionCollector {
    fn visit_variable_declarator(&mut self, decl: &VariableDeclarator<'static>) {
        let prev_name = self.declarator_name.take();
        let prev_init = self.init_span.take();
        self.declarator_name =
            if let BindingPattern::BindingIdentifier(id) = &decl.id { Some(id.name.as_str()) } else { None };
        self.init_span = decl.init.as_ref().map(|expr| expr.span());
        walk::walk_variable_declarator(self, decl);
        self.declarator_name = prev_name;
        self.init_span = prev_init;
    }

    fn enter_node(&mut self, kind: AstKind<'static>) {
        self.node_stack.push(kind.node_id());
    }

    fn leave_node(&mut self, _kind: AstKind<'static>) {
        self.node_stack.pop();
    }

    fn visit_function(&mut self, func: &Function<'static>, flags: ScopeFlags) {
        let parent_node_id = self.node_stack.last().copied().unwrap_or(NodeId::ROOT);
        let inherited = (self.init_span == Some(func.span)).then_some(self.declarator_name).flatten();
        self.functions.push(FunctionInfo {
            name: func.id.as_ref().map(|id| id.name.as_str()).or(inherited),
            span: func.span,
            r#type: func.r#type,
            generator: func.generator,
            arrow: false,
            node_id: func.node_id(),
            parent_node_id,
            callees: Vec::new(),
            writes_outer: BTreeSet::new(),
            reads_outer: BTreeSet::new(),
            purity: Purity::Depends,
        });
        self.function_stack.push(self.functions.len() - 1);
        self.locals.push(HashSet::new());
        walk::walk_function(self, func, flags);
        self.locals.pop();
        self.function_stack.pop();
    }

    fn visit_arrow_function_expression(&mut self, arrow: &ArrowFunctionExpression<'static>) {
        let parent_node_id = self.node_stack.last().copied().unwrap_or(NodeId::ROOT);
        let inherited = (self.init_span == Some(arrow.span)).then_some(self.declarator_name).flatten();
        self.functions.push(FunctionInfo {
            name: inherited,
            span: arrow.span,
            r#type: FunctionType::FunctionExpression,
            generator: false,
            arrow: true,
            node_id: arrow.node_id(),
            parent_node_id,
            callees: Vec::new(),
            writes_outer: BTreeSet::new(),
            reads_outer: BTreeSet::new(),
            purity: Purity::Depends,
        });
        self.function_stack.push(self.functions.len() - 1);
        self.locals.push(HashSet::new());
        walk::walk_arrow_function_expression(self, arrow);
        self.locals.pop();
        self.function_stack.pop();
    }

    fn visit_binding_identifier(&mut self, id: &BindingIdentifier<'static>) {
        if let Some(locals) = self.locals.last_mut() {
            locals.insert(id.name.as_str());
        }
        walk::walk_binding_identifier(self, id);
    }

    fn visit_identifier_reference(&mut self, id: &IdentifierReference<'static>) {
        self.mark_read(id.name.as_str());
        walk::walk_identifier_reference(self, id);
    }

    fn visit_assignment_expression(&mut self, expr: &AssignmentExpression<'static>) {
        self.check_assignment_target(&expr.left);
        walk::walk_assignment_expression(self, expr);
    }

    fn visit_update_expression(&mut self, expr: &UpdateExpression<'static>) {
        self.check_simple_target(&expr.argument);
        walk::walk_update_expression(self, expr);
    }

    fn visit_for_statement_left(&mut self, left: &ForStatementLeft<'static>) {
        if let Some(target) = left.as_assignment_target() {
            self.check_assignment_target(target);
        }
        walk::walk_for_statement_left(self, left);
    }

    fn visit_call_expression(&mut self, expr: &CallExpression<'static>) {
        if let Some(&index) = self.function_stack.last() {
            self.functions[index].callees.push(callee_of(&expr.callee));
        }
        walk::walk_call_expression(self, expr);
    }

    fn visit_new_expression(&mut self, expr: &NewExpression<'static>) {
        if let Some(&index) = self.function_stack.last() {
            self.functions[index].callees.push(callee_of(&expr.callee));
        }
        walk::walk_new_expression(self, expr);
    }
}

/// Classifies the callee expression of a call: direct identifier call,
/// static member call with a named object, or anything else.
fn callee_of(expr: &Expression<'static>) -> Callee {
    match expr {
        Expression::Identifier(id) => Callee::Identifier(id.name.as_str()),
        Expression::StaticMemberExpression(member) => match &member.object {
            Expression::Identifier(obj) => {
                Callee::Member { object: obj.name.as_str(), property: member.property.name.as_str() }
            }
            _ => Callee::Other,
        },
        _ => Callee::Other,
    }
}
