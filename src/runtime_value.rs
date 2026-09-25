use std::fmt;

use crate::heap::{Heap, HeapError, HeapObject, HeapRef};

#[derive(Debug, Clone)]
pub enum RuntimeValue {
    Number(f64),
    Boolean(bool),
    Null,
    HeapRef(HeapRef),
}

impl RuntimeValue {
    pub fn is_number(&self) -> bool {
        matches!(self, RuntimeValue::Number(_))
    }

    pub fn is_boolean(&self) -> bool {
        matches!(self, RuntimeValue::Boolean(_))
    }

    pub fn is_null(&self) -> bool {
        matches!(self, RuntimeValue::Null)
    }

    pub fn is_heap_ref(&self) -> bool {
        matches!(self, RuntimeValue::HeapRef(_))
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            RuntimeValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            RuntimeValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_heap_ref(&self) -> Option<&HeapRef> {
        match self {
            RuntimeValue::HeapRef(r) => Some(r),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            RuntimeValue::Number(_) => "number",
            RuntimeValue::Boolean(_) => "boolean",
            RuntimeValue::Null => "null",
            RuntimeValue::HeapRef(_) => "heap_ref",
        }
    }

    pub fn runtime_type_name(&self, heap: &Heap) -> &'static str {
        match self {
            RuntimeValue::Number(_) => "number",
            RuntimeValue::Boolean(_) => "boolean",
            RuntimeValue::Null => "null",
            RuntimeValue::HeapRef(r) => match heap.get(r) {
                Ok(obj) => match obj {
                    HeapObject::Str(_) => "string",
                    HeapObject::Number(_) => "number",
                    HeapObject::Boolean(_) => "boolean",
                    HeapObject::Null => "null",
                    HeapObject::List { .. } => "list",
                    HeapObject::Object { .. } => "object",
                    HeapObject::Function { .. } => "function",
                    HeapObject::Environment(_) => "environment",
                    HeapObject::Closure { .. } => "function",
                    HeapObject::Task(_) => "task",
                },
                Err(_) => "invalid",
            },
        }
    }
}

impl PartialEq for RuntimeValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (RuntimeValue::Number(a), RuntimeValue::Number(b)) => a == b,
            (RuntimeValue::Boolean(a), RuntimeValue::Boolean(b)) => a == b,
            (RuntimeValue::Null, RuntimeValue::Null) => true,
            (RuntimeValue::HeapRef(a), RuntimeValue::HeapRef(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for RuntimeValue {}

impl fmt::Display for RuntimeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeValue::Number(n) => {
                if *n == (*n as i64) as f64 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{}", n)
                }
            }
            RuntimeValue::Boolean(b) => write!(f, "{}", b),
            RuntimeValue::Null => write!(f, "null"),
            RuntimeValue::HeapRef(r) => write!(f, "{}", r),
        }
    }
}

pub fn raw_eq(a: &RuntimeValue, b: &RuntimeValue) -> bool {
    match (a, b) {
        (RuntimeValue::Number(x), RuntimeValue::Number(y)) => x == y,
        (RuntimeValue::Boolean(x), RuntimeValue::Boolean(y)) => x == y,
        (RuntimeValue::Null, RuntimeValue::Null) => true,
        (RuntimeValue::Number(_), RuntimeValue::HeapRef(_)) => {
            false // TODO: when string interning is available
        }
        (RuntimeValue::HeapRef(_), RuntimeValue::Number(_)) => {
            false // TODO: when string interning is available
        }
        (RuntimeValue::HeapRef(a_ref), RuntimeValue::HeapRef(b_ref)) => a_ref == b_ref,
        _ => false,
    }
}

