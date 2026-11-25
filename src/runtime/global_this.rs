use std::collections::HashMap;

use crate::js_value::{JsClass, JsInternal, JsValue};

pub(crate) enum NodeOperation {
    Insert(String, JsValue),
    Delete(String),
    Update(String, JsValue),
}

pub struct GlobalThisSnapshot {
    operations: Vec<NodeOperation>,
}

impl GlobalThisSnapshot {
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
        }
    }

    fn push_operation(&mut self, operation: NodeOperation) {
        self.operations.push(operation);
    }

    pub fn insert(&mut self, name: String, value: JsValue) {
        self.push_operation(NodeOperation::Insert(name, value));
    }

    pub fn delete(&mut self, name: String) {
        self.push_operation(NodeOperation::Delete(name));
    }

    pub fn update(&mut self, name: String, value: JsValue) {
        self.push_operation(NodeOperation::Update(name, value));
    }

    /// Operations recorded on this snapshot, in order.
    pub fn operations(&self) -> &[NodeOperation] {
        &self.operations
    }
}

/// The tracked state of `globalThis`: variable name -> value.
///
/// TODO: `values` are never released (`JsValue` is `Copy` and holds no
/// `Drop`), so every `save` / `restore` cycle leaks the refcounted
/// payloads it tracks. Long-running loops accumulate memory; introduce
/// a proper ownership discipline (`dup_value` on store, `free_value`
/// on drop) so the state can be freed.
// GlobalThis/snapshot 目前按既有设计不释放持有的 JsValue（泄漏而非悬垂），
// 长期运行的 save/restore 循环会累积内存，这是历史设计，本次未动。
pub struct GlobalThis {
    values: HashMap<String, JsValue>,
}

impl GlobalThis {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    pub fn insert(&mut self, name: String, value: JsValue) {
        self.values.insert(name, value);
    }

    pub fn delete(&mut self, name: &str) {
        self.values.remove(name);
    }

    pub fn get(&self, name: &str) -> Option<&JsValue> {
        self.values.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &JsValue)> {
        self.values.iter()
    }
}

/// Builds a self-contained JS block rebuilding a set of values while
/// preserving shared references and cycles.
///
/// Every distinct object is declared as a local variable up front, then
/// all properties / elements are wired by reference, so cycles resolve
/// to the very same object instead of nested copies.
struct GraphSerializer {
    /// Object id -> local variable name.
    registry: HashMap<usize, String>,
    /// `var oN = {}` / `var oN = new Date(...)` declarations.
    creations: Vec<String>,
    /// Property / element writes referencing the declared variables.
    assignments: Vec<String>,
    next_id: usize,
}

impl GraphSerializer {
    fn new() -> Self {
        Self {
            registry: HashMap::new(),
            creations: Vec::new(),
            assignments: Vec::new(),
            next_id: 0,
        }
    }

    /// JS expression for `value`: a literal inline, or the variable of
    /// an already declared object.
    fn visit(&mut self, value: JsValue) -> String {
        let Some(id) = value.object_id() else {
            // Literals (and symbols, which degrade to `undefined`).
            return value.to_source();
        };

        // Shared or cyclic reference: point at the same variable.
        if let Some(var) = self.registry.get(&id) {
            return var.clone();
        }
        let var = format!("o{}", self.next_id);
        self.next_id += 1;
        self.registry.insert(id, var.clone());

        let obj = value.as_object().unwrap();
        match obj.class() {
            JsClass::Array => {
                self.creations.push(format!("var {var} = [];"));
                for (index, element) in obj.elements().iter().enumerate() {
                    let expr = self.visit(*element);
                    self.assignments.push(format!("{var}[{index}] = {expr};"));
                }
            }
            JsClass::Object | JsClass::Error => {
                self.creations.push(format!("var {var} = {{}};"));
                for (key, prop) in obj.props() {
                    if let Some(key) = key.as_str() {
                        let expr = self.visit(*prop);
                        let key = serde_json::to_string(key).unwrap();
                        self.assignments.push(format!("{var}[{key}] = {expr};"));
                    }
                }
            }
            // Built-in classes (Date / RegExp / Map / Set / ArrayBuffer):
            // replay their private data inline; no further traversal.
            _ => {
                let source = obj
                    .internal()
                    .and_then(JsInternal::to_source)
                    .unwrap_or_else(|| "undefined".into());
                self.creations.push(format!("var {var} = {source};"));
            }
        }
        var
    }

