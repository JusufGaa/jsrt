use std::rc::Rc;
use std::cell::RefCell;
use v8::{
    Global, OwnedIsolate, PinScope, Context,
    Local, Value as V8Value, Array, GetPropertyNamesArgs,
    KeyCollectionMode,
};
use serde_json::{
    Number as JsonNumber,
    Value as JsonValue,
};
use std::collections::HashSet;
use crate::runtime::value;

pub struct V8Worker {
    file_name: Option<String>,
    isolate: Rc<RefCell<OwnedIsolate>>,
    context: Rc<RefCell<Global<Context>>>,
    snapshots: Rc<RefCell<Vec<JsonValue>>>,
}

pub struct V8ScriptOrigin {
    resource_line_offset: i32,
    resource_column_offset: i32,
    resource_is_shared_cross_origin: bool,
    script_id: i32,
    resource_is_opaque: bool,
    is_wasm: bool,
    is_module: bool,
}

impl V8Worker {
    pub fn new() -> Self {
        let mut isolate = v8::Isolate::new(Default::default());
    
        let context = {
            let mut pinned =
                std::pin::pin!(v8::HandleScope::<Context>::new(&mut isolate));
            let scope = pinned.as_mut().init();

            let ctx = v8::Context::new(&scope, Default::default());
            Global::new(&scope, ctx)
        };

        V8Worker {
            file_name: None,
            isolate: Rc::new(RefCell::new(isolate)),
            context: Rc::new(RefCell::new(context)),
            snapshots: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn eval_with_origin(&mut self, code: &str, origin: impl Into<Option<V8ScriptOrigin>>) -> Option<value::Value> { 
        let mut isolate = self.isolate.borrow_mut();
        let context = self.context.borrow();

        let mut pinned =
                std::pin::pin!(v8::HandleScope::new(&mut *isolate));
        let mut scope = pinned.as_mut().init();

        let ctx = Global::<Context>::new(&scope, &*context);
        let ctx = Local::new(&scope, ctx);

        let mut ctx_scope = v8::ContextScope::new(&mut scope, ctx);
        let mut pinned2 = std::pin::pin!(v8::HandleScope::new(&mut ctx_scope));
        let scope2 = pinned2.as_mut().init();
        let resource_name = if let Some(ref file_name) = self.file_name {
            v8::String::new(&scope2, &*file_name)
        } else {
            v8::String::new(&scope2, "repl")
        };
        match resource_name {
            None => return  None,
            _ => {}
        }
        if let Some(code) = v8::String::new(&scope2, code) {
            if let Some(script) = match origin.into() {
                Some(origin) => {
                    let origin = v8::ScriptOrigin::new(&scope2,
                        resource_name.unwrap().into(), 
                        origin.resource_line_offset, 
                        origin.resource_column_offset, 
                        origin.resource_is_shared_cross_origin, 
                        origin.script_id, 
                        None, 
                        origin.resource_is_opaque, 
                        origin.is_wasm, 
                        origin.is_module, 
                        None);
                    v8::Script::compile(&scope2, code, Some(&origin))
                },
                _ => v8::Script::compile(&scope2, code, None)
            } {
                if let Some(result) = script.run(&scope2) {
                    return Some(v8_value_to_value(&scope2, result.into()))
                }
            }
        }
        None
    }
}

impl super::WorkerPrivate for V8Worker {
    fn set_file_name(&mut self, file_name: impl Into<String>) {
        self.file_name = Some(file_name.into());
    }
}

impl super::Worker for V8Worker {
    fn save(&mut self) {
        let mut isolate = self.isolate.borrow_mut();
        let context = self.context.borrow();

        let mut pinned =
            std::pin::pin!(v8::HandleScope::new(&mut *isolate));
        let mut scope = pinned.as_mut().init();

        let ctx = Local::new(&scope, &*context);
        let mut ctx_scope = v8::ContextScope::new(&mut scope, ctx);
        let mut pinned2 = std::pin::pin!(v8::HandleScope::new(&mut ctx_scope));
        let scope2 = pinned2.as_mut().init();

        let global = ctx.global(&scope2);
        let snapshot = v8_value_to_json(&scope2, global.into());

        self.snapshots.borrow_mut().push(snapshot);
    }

    fn restore(&mut self) {
        let context = {
            let isolate = &mut *self.isolate.borrow_mut();
            let mut pinned =
                std::pin::pin!(v8::HandleScope::<Context>::new(isolate));
            let scope = pinned.as_mut().init();

            let ctx = v8::Context::new(&scope, Default::default());
            Global::new(&scope, ctx)
        };
        self.context.replace(context);

        let t = self.snapshots.borrow_mut().pop();
        if let Some(snap) = t {
            let code = snap.as_object().unwrap().iter().map(|(key, value)| {
                format!("var {} = {};", key, value)
            }).reduce(|a, b| format!("{}\n{}", a, b)).unwrap();

            self.eval_with_origin(&*code, None);
        }
    }

    fn eval(&mut self, code: impl Into<String>) -> value::Value {
        self.eval_with_origin(&* code.into(), V8ScriptOrigin {
            resource_line_offset: 0,
            resource_column_offset: 0,
            resource_is_shared_cross_origin: false,
            script_id: -1,
            resource_is_opaque: true,
            is_wasm: false,
            is_module: false,
        }).unwrap_or(value::Value::Primitive(value::PrimitiveValue::Null))
    }
}

fn v8_value_to_json(
    scope: &PinScope<'_, '_>, 
    value: Local<'_, V8Value>,
) -> JsonValue {
    if let Some(str) = v8::json::stringify(scope, value) {
        let str = str.to_rust_string_lossy(scope);
        if let Ok(result) = serde_json::from_str(&*str) {
            return result;
        }
    }

    JsonValue::Null
}

fn v8_value_to_value(
    scope: &PinScope<'_, '_>,
    value: Local<'_, V8Value>,
) -> value::Value {
    let mut visited = HashSet::new();
    v8_value_to_value_impl(scope, value, &mut visited)
}

fn v8_value_to_primitive_value(
    scope: &PinScope<'_, '_>,
    value: Local<'_, V8Value>,
) -> value::PrimitiveValue {
    if value.is_undefined() {
        return value::PrimitiveValue::Undefined;
    }
    if value.is_null() {
        return value::PrimitiveValue::Null;
    }
    if value.is_boolean() {
        let b = value.boolean_value(scope);
        return value::PrimitiveValue::Boolean(b);
    }
    if value.is_number() {
        let n = value.number_value(scope).unwrap_or(0.0);
        return value::PrimitiveValue::Number(
            JsonNumber::from_f64(n).unwrap()
        );
    }
    if value.is_big_int() {
        let bi = value.to_big_int(scope).unwrap();
        let s = bi.to_string(scope).unwrap();
        let s = s.to_rust_string_lossy(scope);
        let parsed: num_bigint::BigInt = s.parse().unwrap_or_default();
        return value::PrimitiveValue::Bigint(parsed);
    }
    if value.is_string() {
        let s = value.to_string(scope).unwrap();
        let s = s.to_rust_string_lossy(scope);
        return value::PrimitiveValue::String(s);
    }
    
    value::PrimitiveValue::Undefined
}


fn v8_value_to_value_impl(
    scope: &PinScope<'_, '_>,
    value: Local<'_, V8Value>,
    visited: &mut HashSet<usize>, // 用对象身份（指针）判重
) -> value::Value {
    if value.is_object() {
        let obj = value.to_object(scope).unwrap();
        let identity = obj.get_identity_hash().get() as usize;

        // 循环引用：用 null 兜底（或你想要的哨兵值）
        if visited.contains(&identity) {
            return value::Value::Primitive(value::PrimitiveValue::Null);
        }
        visited.insert(identity);

        // 数组
        if value.is_array() {
            let arr: Local<'_, Array> = value.try_into().unwrap();
            let len = arr.length() as usize;
            let mut vec = Vec::with_capacity(len);
            for i in 0..len {
                let elem = arr.get_index(scope, i as u32).unwrap();
                vec.push(v8_value_to_value_impl(scope, elem, visited));
            }
            visited.remove(&identity);
            return value::Value::Array(vec);
        }

        // 普通对象
        let mut map = value::Map::<value::Value>::new();
        let own_keys = obj
            .get_own_property_names(
                scope,
                GetPropertyNamesArgs {
                    mode: KeyCollectionMode::OwnOnly,
                    ..Default::default()
                },
            )
            .unwrap();
        let key_len = own_keys.length() as usize;
        for i in 0..key_len {
            let key = own_keys.get_index(scope, i as u32).unwrap();
            if let Some(key_str) = key.to_string(scope) {
                let key_rust = key_str.to_rust_string_lossy(scope);
                let val = obj.get(scope, key).unwrap();
                map.insert(
                    key_rust,
                    v8_value_to_value_impl(scope, val, visited),
                );
            }
        }
        visited.remove(&identity);
        value::Value::Object(map)
    } else {
        value::Value::Primitive(v8_value_to_primitive_value(scope, value))   
    }
}
