use std::fmt;

use crate::heap::{Heap, HeapObject, HeapRef as HeapRefHandle};
use crate::runtime_value::{RuntimeValue, RuntimeError};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Str(String),
    Boolean(bool),
    Null,
    List(HeapRefHandle),
    Object(HeapRefHandle),
    Function(usize),
    BuiltIn(String),
    Closure {
        function_index: usize,
        env: HeapRefHandle,
        param_defaults: Vec<Option<usize>>,
    },
    /// Asynchronous task handle (§101). Index into `ZeroVM.tasks`;
    /// state/result live in the task table, not on the heap.
    Task(usize),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Number(n) => {
                if *n == (*n as i64) as f64 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{}", n)
                }
            }
            Value::Str(s) => write!(f, "{}", s),
            Value::Boolean(b) => write!(f, "{}", b),
            Value::Null => write!(f, "null"),
            // List/Object require heap access to render contents; use format_value(&self, &heap).
            Value::List(r) => write!(f, "<list {}:{}>", r.index(), r.generation()),
            Value::Object(r) => write!(f, "<object {}:{}>", r.index(), r.generation()),
            Value::Function(idx) => write!(f, "<function {}>", idx),
            Value::BuiltIn(name) => write!(f, "<built-in {}>", name),
            Value::Closure { function_index, .. } => write!(f, "<closure {}>", function_index),
            Value::Task(id) => write!(f, "<task {}>", id),
        }
    }
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Number(_) => "number",
            Value::Str(_) => "string",
            Value::Boolean(_) => "boolean",
            Value::Null => "null",
            Value::List(_) => "list",
            Value::Object(_) => "object",
            Value::Function(_) => "function",
            Value::BuiltIn(_) => "function",
            Value::Closure { .. } => "function",
            // §101: task is a distinct runtime value (not a promise type exposed
            // as language type per §49, but observable via task.state).
            Value::Task(_) => "task",
        }
    }

    /// `Some(message)` when this value is a caught error object (ISSUES §59a),
    /// identified by the heap's internal error flag — a plain object carrying
    /// `type`/`message`/`location` is an ordinary object, not an error.
    pub fn error_message(&self, heap: &Heap) -> Option<String> {
        let Value::Object(reference) = self else {
            return None;
        };
        let Ok(object) = heap.get(reference) else {
            return None;
        };
        if !object.is_error_object() {
            return None;
        }
        let HeapObject::Object { props, .. } = object else {
            return None;
        };
        let message_ref = &props
            .iter()
            .find(|(key, _)| key.as_str() == "message")?
            .1;
        match heap.get(message_ref) {
            Ok(HeapObject::Str(text)) => Some(text.clone()),
            _ => None,
        }
    }

    /// Format a value for output, resolving heap-backed List/Object contents.
    pub fn format_value(&self, heap: &Heap) -> String {
        match self {
            Value::List(r) => match heap.get(r) {
                Ok(HeapObject::List { items, .. }) => {
                    let parts: Vec<String> = items
                        .iter()
                        .map(|ir| {
                            match Value::from_heap_ref(ir, heap) {
                                Ok(v) => v.format_value(heap),
                                Err(_) => "<invalid>".to_string(),
                            }
                        })
                        .collect();
                    format!("[{}]", parts.join(", "))
                }
                _ => "<list>".to_string(),
            },
            Value::Object(r) => match heap.get(r) {
                Ok(HeapObject::Object { props, .. }) => {
                    let parts: Vec<String> = props
                        .iter()
                        .map(|(k, vr)| {
                            let vstr = match Value::from_heap_ref(vr, heap) {
                                Ok(v) => v.format_value(heap),
                                Err(_) => "<invalid>".to_string(),
                            };
                            format!("{}: {}", k, vstr)
                        })
                        .collect();
                    format!("{{{}}}", parts.join(", "))
                }
                _ => "<object>".to_string(),
            },
            other => format!("{}", other),
        }
    }

    pub fn to_runtime_value(&self, heap: &mut Heap) -> RuntimeValue {
        match self {
            Value::Number(n) => {
                let r = heap.allocate(HeapObject::Number(*n));
                RuntimeValue::HeapRef(r)
            }
            Value::Boolean(b) => {
                let r = heap.allocate(HeapObject::Boolean(*b));
                RuntimeValue::HeapRef(r)
            }
            Value::Null => {
                let r = heap.allocate(HeapObject::Null);
                RuntimeValue::HeapRef(r)
            }
            Value::Str(s) => {
                let r = heap.allocate(HeapObject::Str(s.clone()));
                RuntimeValue::HeapRef(r)
            }
            Value::List(r) => RuntimeValue::HeapRef(r.clone()),
            Value::Object(r) => RuntimeValue::HeapRef(r.clone()),
            Value::Function(idx) => {
                let r = heap.allocate(HeapObject::Function {
                    index: *idx,
                    name: String::new(),
                    params: vec![],
                    entry_point: 0,
                });
                RuntimeValue::HeapRef(r)
            }
            Value::BuiltIn(name) => {
                let r = heap.allocate(HeapObject::Function {
                    index: 0,
                    name: name.clone(),
                    params: vec![],
                    entry_point: 0,
                });
                RuntimeValue::HeapRef(r)
            }
            // Job 021 (§130/§31): a stored/converted closure keeps its
            // original environment — no Environment clone.
            Value::Closure { function_index, env, param_defaults } => {
                let r = heap.allocate(HeapObject::Closure {
                    function_index: *function_index,
                    env: env.clone(),
                    param_defaults: param_defaults.clone(),
                });
                RuntimeValue::HeapRef(r)
            }
            Value::Task(id) => RuntimeValue::HeapRef(heap.allocate(HeapObject::Task(*id))),
        }
    }

    pub(super) fn heap_object_for_element(&self, heap: &mut Heap) -> HeapRefHandle {
        match self {
            Value::Number(n) => heap.allocate(HeapObject::Number(*n)),
            Value::Boolean(b) => heap.allocate(HeapObject::Boolean(*b)),
            Value::Null => heap.allocate(HeapObject::Null),
            Value::Str(s) => heap.allocate(HeapObject::Str(s.clone())),
            Value::List(r) => r.clone(),
            Value::Object(r) => r.clone(),
            Value::Function(idx) => heap.allocate(HeapObject::Function {
                index: *idx,
                name: String::new(),
                params: vec![],
                entry_point: 0,
            }),
            Value::BuiltIn(name) => heap.allocate(HeapObject::Function {
                index: 0,
                name: name.clone(),
                params: vec![],
                entry_point: 0,
            }),
            // Job 021 (§130/§31/§35): storing a closure into a list/
            // object/environment binding must preserve the original
            // environment identity — sibling closures and direct calls
            // keep observing each other's captured-variable mutations.
            Value::Closure { function_index, env, param_defaults } => {
                heap.allocate(HeapObject::Closure {
                    function_index: *function_index,
                    env: env.clone(),
                    param_defaults: param_defaults.clone(),
                })
            }
            Value::Task(id) => heap.allocate(HeapObject::Task(*id)),
        }
    }

    pub fn from_runtime_value(rv: &RuntimeValue, heap: &Heap) -> Result<Self, RuntimeError> {
        match rv {
            RuntimeValue::Number(n) => Ok(Value::Number(*n)),
            RuntimeValue::Boolean(b) => Ok(Value::Boolean(*b)),
            RuntimeValue::Null => Ok(Value::Null),
            RuntimeValue::HeapRef(r) => Self::from_heap_ref(r, heap),
        }
    }

    pub(super) fn from_heap_ref(r: &HeapRefHandle, heap: &Heap) -> Result<Self, RuntimeError> {
        let obj = heap.get(r).map_err(RuntimeError::HeapError)?;
        match obj {
            HeapObject::Str(s) => Ok(Value::Str(s.clone())),
            HeapObject::Number(n) => Ok(Value::Number(*n)),
            HeapObject::Boolean(b) => Ok(Value::Boolean(*b)),
            HeapObject::Null => Ok(Value::Null),
            HeapObject::List { .. } => Ok(Value::List(r.clone())),
            HeapObject::Object { .. } => Ok(Value::Object(r.clone())),
            HeapObject::Function { index, .. } => Ok(Value::Function(*index)),
            HeapObject::Environment(_) => {
                // Represent environment as an object-like view for debug/display purposes.
                Ok(Value::Object(r.clone()))
            }
            HeapObject::Closure { function_index, env, param_defaults } => Ok(Value::Closure {
                function_index: *function_index,
                env: env.clone(),
                param_defaults: param_defaults.clone(),
            }),
            HeapObject::Task(id) => Ok(Value::Task(*id)),
        }
    }
}
