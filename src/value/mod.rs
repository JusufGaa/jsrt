use serde_json::{
    Number as JsonNumber,
    Value as JsonValue,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub enum PrimitiveValue {
    Bigint(num_bigint::BigInt),
    Boolean(bool),
    Null,
    Number(JsonNumber),
    String(String),
    Symbol(usize),
    Undefined,
}

impl From<JsonValue> for PrimitiveValue {
    fn from(value: JsonValue) -> Self {
        match value {
            JsonValue::Null => PrimitiveValue::Null,
            JsonValue::Bool(b) => PrimitiveValue::Boolean(b),
            JsonValue::Number(n) => PrimitiveValue::Number(n),
            JsonValue::String(s) => PrimitiveValue::String(s),
            _ => PrimitiveValue::Undefined,
        }
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Name {
    String(String),
    Symbol(usize),
}

enum ValueWithIndex {
    Primitive(PrimitiveValue),
    Index(usize),
}

enum ValueNode {
    Primitive(PrimitiveValue),
    Array(Vec<ValueWithIndex>),
    Object(BTreeMap<Name, ValueWithIndex>),
}

pub struct Value {
    root: ValueWithIndex,
    nodes: Vec<ValueNode>,
}

/// 对象/数组键值集合：BTreeMap<String, V>
pub type Map<V, K = String> = BTreeMap<K, V>;

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

/// 把子 Value 的节点合并进父节点的存储，返回指向子根的索引或原始值
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

impl fmt::Display for PrimitiveValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrimitiveValue::Bigint(n) => write!(f, "{}", n),
            PrimitiveValue::Boolean(b) => write!(f, "{}", b),
            PrimitiveValue::Null => f.write_str("null"),
            PrimitiveValue::Number(n) => write!(f, "{}", n),
            PrimitiveValue::String(s) => f.write_str(s),
            PrimitiveValue::Symbol(i) => write!(f, "Symbol({})", i),
            PrimitiveValue::Undefined => f.write_str("undefined"),
        }
    }
}

impl PrimitiveValue {
    /// JSON 上下文中的输出：字符串需要加引号并转义
    fn fmt_json(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrimitiveValue::String(s) => write!(f, "\"{}\"", escape_json_string(s)),
            other => fmt::Display::fmt(other, f),
        }
    }
}

impl Name {
    fn fmt_json(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Name::String(s) => write!(f, "\"{}\"", escape_json_string(s)),
            Name::Symbol(i) => write!(f, "Symbol({})", i),
        }
    }
}

fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
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
                    name.fmt_json(f)?;
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
