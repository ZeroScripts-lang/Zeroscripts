use super::{Value, VmError, ZeroVM};

impl ZeroVM {
    pub(super) fn add_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => {
                let result = x + y;
                if !result.is_finite() {
                    return Err(VmError::RuntimeError(
                        "Arithmetic result is not finite".to_string(),
                    ));
                }
                Ok(Value::Number(result))
            }
            (Value::Str(x), Value::Str(y)) => Ok(Value::Str(format!("{}{}", x, y))),
            (Value::Null, Value::Number(y)) => Ok(Value::Number(*y)),
            (Value::Number(x), Value::Null) => Ok(Value::Number(*x)),
            (Value::Null, Value::Null) => Ok(Value::Number(0.0)),
            _ => Err(VmError::RuntimeError(format!(
                "Cannot add {:?} and {:?}",
                a, b
            ))),
        }
    }

    pub(super) fn sub_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        let (x, y) = Self::to_numbers(a, b, "subtract")?;
        let result = x - y;
        if !result.is_finite() {
            return Err(VmError::RuntimeError(
                "Arithmetic result is not finite".to_string(),
            ));
        }
        Ok(Value::Number(result))
    }

    pub(super) fn mul_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        let (x, y) = Self::to_numbers(a, b, "multiply")?;
        let result = x * y;
        if !result.is_finite() {
            return Err(VmError::RuntimeError(
                "Arithmetic result is not finite".to_string(),
            ));
        }
        Ok(Value::Number(result))
    }

    pub(super) fn div_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        let (x, y) = Self::to_numbers(a, b, "divide")?;
        if y == 0.0 {
            return Err(VmError::RuntimeError("Division by zero".to_string()));
        }
        let result = x / y;
        if !result.is_finite() {
            return Err(VmError::RuntimeError(
                "Arithmetic result is not finite".to_string(),
            ));
        }
        Ok(Value::Number(result))
    }

    pub(super) fn mod_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        let (x, y) = Self::to_numbers(a, b, "modulo")?;
        if y == 0.0 {
            return Err(VmError::RuntimeError("Modulo by zero".to_string()));
        }
        let result = x % y;
        if !result.is_finite() {
            return Err(VmError::RuntimeError(
                "Arithmetic result is not finite".to_string(),
            ));
        }
        Ok(Value::Number(result))
    }

    pub(super) fn pow_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        let (x, y) = Self::to_numbers(a, b, "power")?;
        let result = x.powf(y);
        if !result.is_finite() {
            return Err(VmError::RuntimeError(
                "Arithmetic result is not finite".to_string(),
            ));
        }
        Ok(Value::Number(result))
    }

    pub(super) fn to_numbers(a: &Value, b: &Value, op: &str) -> Result<(f64, f64), VmError> {
        let x = match a {
            Value::Number(n) => *n,
            Value::Null => 0.0,
            _ => {
                return Err(VmError::RuntimeError(format!(
                    "Cannot {} {:?} and {:?}",
                    op, a, b
                )))
            }
        };
        let y = match b {
            Value::Number(n) => *n,
            Value::Null => 0.0,
            _ => {
                return Err(VmError::RuntimeError(format!(
                    "Cannot {} {:?} and {:?}",
                    op, a, b
                )))
            }
        };
        Ok((x, y))
    }

    pub(super) fn gt_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        let (x, y) = Self::require_numbers(a, b, "compare")?;
        Ok(Value::Boolean(x > y))
    }

    pub(super) fn lt_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        let (x, y) = Self::require_numbers(a, b, "compare")?;
        Ok(Value::Boolean(x < y))
    }

    pub(super) fn gte_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        let (x, y) = Self::require_numbers(a, b, "compare")?;
        Ok(Value::Boolean(x >= y))
    }

    pub(super) fn lte_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        let (x, y) = Self::require_numbers(a, b, "compare")?;
        Ok(Value::Boolean(x <= y))
    }

    pub(super) fn require_numbers(a: &Value, b: &Value, op: &str) -> Result<(f64, f64), VmError> {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => Ok((*x, *y)),
            _ => Err(VmError::RuntimeError(format!(
                "Cannot {} {:?} and {:?}",
                op, a, b
            ))),
        }
    }

    pub(super) fn and_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        match (a, b) {
            (Value::Boolean(x), Value::Boolean(y)) => Ok(Value::Boolean(*x && *y)),
            _ => Err(VmError::RuntimeError(format!(
                "Cannot apply 'and' to {:?} and {:?}",
                a, b
            ))),
        }
    }

    pub(super) fn or_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        match (a, b) {
            (Value::Boolean(x), Value::Boolean(y)) => Ok(Value::Boolean(*x || *y)),
            _ => Err(VmError::RuntimeError(format!(
                "Cannot apply 'or' to {:?} and {:?}",
                a, b
            ))),
        }
    }
}
