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
/// 分步式代理读取：ctx.get(index) 得到 Pending（过程态），pending.get(name) 继续解析
pub(crate) trait Get<T> {
    fn get<'a>(&'a self, key: T) -> Pending<'a>;
}

/// 代理式读取的游标：要么停在 Context 的堆节点上（过程态），要么已落到原始值（终态）
pub(crate) enum Pending<'a> {
    /// 过程状态：仍在堆上，指向槽位，可继续按属性读取；读取失败也停在此态
    Process(&'a Context, usize),
    /// 最终状态：已解析到原始值
    Final(&'a PrimitiveValue),
}

impl Get<usize> for Context {
    fn get<'b>(&'b self, index: usize) -> Pending<'b> {
        Pending::Process(self, index)
    }
}

impl Get<Name> for Pending<'_> {
    fn get<'b>(&'b self, name: Name) -> Pending<'b> {
        match self {
            Pending::Process(ctx, index) => {
                let (ctx, index) = (*ctx, *index);
                let mut path = Vec::new();
                let mut cursor = index;
                let found = loop {
                    // 循环引用检测：游标回到已访问过的槽位
                    if path.contains(&cursor) {
                        break None;
                    }
                    path.push(cursor);
                    match ctx.heap.get(cursor) {
                        // PriObject 直接命中原始值
                        Some(Value::PriObject(map)) => break map.get(&name),
                        // 数组没有命名属性，槽位不存在同样视为读不到
                        None | Some(Value::PriArray(_) | Value::PtArray(_) | Value::Array(_)) => {
                            break None
                        }
                        // PtObject：属性是指针，前进到目标槽位继续读
                        Some(Value::PtObject(map)) => match map.get(&name) {
                            Some(&next) => cursor = next,
                            None => break None,
                        },
                        // Object：属性是原始值则命中，是指针则前进继续读
                        Some(Value::Object(map)) => match map.get(&name) {
                            Some(ValueOrPoint::Value(p)) => break Some(p),
                            Some(ValueOrPoint::Point(i)) => cursor = *i,
                            None => break None,
                        },
                    }
                };
                match found {
                    Some(p) => Pending::Final(p),
                    // 读不到：游标停在原地（等价于 None）
                    None => Pending::Process(ctx, index),
                }
            }
            // 终态：原始值没有属性，保持不变
            Pending::Final(p) => Pending::Final(*p),
        }
    }
}

impl<'a> Pending<'a> {
    /// 已落到终态（解析到原始值）
    pub(crate) fn is_some(&self) -> bool {
        matches!(self, Pending::Final(_))
    }

    /// 仍处于过程态（尚未确认）
    pub(crate) fn is_none(&self) -> bool {
        matches!(self, Pending::Process(..))
    }

    /// 确认读取，仍处于过程态时 panic
    pub(crate) fn unwrap(self) -> &'a PrimitiveValue {
        match self {
            Pending::Final(p) => p,
            Pending::Process(..) => panic!("called `Pending::unwrap()` on a pending value"),
        }
    }

    /// 确认读取，仍处于过程态时 panic 并附带自定义信息
    pub(crate) fn expect(self, msg: &str) -> &'a PrimitiveValue {
        match self {
            Pending::Final(p) => p,
            Pending::Process(..) => panic!("{}", msg),
        }
    }

    /// 确认读取，仍处于过程态时返回默认值
    pub(crate) fn unwrap_or(self, default: &'a PrimitiveValue) -> &'a PrimitiveValue {
        match self {
            Pending::Final(p) => p,
            Pending::Process(..) => default,
        }
    }

    /// 确认读取，仍处于过程态时用闭包计算默认值
    pub(crate) fn unwrap_or_else(
        self,
        f: impl FnOnce() -> &'a PrimitiveValue,
    ) -> &'a PrimitiveValue {
        match self {
            Pending::Final(p) => p,
            Pending::Process(..) => f(),
        }
    }

    /// 对终态值做映射，过程态返回 None
    pub(crate) fn map<U>(self, f: impl FnOnce(&'a PrimitiveValue) -> U) -> Option<U> {
        match self {
            Pending::Final(p) => Some(f(p)),
            Pending::Process(..) => None,
        }
    }

    /// 对终态值继续做可失败的映射，过程态返回 None
    pub(crate) fn and_then<U>(
        self,
        f: impl FnOnce(&'a PrimitiveValue) -> Option<U>,
    ) -> Option<U> {
        match self {
            Pending::Final(p) => f(p),
            Pending::Process(..) => None,
        }
    }

    /// 过程态时返回备选项
    pub(crate) fn or(self, other: Option<&'a PrimitiveValue>) -> Option<&'a PrimitiveValue> {
        match self {
            Pending::Final(p) => Some(p),
            Pending::Process(..) => other,
        }
    }

    /// 过程态时用闭包计算备选项
    pub(crate) fn or_else(
        self,
        f: impl FnOnce() -> Option<&'a PrimitiveValue>,
    ) -> Option<&'a PrimitiveValue> {
        match self {
            Pending::Final(p) => Some(p),
            Pending::Process(..) => f(),
        }
    }

    /// 转换为 Result，过程态时返回错误
    pub(crate) fn ok_or<E>(self, err: E) -> Result<&'a PrimitiveValue, E> {
        match self {
            Pending::Final(p) => Ok(p),
            Pending::Process(..) => Err(err),
        }
    }

    /// 转换为 Result，过程态时用闭包计算错误
    pub(crate) fn ok_or_else<E>(
        self,
        err: impl FnOnce() -> E,
    ) -> Result<&'a PrimitiveValue, E> {
        match self {
            Pending::Final(p) => Ok(p),
            Pending::Process(..) => Err(err()),
        }
    }

    /// 仅当终态值满足谓词时保留
    pub(crate) fn filter(
        self,
        pred: impl FnOnce(&&'a PrimitiveValue) -> bool,
    ) -> Option<&'a PrimitiveValue> {
        match self {
            Pending::Final(p) if pred(&p) => Some(p),
            _ => None,
        }
    }
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
        self.gc_repoint(&marked);

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

    #[inline]
    fn gc_repoint(&mut self, marked: &[bool]) {
        let remap = self.gc_remap(marked);
        let mut update = |p: &mut usize| *p = remap[*p];
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
    }

    #[inline]
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

    #[inline]
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