    /// Drain the pending declarations and property writes into `out`.
    fn flush(&mut self, out: &mut Vec<String>) {
        out.append(&mut self.creations);
        out.append(&mut self.assignments);
    }
}

/// JS code rebuilding `entries` as `globalThis` bindings, preserving
/// shared references and cycles across the whole batch. Empty entries
/// yield empty code.
pub(crate) fn graph_replay(entries: &[(String, JsValue)]) -> String {
    let mut serializer = GraphSerializer::new();
    let mut body = Vec::new();
    for (name, value) in entries {
        let expr = serializer.visit(*value);
        serializer.flush(&mut body);
        let key = serde_json::to_string(name).unwrap();
        body.push(format!("globalThis[{key}] = {expr};"));
    }
    if body.is_empty() {
        return String::new();
    }
    // Wrap in an IIFE so the helper variables stay out of `globalThis`.
    format!("(function() {{\n{}\n}})();", body.join("\n"))
}

/// Resolve a `snapshot_go` index against a history of `len` snapshots.
///
/// Positive indices count from the front, negative indices count back
/// from the end (`len + index`, Python-style). Returns `None` when the
/// resolved position is out of range.
pub(crate) fn resolve_snapshot_index(len: usize, index: isize) -> Option<usize> {
    let len = len as isize;
    let resolved = if index < 0 { len + index } else { index };
    if resolved < 0 || resolved >= len {
        None
    } else {
        Some(resolved as usize)
    }
}

/// JS statements replaying `snapshots` up to and including `end`
/// (snapshots are deltas, so everything before `end` is replayed too).
/// The object graph is rebuilt through [`graph_replay`], so shared
/// references and cycles survive the replay; the whole sequence runs
/// inside one IIFE and shares a single object registry.
pub(crate) fn replay_statements(snapshots: &[GlobalThisSnapshot], end: usize) -> String {
    let mut serializer = GraphSerializer::new();
    let mut statements = Vec::new();
    for snapshot in &snapshots[..=end] {
        for operation in snapshot.operations() {
            match operation {
                NodeOperation::Insert(name, value) | NodeOperation::Update(name, value) => {
                    let expr = serializer.visit(*value);
                    serializer.flush(&mut statements);
                    let key = serde_json::to_string(name).unwrap();
                    statements.push(format!("globalThis[{key}] = {expr};"));
                }
                NodeOperation::Delete(name) => {
                    let key = serde_json::to_string(name).unwrap();
                    statements.push(format!("delete globalThis[{key}];"));
                }
            }
        }
    }
    if statements.is_empty() {
        return String::new();
    }
    format!("(function() {{\n{}\n}})();", statements.join("\n"))
}

/// The tracked state produced by replaying `snapshots` up to and
/// including `end`, by folding `Insert` / `Update` / `Delete` in order.
pub(crate) fn replay_state(snapshots: &[GlobalThisSnapshot], end: usize) -> GlobalThis {
    let mut state = GlobalThis::new();
    for snapshot in &snapshots[..=end] {
        for operation in snapshot.operations() {
            match operation {
                NodeOperation::Insert(name, value) | NodeOperation::Update(name, value) => {
                    state.insert(name.clone(), *value);
                }
                NodeOperation::Delete(name) => state.delete(name),
            }
        }
    }
    state
}
