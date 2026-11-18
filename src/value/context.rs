use std::collections::HashMap;
use super::name::Name;
use super::primitive_value::PrimitiveValue;

enum ValueOrPoint {
    Value(PrimitiveValue),
    Point(usize),
}

enum Value {
    PriObject(HashMap<Name, PrimitiveValue>),
    PriArray(Vec<PrimitiveValue>),
    PtObject(HashMap<Name, usize>),
    PtArray(Vec<usize>),
    Object(HashMap<Name, ValueOrPoint>),
    Array(Vec<ValueOrPoint>),
}



pub struct Context {
    heap: Vec<Value>,
    root: Vec<usize>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            heap: Vec::new(),
            root: Vec::new(),
        }
    }

    pub fn gc(&mut self) {
        let marked = self.gc_mark();
        let remap = self.gc_remap(&marked);

        // 先修正存活对象内部的指针，再搬运
        let update = |p: &mut usize| *p = remap[*p];
        for i in 0..self.heap.len() {
            if !marked[i] {
                continue;
            }
            match &mut self.heap[i] {
                // PriObject/PriArray 只含原始值，无指针
                Value::PriObject(_) | Value::PriArray(_) => {}
                Value::PtObject(map) => map.values_mut().for_each(&mut update),
                Value::PtArray(items) => items.iter_mut().for_each(&mut update),
                Value::Object(map) => {
                    for v in map.values_mut() {
                        if let ValueOrPoint::Point(p) = v {
                            update(p);
                        }
                    }
                }
                Value::Array(items) => {
                    for v in items.iter_mut() {
                        if let ValueOrPoint::Point(p) = v {
                            update(p);
                        }
                    }
                }
            }
        }
        for r in &mut self.root {
            *r = remap[*r];
        }

        // 压缩阶段：原地搬家，用双指针把存活对象换到堆前部，末尾垃圾截断丢弃
        let mut next = 0;
        for i in 0..self.heap.len() {
            if marked[i] {
                self.heap.swap(next, i);
                next += 1;
            }
        }
        self.heap.truncate(next);
    }

    /// 计算新旧索引映射：存活对象按原顺序紧凑排列
    fn gc_remap(&self, marked: &[bool]) -> Vec<usize> {
        let mut remap = vec![usize::MAX; self.heap.len()];
        let mut next = 0;
        for i in 0..self.heap.len() {
            if marked[i] {
                remap[i] = next;
                next += 1;
            }
        }
        remap
    }

    /// 标记阶段：深度优先遍历，收集所有从根可达的索引
    fn gc_mark(&self) -> Vec<bool> {
        let mut marked = vec![false; self.heap.len()];
        let mut stack = self.root.clone();
        while let Some(i) = stack.pop() {
            if i >= self.heap.len() || marked[i] {
                continue;
            }
            marked[i] = true;
            match &self.heap[i] {
                // PriObject/PriArray 只含原始值，无子引用
                Value::PriObject(_) | Value::PriArray(_) => {}
                Value::PtObject(map) => stack.extend(map.values().copied()),
                Value::PtArray(items) => stack.extend(items.iter().copied()),
                Value::Object(map) => {
                    for v in map.values() {
                        if let ValueOrPoint::Point(p) = v {
                            stack.push(*p);
                        }
                    }
                }
                Value::Array(items) => {
                    for v in items {
                        if let ValueOrPoint::Point(p) = v {
                            stack.push(*p);
                        }
                    }
                }
            }
        }
        marked
    }
}