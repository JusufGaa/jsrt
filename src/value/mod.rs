use serde_json::Value as JsonValue;
use std::collections::{BTreeSet, HashMap};
use std::fmt;

pub mod name;
pub mod primitive_value;
pub mod symbol;

pub use name::Name;
pub use primitive_value::PrimitiveValue;
pub use symbol::Symbol;

#[derive(Clone)]
pub(crate) enum ValueWithIndex {
    Primitive(PrimitiveValue),
    Index(usize),
}

#[derive(Clone)]
pub(crate) enum ValueNode {
    Primitive(PrimitiveValue),
    Array(Vec<ValueWithIndex>),
    Object(HashMap<Name, ValueWithIndex>),
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
        let entries = map
            .into_iter()
            .map(|(k, v)| (Name::String(k), merge_child(v, &mut nodes)))
            .collect();
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
            let children = map
                .into_iter()
                .map(|(k, v)| (Name::String(k), build(v, nodes)))
                .collect();
            nodes.push(ValueNode::Object(children));
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
                    match item {
                        ValueWithIndex::Primitive(p) => p.fmt_json(f)?,
                        ValueWithIndex::Index(child) => self.fmt_node(*child, f, path)?,
                    }
                }
                f.write_str("]")
            }
            ValueNode::Object(map) => {
                f.write_str("{")?;
                for (i, (name, value)) in map.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    write!(f, "{}", name)?;
                    f.write_str(":")?;
                    match value {
                        ValueWithIndex::Primitive(p) => p.fmt_json(f)?,
                        ValueWithIndex::Index(child) => self.fmt_node(*child, f, path)?,
                    }
                }
                f.write_str("}")
            }
        };
        path.remove(&index);
        result
    }
}
