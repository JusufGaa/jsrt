use std::collections::{BTreeMap, BTreeSet};

use crate::value::{Name, Value, ValueNode, ValueWithIndex};

pub(crate) enum NodeOperation {
    Insert(Name, Value),
    Delete(Name),
    Update(Name, Value),
}

pub struct GlobalThisLogPage {
    parent: Option<Box<Self>>,
    operations: Vec<NodeOperation>,
}

pub struct GlobalThis {
    root: GlobalThisLogPage,
    public_names: Vec<Name>,
}

impl GlobalThisLogPage {
    pub fn new() -> Self {
        Self {
            parent: None,
            operations: Vec::new(),
        }
    }

    fn push_operation(&mut self, operation: NodeOperation) {
        self.operations.push(operation);
    }

    pub fn insert(&mut self, name: Name, value: Value) {
        self.push_operation(NodeOperation::Insert(name, value));
    }

    pub fn delete(&mut self, name: Name) {
        self.push_operation(NodeOperation::Delete(name));
    }

    pub fn update(&mut self, name: Name, value: Value) {
        self.push_operation(NodeOperation::Update(name, value));
    }
}

impl GlobalThis {
    pub fn new() -> Self {
        Self {
            root: GlobalThisLogPage::new(),
            public_names: Vec::new(),
        }
    }

    pub fn push(&mut self, mut new_value: GlobalThisLogPage) {
        let old = std::mem::replace(&mut self.root, new_value);
        self.root.parent = Some(Box::new(old));
        for op in &self.root.operations {
            match op {
                NodeOperation::Insert(name, _) | NodeOperation::Update(name, _) => {
                    if !self.public_names.contains(name) {
                        self.public_names.push(name.clone());
                    }
                }
                NodeOperation::Delete(name) => {
                    self.public_names.retain(|n| n != name);
                }
            }
        }
    }

    pub fn pop(&mut self) -> Option<GlobalThisLogPage> {
        let parent = self.root.parent.take()?;
        let popped = std::mem::replace(&mut self.root, *parent);
        for op in popped.operations.iter().rev() {
            match op {
                NodeOperation::Insert(name, _) | NodeOperation::Update(name, _) => {
                    self.public_names.retain(|n| n != name);
                }
                NodeOperation::Delete(name) => {
                    if !self.public_names.contains(name) {
                        self.public_names.push(name.clone());
                    }
                }
            }
        }
        Some(popped)
    }
}
