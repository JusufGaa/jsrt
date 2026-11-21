use serde_json::Value as JsonValue;
use std::collections::{BTreeSet, HashMap};
use std::fmt;

pub mod collection;
pub mod primitive_value;
pub mod symbol;
pub mod value_or_point;
pub mod context;

pub use primitive_value::PrimitiveValue;
pub use symbol::Symbol;

/// 可以作为属性名（`String` / `&str` / `&'static Symbol`）。
pub trait Name {}

impl Name for String {}
impl Name for &str {}
impl Name for &'static Symbol {}

/// 可以作为索引（`String` / `&str` / `&'static Symbol` / `usize`）。
pub trait Index {}

impl Index for String {}
impl Index for &str {}
impl Index for &'static Symbol {}
impl Index for usize {}

#[derive(Clone)]
pub(crate) enum ValueWithIndex {
    Primitive(PrimitiveValue),
    Index(usize),
}

#[derive(Clone)]
pub(crate) enum ValueNode {
    Primitive(PrimitiveValue),
    Array(Vec<ValueWithIndex>),
    Object(HashMap<String, ValueWithIndex>),
}

pub struct Value {
    pub(crate) root: ValueWithIndex,
    pub(crate) nodes: Vec<ValueNode>,
}

pub type Map<V, K = String> = HashMap<K, V>;

#[allow(non_snake_case)]
impl Value {
    pub fn new() -> Self {
        Self {
            root: ValueWithIndex::Primitive(PrimitiveValue::Undefined),
            nodes: vec![],
        }
    }

    pub fn Primitive(p: PrimitiveValue) -> Self {
        Self {
            root: ValueWithIndex::Primitive(p),
            nodes: vec![],
        }
    }

    pub fn Array(items: Vec<Value>) -> Self {
        let mut nodes = Vec::new();
        let children = items
            .into_iter()
            .map(|child| merge_child(child, &mut nodes))
            .collect();
        nodes.push(ValueNode::Array(children));
        Self {
            root: ValueWithIndex::Index(nodes.len() - 1),
            nodes,
        }
    }

    pub fn Object(map: Map<Value>) -> Self {
        let mut nodes = Vec::new();
        let mut entries = HashMap::new();
        for (k, v) in map {
            entries.insert(k, merge_child(v, &mut nodes));
        }
        nodes.push(ValueNode::Object(entries));
        Self {
            root: ValueWithIndex::Index(nodes.len() - 1),
            nodes,
        }
    }
}

fn merge_child(child: Value, nodes: &mut Vec<ValueNode>) -> ValueWithIndex {
    let Value { root, nodes: mut child_nodes } = child;
    match root {
        // 原始值没有子节点，直接上移
        root @ ValueWithIndex::Primitive(_) => {
            debug_assert!(child_nodes.is_empty());
            root
        }
        // 复合值：平移其节点索引后并入
        ValueWithIndex::Index(i) => {
            let offset = nodes.len();
            nodes.append(&mut child_nodes);
            ValueWithIndex::Index(i + offset)
        }
    }
}

impl From<JsonValue> for Value {
    fn from(value: JsonValue) -> Self {
        let mut nodes = Vec::new();
        let root = build(value, &mut nodes);
        Value { root, nodes }
    }
}

fn build(value: JsonValue, nodes: &mut Vec<ValueNode>) -> ValueWithIndex {
    match value {
        JsonValue::Array(items) => {
            let children = items.into_iter().map(|v| build(v, nodes)).collect();
            nodes.push(ValueNode::Array(children));
            ValueWithIndex::Index(nodes.len() - 1)
        }
        JsonValue::Object(map) => {
            let mut entries = HashMap::new();
            for (k, v) in map {
                entries.insert(k, build(v, nodes));
            }
            nodes.push(ValueNode::Object(entries));
            ValueWithIndex::Index(nodes.len() - 1)
        }
        _ => ValueWithIndex::Primitive(PrimitiveValue::from(value)),
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut path = BTreeSet::new();
        match &self.root {
            ValueWithIndex::Primitive(p) => fmt::Display::fmt(p, f),
            ValueWithIndex::Index(i) => self.fmt_node(*i, f, &mut path),
        }
    }
}

impl Value {
    fn fmt_node(
        &self,
        index: usize,
        f: &mut fmt::Formatter<'_>,
        path: &mut BTreeSet<usize>,
    ) -> fmt::Result {
        if path.contains(&index) {
            return write!(f, "[Circular *{}]", index);
        }
        path.insert(index);
        let result = match &self.nodes[index] {
            ValueNode::Primitive(p) => p.fmt_json(f),
            ValueNode::Array(items) => {
                f.write_str("[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    self.fmt_value(item, f, path)?;
                }
                f.write_str("]")
            }
            ValueNode::Object(map) => {
                f.write_str("{")?;
                let mut first = true;
                for (name, value) in map {
                    if !first {
                        f.write_str(",")?;
                    }
                    first = false;
                    f.write_str(&crate::utils::format_string(name, '"'))?;
                    f.write_str(":")?;
                    self.fmt_value(value, f, path)?;
                }
                f.write_str("}")
            }
        };
        path.remove(&index);
        result
    }

    /// Formats a single element, delegating to its node when indexed.
    fn fmt_value(
        &self,
        value: &ValueWithIndex,
        f: &mut fmt::Formatter<'_>,
        path: &mut BTreeSet<usize>,
    ) -> fmt::Result {
        match value {
            ValueWithIndex::Primitive(p) => p.fmt_json(f),
            ValueWithIndex::Index(child) => self.fmt_node(*child, f, path),
        }
    }
}