pub fn runtime_add(
    a: &RuntimeValue,
    b: &RuntimeValue,
    heap: &mut Heap,
) -> Result<RuntimeValue, RuntimeError> {
    match (a, b) {
        (RuntimeValue::Number(x), RuntimeValue::Number(y)) => {
            let result = x + y;
            if !result.is_finite() {
                return Err(RuntimeError::ArithmeticError(
                    "Arithmetic result is not finite".to_string(),
                ));
            }
            Ok(RuntimeValue::Number(result))
        }
        (RuntimeValue::HeapRef(r1), RuntimeValue::HeapRef(r2)) => {
            let obj1 = heap.get(r1).map_err(RuntimeError::HeapError)?;
            match obj1 {
                HeapObject::Str(s1) => {
                    let s1_clone = s1.clone();
                    let obj2 = heap.get(r2).map_err(RuntimeError::HeapError)?;
                    match obj2 {
                        HeapObject::Str(s2) => {
                            let result = format!("{}{}", s1_clone, s2);
                            let new_ref = heap.allocate(HeapObject::Str(result));
                            Ok(RuntimeValue::HeapRef(new_ref))
                        }
                        _ => Err(RuntimeError::TypeError(format!(
                            "Cannot add string and {:?}",
                            b.type_name()
                        ))),
                    }
                }
                _ => Err(RuntimeError::TypeError(format!(
                    "Cannot add {:?} and {:?}",
                    a.type_name(),
                    b.type_name()
                ))),
            }
        }
        (RuntimeValue::Null, RuntimeValue::Number(y)) => Ok(RuntimeValue::Number(*y)),
        (RuntimeValue::Number(x), RuntimeValue::Null) => Ok(RuntimeValue::Number(*x)),
        (RuntimeValue::Null, RuntimeValue::Null) => Ok(RuntimeValue::Number(0.0)),
        (RuntimeValue::Null, RuntimeValue::HeapRef(r)) => {
            let obj = heap.get(r).map_err(RuntimeError::HeapError)?;
            match obj {
                HeapObject::Str(s) => {
                    let new_ref = heap.allocate(HeapObject::Str(s.clone()));
                    Ok(RuntimeValue::HeapRef(new_ref))
                }
                _ => Err(RuntimeError::TypeError(format!(
                    "Cannot add null and {:?}",
                    b.type_name()
                ))),
            }
        }
        (RuntimeValue::HeapRef(r), RuntimeValue::Null) => {
            let obj = heap.get(r).map_err(RuntimeError::HeapError)?;
            match obj {
                HeapObject::Str(s) => {
                    let new_ref = heap.allocate(HeapObject::Str(s.clone()));
                    Ok(RuntimeValue::HeapRef(new_ref))
                }
                _ => Err(RuntimeError::TypeError(format!(
                    "Cannot add {:?} and null",
                    a.type_name()
                ))),
            }
        }
        _ => Err(RuntimeError::TypeError(format!(
            "Cannot add {:?} and {:?}",
            a.type_name(),
            b.type_name()
        ))),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeError {
    TypeError(String),
    IndexError(String),
    KeyError(String),
    DivisionByZero(String),
    ArithmeticError(String),
    HeapError(HeapError),
    InvalidOperation(String),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::TypeError(msg) => write!(f, "Type error: {}", msg),
            RuntimeError::IndexError(msg) => write!(f, "Index error: {}", msg),
            RuntimeError::KeyError(msg) => write!(f, "Key error: {}", msg),
            RuntimeError::DivisionByZero(msg) => write!(f, "Division by zero: {}", msg),
            RuntimeError::ArithmeticError(msg) => write!(f, "Arithmetic error: {}", msg),
            RuntimeError::HeapError(e) => write!(f, "{}", e),
            RuntimeError::InvalidOperation(msg) => write!(f, "Invalid operation: {}", msg),
        }
    }
}

impl std::error::Error for RuntimeError {}

