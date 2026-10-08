use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum VmError {
    InvalidHeader(String),
    UnsupportedBytecodeVersion(u32),
    InvalidBytecode(String),
    InvalidOpcode(String),
    InvalidOperand(String),
    InvalidConstantIndex(usize),
    InvalidJumpTarget(usize),
    StackUnderflow(String),
    InvalidRuntimeOperation(String),
    InvalidExecutionState(String),
    /// §149 startup/runtime configuration error (ISSUES §71f).
    InvalidRuntimeConfiguration(String),
    /// §133 heap memory limit — a VM Error, not catchable (ISSUES §71a).
    MemoryLimit(String),
    RuntimeError(String),
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmError::InvalidHeader(msg) => write!(f, "Invalid bytecode header: {}", msg),
            VmError::UnsupportedBytecodeVersion(v) => {
                write!(f, "Unsupported bytecode version: {}", v)
            }
            VmError::InvalidBytecode(msg) => write!(f, "Invalid bytecode: {}", msg),
            VmError::InvalidOpcode(msg) => write!(f, "Invalid opcode: {}", msg),
            VmError::InvalidOperand(msg) => write!(f, "Invalid operand: {}", msg),
            VmError::InvalidConstantIndex(idx) => {
                write!(f, "Invalid constant index: {}", idx)
            }
            VmError::InvalidJumpTarget(target) => {
                write!(f, "Invalid jump target: {}", target)
            }
            VmError::StackUnderflow(msg) => write!(f, "Stack underflow: {}", msg),
            VmError::InvalidRuntimeOperation(msg) => {
                write!(f, "Invalid runtime operation: {}", msg)
            }
            VmError::InvalidExecutionState(msg) => {
                write!(f, "Invalid execution state: {}", msg)
            }
            VmError::InvalidRuntimeConfiguration(msg) => {
                write!(f, "Invalid runtime configuration: {}", msg)
            }
            VmError::MemoryLimit(msg) => write!(f, "Memory limit exceeded: {}", msg),
            VmError::RuntimeError(msg) => write!(f, "Runtime error: {}", msg),
        }
    }
}

impl Error for VmError {}

/// Message prefixes that identify an operand/operator type mismatch.
pub(super) const TYPE_ERROR_PREFIXES: &[&str] = &[
    "Cannot access property",
    "Cannot add",
    "Cannot apply",
    "Cannot call",
    "Cannot compare",
    "Cannot divide",
    "Cannot index",
    "Cannot modulo",
    "Cannot multiply",
    "Cannot negate",
    "Cannot power",
    "Cannot set index",
    "Cannot set property",
    "Cannot stringify",
    "Cannot subtract",
];

/// ISSUES §59a: category reported in `e.type` of a caught error object.
/// Categories follow the examples in Runtime.md §154 ("jenis kegagalan dapat
/// dibedakan secara konsisten"); a message with no unambiguous category falls
/// back to "RuntimeError".
pub(super) fn classify_runtime_error(msg: &str) -> &'static str {
    if msg.starts_with("Invalid JSON:") {
        return "SyntaxError";
    }
    if msg.contains("out of bounds") || msg.starts_with("Index error") {
        return "IndexError";
    }
    if msg.starts_with("Key error")
        || (msg.starts_with("Key '") && msg.contains("not found"))
        || (msg.starts_with("Property '") && msg.contains("not found"))
    {
        return "KeyError";
    }
    if msg.contains("Division by zero") || msg.contains("Modulo by zero") {
        return "DivisionError";
    }
    if msg.contains("Undefined variable") {
        return "ReferenceError";
    }
    if msg.contains("cancelled") {
        return "CancellationError";
    }
    if msg.contains("expects at least")
        || msg.contains("expects exactly")
        || (msg.starts_with("Expected ") && msg.contains("arguments"))
    {
        return "ArgumentError";
    }
    if msg.starts_with("Type error")
        || msg.starts_with("bad type")
        || msg.starts_with("Expected ")
        || msg.contains("Invalid conversion")
        || msg.contains("Conditional jump requires")
        || msg.contains("expects a ")
        || msg.contains(" must be ")
        || TYPE_ERROR_PREFIXES.iter().any(|prefix| msg.starts_with(prefix))
    {
        return "TypeError";
    }
    if msg.starts_with("Execution limit exceeded") {
        return "LimitError";
    }
    "RuntimeError"
}
