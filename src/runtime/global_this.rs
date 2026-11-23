use crate::js_value::JsValue;

pub(crate) enum NodeOperation {
    Insert(String, JsValue),
    Delete(String),
    Update(String, JsValue),
}

pub struct GlobalThisLogPage {
    operations: Vec<NodeOperation>,
}

pub struct GlobalThis {
    pages: Vec<GlobalThisLogPage>,
}

impl GlobalThisLogPage {
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
}

impl GlobalThis {
    pub fn new() -> Self {
        Self {
            pages: vec![],
        }
    }

    pub fn push(&mut self, new_page: GlobalThisLogPage) {
        self.pages.push(new_page);
    }

    pub fn pop(&mut self) -> Option<GlobalThisLogPage> {
        if self.pages.len() <= 1 {
            return None;
        }
        self.pages.pop()
    }
}