impl From<HeapError> for RuntimeError {
    fn from(e: HeapError) -> Self {
        RuntimeError::HeapError(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_value_types() {
        let n = RuntimeValue::Number(42.0);
        assert!(n.is_number());
        assert!(!n.is_boolean());
        assert!(!n.is_null());
        assert!(!n.is_heap_ref());
        assert_eq!(n.type_name(), "number");
        assert_eq!(n.as_number(), Some(42.0));

        let b = RuntimeValue::Boolean(true);
        assert!(b.is_boolean());
        assert_eq!(b.type_name(), "boolean");
        assert_eq!(b.as_boolean(), Some(true));

        let null = RuntimeValue::Null;
        assert!(null.is_null());
        assert_eq!(null.type_name(), "null");
    }

    #[test]
    fn test_runtime_value_display() {
        assert_eq!(format!("{}", RuntimeValue::Number(42.0)), "42");
        assert_eq!(format!("{}", RuntimeValue::Number(3.14)), "3.14");
        assert_eq!(format!("{}", RuntimeValue::Boolean(true)), "true");
        assert_eq!(format!("{}", RuntimeValue::Boolean(false)), "false");
        assert_eq!(format!("{}", RuntimeValue::Null), "null");
    }

    #[test]
    fn test_runtime_value_equality() {
        assert_eq!(RuntimeValue::Number(10.0), RuntimeValue::Number(10.0));
        assert_ne!(RuntimeValue::Number(10.0), RuntimeValue::Number(20.0));
        assert_eq!(RuntimeValue::Boolean(true), RuntimeValue::Boolean(true));
        assert_ne!(RuntimeValue::Boolean(true), RuntimeValue::Boolean(false));
        assert_eq!(RuntimeValue::Null, RuntimeValue::Null);
        assert_ne!(RuntimeValue::Number(0.0), RuntimeValue::Null);
        assert_ne!(RuntimeValue::Number(10.0), RuntimeValue::Boolean(true));
    }

    #[test]
    fn test_raw_eq() {
        assert!(raw_eq(
            &RuntimeValue::Number(10.0),
            &RuntimeValue::Number(10.0)
        ));
        assert!(!raw_eq(
            &RuntimeValue::Number(10.0),
            &RuntimeValue::Number(20.0)
        ));
        assert!(raw_eq(
            &RuntimeValue::Boolean(true),
            &RuntimeValue::Boolean(true)
        ));
        assert!(raw_eq(&RuntimeValue::Null, &RuntimeValue::Null));
    }

    #[test]
    fn test_runtime_add_numbers() {
        let mut heap = Heap::new();
        let a = RuntimeValue::Number(10.0);
        let b = RuntimeValue::Number(20.0);
        let result = runtime_add(&a, &b, &mut heap).unwrap();
        assert_eq!(result, RuntimeValue::Number(30.0));
    }

    #[test]
    fn test_runtime_add_strings() {
        let mut heap = Heap::new();
        let r1 = heap.allocate(HeapObject::Str("hello".to_string()));
        let r2 = heap.allocate(HeapObject::Str(" world".to_string()));
        let a = RuntimeValue::HeapRef(r1);
        let b = RuntimeValue::HeapRef(r2);
        let result = runtime_add(&a, &b, &mut heap).unwrap();
        match result {
            RuntimeValue::HeapRef(r) => {
                let obj = heap.get(&r).unwrap();
                match obj {
                    HeapObject::Str(s) => assert_eq!(s, "hello world"),
                    _ => panic!("Expected string"),
                }
            }
            _ => panic!("Expected heap ref"),
        }
    }

    #[test]
    fn test_runtime_add_null_number() {
        let mut heap = Heap::new();
        let result = runtime_add(&RuntimeValue::Null, &RuntimeValue::Number(5.0), &mut heap).unwrap();
        assert_eq!(result, RuntimeValue::Number(5.0));
    }

    #[test]
    fn test_runtime_add_null_null() {
        let mut heap = Heap::new();
        let result = runtime_add(&RuntimeValue::Null, &RuntimeValue::Null, &mut heap).unwrap();
        assert_eq!(result, RuntimeValue::Number(0.0));
    }

    #[test]
    fn test_runtime_type_name_with_heap() {
        let mut heap = Heap::new();
        let r = heap.allocate(HeapObject::Str("test".to_string()));
        let v = RuntimeValue::HeapRef(r);
        assert_eq!(v.runtime_type_name(&heap), "string");

        let r = heap.allocate(HeapObject::list(vec![]));
        let v = RuntimeValue::HeapRef(r);
        assert_eq!(v.runtime_type_name(&heap), "list");

        let r = heap.allocate(HeapObject::object(vec![]));
        let v = RuntimeValue::HeapRef(r);
        assert_eq!(v.runtime_type_name(&heap), "object");

        let r = heap.allocate(HeapObject::Function {
            index: 0,
            name: "test".to_string(),
            params: vec![],
            entry_point: 0,
        });
        let v = RuntimeValue::HeapRef(r);
        assert_eq!(v.runtime_type_name(&heap), "function");
    }

    #[test]
    fn test_runtime_error_display() {
        let e = RuntimeError::TypeError("bad type".to_string());
        assert_eq!(format!("{}", e), "Type error: bad type");

        let e = RuntimeError::IndexError("out of bounds".to_string());
        assert_eq!(format!("{}", e), "Index error: out of bounds");

        let e = RuntimeError::KeyError("missing".to_string());
        assert_eq!(format!("{}", e), "Key error: missing");

        let e = RuntimeError::DivisionByZero("by zero".to_string());
        assert_eq!(format!("{}", e), "Division by zero: by zero");
    }
}
