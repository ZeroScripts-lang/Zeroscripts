//! Native operation registry (Bytecode.md §145, §146).
//!
//! Single source of truth for the names ZeroVM exposes through the native-call
//! boundary. The compiler uses it to recognise statically resolvable calls
//! (emitted as `CALL_NATIVE <operation>` / `LIST_ADD` / `LIST_REMOVE` /
//! `OBJECT_REMOVE`); the ZeroVM uses it for namespace property resolution
//! (`string.length` on a BuiltIn root) and for dispatch in
//! `ZeroVM::call_native`. Adding a standard library function means adding a
//! name here plus one arm in `call_native` — never a new opcode (§145).

/// Root bindings installed into the global scope before execution.
pub const BUILTIN_ROOTS: &[&str] = &[
    "print",
    "input",
    "type",
    "number",
    "string",
    "boolean",
    "list",
    "object",
    "color",
    "file",
    "system",
    "json",
    "task",
    "http",
];

/// Roots that are callable directly, not only as namespaces.
pub const ROOT_FUNCTIONS: &[&str] = &["print", "input", "type", "number", "string", "boolean"];

/// Methods reachable on each namespace root: `list.add` → ("list", ["add", ..]).
pub const NATIVE_METHODS: &[(&str, &[&str])] = &[
    ("print", &["err"]),
    (
        "string",
        &[
            "length",
            "contains",
            "trim",
            "split",
            "replace",
            "startsWith",
            "endsWith",
            "lower",
            "upper",
            "indexOf",
        ],
    ),
    ("list", &["add", "remove", "length"]),
    ("object", &["remove"]),
    ("color", &["red", "green", "yellow", "blue", "bold"]),
    ("file", &["read", "write", "exists", "remove", "mkdir"]),
    ("system", &["args", "env", "os", "arch"]),
    ("json", &["parse", "stringify"]),
    (
        "task",
        &["state", "all", "race", "cancel", "wait", "timeout", "sleep"],
    ),
    ("http", &["get", "post", "put", "patch", "delete"]),
];

/// Is `name` a root binding installed by `ZeroVM::install_builtins`?
pub fn is_builtin_root(name: &str) -> bool {
    BUILTIN_ROOTS.contains(&name)
}

/// Methods exposed on `root`; empty when `root` is not a namespace root.
pub fn native_methods(root: &str) -> &'static [&'static str] {
    NATIVE_METHODS
        .iter()
        .find(|(r, _)| *r == root)
        .map(|(_, methods)| *methods)
        .unwrap_or(&[])
}

/// Is `op` a native operation this runtime provides (`print`, `string.length`,
/// `http.get`, …)? §146: not every native operation exists in every runtime
/// environment, and an operation that is not available produces a runtime
/// error (§145) — the compiler only emits `CALL_NATIVE` for names registered
/// here so hand-checked error behaviour stays on the dynamic path.
pub fn is_native_operation(op: &str) -> bool {
    match op.split_once('.') {
        Some((root, method)) => is_builtin_root(root) && native_methods(root).contains(&method),
        None => ROOT_FUNCTIONS.contains(&op),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roots_match_namespace_methods() {
        // Every namespace root (except the callable roots) has methods.
        for root in ["list", "object", "color", "file", "system", "json", "task", "http"] {
            assert!(is_builtin_root(root));
            assert!(!native_methods(root).is_empty(), "{} needs methods", root);
        }
    }

    #[test]
    fn test_is_native_operation_accepts_registered_names() {
        for op in [
            "print",
            "print.err",
            "input",
            "type",
            "number",
            "string",
            "boolean",
            "string.length",
            "list.add",
            "list.remove",
            "list.length",
            "object.remove",
            "color.red",
            "file.exists",
            "system.os",
            "json.parse",
            "task.all",
            "http.get",
        ] {
            assert!(is_native_operation(op), "{} should be native", op);
        }
    }

    #[test]
    fn test_is_native_operation_rejects_unknown_names() {
        for op in [
            "list",
            "object",
            "color",
            "file",
            "system",
            "json",
            "task",
            "http",
            "string.bogus",
            "list.bogus",
            "nope",
            "nope.method",
            "",
        ] {
            assert!(!is_native_operation(op), "{} must not be native", op);
        }
    }

    #[test]
    fn test_unknown_root_has_no_methods() {
        assert!(native_methods("nope").is_empty());
    }
}
