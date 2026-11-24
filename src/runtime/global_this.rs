use std::collections::HashMap;

use crate::js_value::JsValue;

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

/// Turn a snapshot's operations into JS statements replaying them.
pub(crate) fn operation_statements(operations: &[NodeOperation]) -> Vec<String> {
    operations.iter().map(|operation| match operation {
        NodeOperation::Insert(name, value) | NodeOperation::Update(name, value) => {
            let key = serde_json::to_string(name).unwrap();
            format!("globalThis[{}] = {};", key, js_value_to_source(value))
        }
        NodeOperation::Delete(name) => {
            let key = serde_json::to_string(name).unwrap();
            format!("delete globalThis[{}];", key)
        }
    }).collect()
}

/// Serialize a [`JsValue`] back into JS source, so it can be assigned
/// to `globalThis` when restoring a snapshot.
pub(crate) fn js_value_to_source(value: &JsValue) -> String {
    if value.is_undefined() {
        return "undefined".into();
    }
    if value.is_null() {
        return "null".into();
    }
    if value.is_boolean() {
        return if value.as_bool().unwrap() { "true" } else { "false" }.into();
    }
    if value.is_number() {
        let n = value.as_number().unwrap();
        if n.is_nan() {
            return "Number.NaN".into();
        }
        if n.is_infinite() {
            return if n > 0.0 {
                "Number.POSITIVE_INFINITY".into()
            } else {
                "Number.NEGATIVE_INFINITY".into()
            };
        }
        return format!("{}", n);
    }
    if value.is_bigint() {
        return format!("{}n", value.as_bigint().unwrap());
    }
    if value.is_string() {
        return serde_json::to_string(value.as_string().unwrap_or("")).unwrap();
    }
    if value.is_object() {
        let obj = value.as_object().unwrap();
        if obj.is_array() {
            let items: Vec<String> = obj.elements().iter().map(js_value_to_source).collect();
            return format!("[{}]", items.join(","));
        }
        let props: Vec<String> = obj.props().iter().filter_map(|(key, value)| {
            let key = key.as_str()?;
            Some(format!("{}: {}", serde_json::to_string(key).unwrap(), js_value_to_source(value)))
        }).collect();
        return format!("{{{}}}", props.join(","));
    }

    "undefined".into()
}
