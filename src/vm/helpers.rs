use crate::heap::{Heap, HeapRef as HeapRefHandle};

use super::{Value, VmError};

pub(super) fn type_name(value: &Value, _heap: &Heap) -> &'static str {
    match value {
        Value::Number(_) => "number",
        Value::Str(_) => "string",
        Value::Boolean(_) => "boolean",
        Value::Null => "null",
        Value::List(_) => "list",
        Value::Object(_) => "object",
        Value::Function(_) | Value::BuiltIn(_) | Value::Closure { .. } => "function",
        Value::Task(_) => "task",
    }
}

/// Require `args[idx]` to be a string (§58 wrong type → ERROR).
pub(super) fn expect_string_arg<'a>(args: &'a [Value], idx: usize, fname: &str) -> Result<&'a str, VmError> {
    if idx >= args.len() {
        return Err(VmError::RuntimeError(format!(
            "{} expects at least {} argument(s)",
            fname,
            idx + 1
        )));
    }
    match &args[idx] {
        Value::Str(s) => Ok(s),
        other => Err(VmError::RuntimeError(format!(
            "{} argument {} must be string, got {:?}",
            fname,
            idx + 1,
            other
        ))),
    }
}

/// Require `args[idx]` to be a list.
pub(super) fn expect_list_arg(args: &[Value], idx: usize, fname: &str) -> Result<HeapRefHandle, VmError> {
    if idx >= args.len() {
        return Err(VmError::RuntimeError(format!(
            "{} expects at least {} argument(s)",
            fname,
            idx + 1
        )));
    }
    match &args[idx] {
        Value::List(r) => Ok(r.clone()),
        other => Err(VmError::RuntimeError(format!(
            "{} argument {} must be list, got {:?}",
            fname,
            idx + 1,
            other
        ))),
    }
}

/// Require `args[idx]` to be a task handle (§105, §109, §110).
pub(super) fn expect_task_arg(args: &[Value], idx: usize, fname: &str) -> Result<usize, VmError> {
    if idx >= args.len() {
        return Err(VmError::RuntimeError(format!(
            "{} expects at least {} argument(s)",
            fname,
            idx + 1
        )));
    }
    match &args[idx] {
        Value::Task(id) => Ok(*id),
        other => Err(VmError::RuntimeError(format!(
            "{} argument {} must be a task, got {:?}",
            fname,
            idx + 1,
            other
        ))),
    }
}

/// Parse a non-negative integer millisecond duration (§110, §111).
pub(super) fn duration_ms(value: &Value, fname: &str) -> Result<u128, VmError> {
    match value {
        Value::Number(n) if n.is_finite() && *n >= 0.0 && n.fract() == 0.0 => Ok(*n as u128),
        _ => Err(VmError::RuntimeError(format!(
            "{} time must be a non-negative integer number of milliseconds",
            fname
        ))),
    }
}

/// Chain an error and its sources into one message (ureq's Display prints
/// only the top-level variant).
pub(super) fn error_chain(e: &dyn std::error::Error) -> String {
    let mut s = e.to_string();
    let mut src = e.source();
    while let Some(cause) = src {
        s.push_str(": ");
        s.push_str(&cause.to_string());
        src = cause.source();
    }
    s
}

/// Require `args[idx]` to be an object.
pub(super) fn expect_object_arg(args: &[Value], idx: usize, fname: &str) -> Result<HeapRefHandle, VmError> {
    if idx >= args.len() {
        return Err(VmError::RuntimeError(format!(
            "{} expects at least {} argument(s)",
            fname,
            idx + 1
        )));
    }
    match &args[idx] {
        Value::Object(r) => Ok(r.clone()),
        other => Err(VmError::RuntimeError(format!(
            "{} argument {} must be object, got {:?}",
            fname,
            idx + 1,
            other
        ))),
    }
}

pub(super) fn raw_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Boolean(x), Value::Boolean(y)) => x == y,
        (Value::Null, Value::Null) => true,
        (Value::List(x), Value::List(y)) => x == y,
        (Value::Object(x), Value::Object(y)) => x == y,
        (Value::Function(x), Value::Function(y)) => x == y,
        (Value::BuiltIn(x), Value::BuiltIn(y)) => x == y,
        (Value::Closure { function_index: fi1, env: e1, .. }, Value::Closure { function_index: fi2, env: e2, .. }) => {
            fi1 == fi2 && e1 == e2
        }
        (Value::Task(x), Value::Task(y)) => x == y,
        (Value::Number(x), Value::Str(y)) => {
            if let Ok(n) = y.parse::<f64>() {
                *x == n
            } else {
                false
            }
        }
        (Value::Str(x), Value::Number(y)) => {
            if let Ok(n) = x.parse::<f64>() {
                n == *y
            } else {
                false
            }
        }
        _ => false,
    }
}
