use crate::ast::*;
use crate::error::CompilerError;
use crate::native;
use std::collections::HashSet;
use std::rc::Rc;

/// Magic marker of a ZeroScript bytecode image (§125 conceptual header).
pub const BYTECODE_MAGIC: &str = "ZERO";

/// Bytecode format version emitted by this compiler (§125). Distinct from the
/// ZeroScript language version and from any package/module version.
pub const BYTECODE_VERSION: u32 = 2;

/// Bytecode versions this ZeroVM accepts (§125: compatibility is stated
/// explicitly; an old bytecode is never assumed compatible). Version 2 adds
/// the debug/position side tables (ISSUES §75f, Job 036); version 1 images
/// are rejected by the validator before any instruction runs.
pub const SUPPORTED_BYTECODE_VERSIONS: &[u32] = &[2];

/// Bytecode header (§124, §125): `ZERO` + `BYTECODE_VERSION`. Checked by the
/// validator before any instruction runs.
#[derive(Debug, Clone, PartialEq)]
pub struct BytecodeHeader {
    pub magic: String,
    pub version: u32,
}

impl BytecodeHeader {
    /// Header the current compiler writes.
    pub fn current() -> Self {
        Self {
            magic: BYTECODE_MAGIC.to_string(),
            version: BYTECODE_VERSION,
        }
    }

    /// Header format is well-formed and the version is supported (§125).
    pub fn is_supported(&self) -> bool {
        self.magic == BYTECODE_MAGIC && SUPPORTED_BYTECODE_VERSIONS.contains(&self.version)
    }
}

/// One debug side-table entry: the source position (1-based, from the lexer)
/// of the opcode at `pc` in one instruction stream. Recorded only for
/// opcodes that can raise a runtime error (ISSUES §75f, Runtime.md §154 —
/// "jika informasi tersebut tersedia"). Missing entry ⇒ `e.location` is
/// null; the compiler never fabricates a position.
#[derive(Debug, Clone, PartialEq)]
pub struct DebugEntry {
    pub pc: u32,
    pub line: u32,
    pub column: u32,
}

/// Sentinel on top of every continuation marker (ISSUES §59d). The marker is
/// four value-stack slots — `[scope-pops, target pc, handler-pops, marker]` —
/// pushed on every entry into a `finally` body and consumed by `FinallyEnd`.
/// The NUL prefix keeps it distinguishable from program string constants.
pub const FINALLY_MARKER: &str = "\u{0}zerovm:finally-marker";

#[derive(Debug, Clone, PartialEq)]
pub enum Opcode {
    // Literals
    LoadConst(usize),

    // Function value (§141 `LOAD_FUNCTION`)
    LoadFunction(usize),

    // Variables
    DeclareLocal(Rc<str>, bool), // name, is_const
    LoadLocal(Rc<str>),
    StoreLocal(Rc<str>),
    DeclareGlobal(Rc<str>, bool), // name, is_const
    LoadGlobal(Rc<str>),
    StoreGlobal(Rc<str>),

    // Scopes
    PushScope,
    PopScope,

    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,

    // Unary
    Neg,
    Pos,
    Not,

    // Comparison
    Eq,
    Neq,
    RawEq,
    Gt,
    Lt,
    Gte,
    Lte,

    // Boolean
    And,
    Or,

    // Stack
    Pop,
    Dup,
    Swap,

    // Control flow
    Jump(usize),
    JumpIfTrue(usize),
    JumpIfFalse(usize),

    // Exception handling (§43)
    TryPush {
        catch: Option<usize>,
        finally: Option<usize>,
    },
    TryPop,
    FinallyEnd,

    // Functions
    Call(usize),
    /// `CALL_NATIVE <operation>` (§145, §146): native operation call.
    /// `usize` is the argument count; no callee is pushed on the value stack.
    CallNative(Rc<str>, usize),
    Return,
    MakeClosure(usize, Vec<(Rc<str>, bool)>), // captured: (name, is_const)
    LoadCaptured(Rc<str>),
    StoreCaptured(Rc<str>),

    // Async (§49, §101): wait for a task to reach a terminal state
    Await,

    // Collections
    MakeList(usize),
    MakeObject(Vec<Rc<str>>),
    /// `LIST_ADD` (§144): append a value to a mutable list. Operand = argc.
    ListAdd(usize),
    /// `LIST_REMOVE` (§144): remove a list element by index. Operand = argc.
    ListRemove(usize),
    /// `OBJECT_REMOVE` (§144): remove an object property by key. Operand = argc.
    ObjectRemove(usize),
    GetIndex,
    SetIndex,
    GetProperty(Rc<str>),
    SetProperty(Rc<str>),

    // Output
    Print,

    // Halt
    Halt,

    // Module boundary (§71-§72): end of one module's instruction range
    EndModule,

    /// Tag of an opcode this VM does not recognize. Never emitted by the
    /// compiler; it exists so hand-built or foreign bytecode fails validation
    /// with a VM Error instead of being executed (§124).
    Unknown(u64),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Number(f64),
    Str(String),
    Boolean(bool),
    Null,
    Function(usize),
}

#[derive(Debug, Clone)]
pub struct BytecodeProgram {
    /// Header (§124, §125): magic + bytecode version, checked before run.
    pub header: BytecodeHeader,
    /// Program entry point (§124). Index of the first instruction to execute.
    pub entry_point: usize,
    pub constants: Vec<Constant>,
    pub instructions: Vec<Opcode>,
    pub functions: Vec<FunctionInfo>,
    pub function_bodies: Vec<Vec<Opcode>>,
    /// Job 022 (§30): one body per default parameter expression, evaluated
    /// at call time. `FunctionInfo.param_defaults` indexes this table.
    pub default_bodies: Vec<Vec<Opcode>>,
    /// Job 036 (ISSUES §75f): source positions of error-capable opcodes in
    /// `instructions` (pc-indexed, sorted). Parallel to no other field —
    /// function bodies carry their own tables in `FunctionInfo`.
    pub debug: Vec<DebugEntry>,
}

/// One module's compiled unit before merge into a combined program.
/// `instructions` end with `Opcode::EndModule` and do not include function bodies.
#[derive(Debug, Clone)]
pub struct ModuleUnit {
    pub path: String,
    pub instructions: Vec<Opcode>,
    pub constants: Vec<Constant>,
    pub functions: Vec<FunctionInfo>,
    pub function_bodies: Vec<Vec<Opcode>>,
    /// Job 022 (§30): default-parameter expression bodies (§30).
    pub default_bodies: Vec<Vec<Opcode>>,
    /// Job 036 (ISSUES §75f): debug entries for this module's top-level
    /// `instructions` (unit-local pc).
    pub debug: Vec<DebugEntry>,
    pub exports: Vec<String>,
    pub imports: Vec<ModuleImport>,
    /// Re-export edges: (export_name, raw_path). §67.
    pub re_exports: Vec<(String, String)>,
}

/// A single named import binding: source export name, local name (alias), raw path.
/// Without alias, local_name == export_name (§69).
#[derive(Debug, Clone)]
pub struct ModuleImport {
    pub local_name: String,
    pub export_name: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: String,
    pub params: Vec<String>,
    /// Job 022 (§30): index into `BytecodeProgram.default_bodies` for each
    /// parameter that has a default expression; `None` when it has none.
    pub param_defaults: Vec<Option<usize>>,
    pub entry_point: usize,
    pub local_count: usize,
    pub captured_vars: Vec<String>,
    /// `async function` (§49): calling it creates a Task (§101).
    pub is_async: bool,
    /// Job 036 (ISSUES §75f): source path of the module this function was
    /// compiled from (input path for the entry, canonical path for deps —
    /// decision 2026-10-13). Empty for embedder `compile()` with no path.
    pub source_file: String,
    /// Job 036: debug entries for this function's body stream (body-local pc).
    pub debug: Vec<DebugEntry>,
    /// Job 036: per-default-parameter debug tables, aligned with
    /// `param_defaults` (`Some` index → table of that default body).
    pub default_debug: Vec<Option<Vec<DebugEntry>>>,
}

struct LoopContext {
    break_jumps: Vec<usize>,
    continue_jumps: Vec<usize>,
    scope_depth: usize, // compiler scopes.len() before the loop body scope was pushed
    /// Live try handlers (ISSUES §59d) when the loop began — crossing count =
    /// current live handlers minus this snapshot.
    handler_depth: usize,
    /// Patchable constants holding the final break/continue destinations of
    /// chain markers whose last `finally` belongs to this loop.
    break_record_targets: Vec<usize>,
    continue_record_targets: Vec<usize>,
}

/// One `try` region with a handler (catch and/or finally) during compilation.
struct TryContext {
    /// compiler scopes.len() at TryPush — the try-outer (finally-entry) depth.
    scope_depth: usize,
    has_finally: bool,
    /// True while compiling the finally body itself: at runtime that try's
    /// handler is already popped, so it never counts as a live crossing handler.
    in_finally: bool,
    /// Patchable constants and jumps registered by break/continue chains whose
    /// target is this try's finally entry (patched when finally_pc is known).
    finally_target_consts: Vec<usize>,
    finally_entry_jumps: Vec<usize>,
}

/// A call the compiler can resolve statically to the native-call boundary
/// (§145, §146) or to a core collection instruction (§144).
#[derive(Debug, Clone, PartialEq)]
enum StaticCallTarget {
    /// `CALL_NATIVE <operation>`
    Native(String),
    /// `LIST_ADD` (§144)
    ListAdd,
    /// `LIST_REMOVE` (§144)
    ListRemove,
    /// `OBJECT_REMOVE` (§144)
    ObjectRemove,
}

#[derive(Debug, Clone)]
struct ScopeVar {
    name: String,
    is_const: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum VarRes {
    Local,
    Captured,
    /// Module-scope binding in module mode that is NOT exported (§70d):
    /// loads read it through the module environment (§130 "binding yang
    /// diperlukan"), but stores stay §4-local — only exported bindings keep
    /// the shared store required by §66 live bindings.
    ModuleScope,
    Global,
    ConstLocal,
    ConstCaptured,
    ConstModuleScope,
    ConstGlobal,
}

struct Compiler {
    constants: Vec<Constant>,
    instructions: Vec<Opcode>,
    functions: Vec<FunctionInfo>,
    function_bodies: Vec<Vec<Opcode>>,
    /// Job 022 (§30): default-parameter expression bodies, one per defaulted
    /// parameter, indexed by `FunctionInfo.param_defaults`.
    default_bodies: Vec<Vec<Opcode>>,
    /// Job 036 (ISSUES §75f): debug entries for the stream currently being
    /// compiled (swapped with bodies, like `instructions`).
    debug_entries: Vec<DebugEntry>,
    /// Debug tables parallel to `function_bodies` / `default_bodies`.
    function_bodies_debug: Vec<Vec<DebugEntry>>,
    default_bodies_debug: Vec<Vec<DebugEntry>>,
    /// Source path stamped on every FunctionInfo this compiler emits.
    source_file: String,
    scopes: Vec<Vec<ScopeVar>>,
    loop_contexts: Vec<LoopContext>,
    try_contexts: Vec<TryContext>,
    /// Constant indices that are patched in place (marker targets). Excluded
    /// from deduplication so neither they nor ordinary constants can alias
    /// each other and be corrupted by a later patch.
    patchable_constants: HashSet<usize>,
    captured_vars: Vec<(Rc<str>, bool)>,
    scope_depth: usize,
    /// Index of the current function's base scope. 0 = global/top-level program.
    function_base: usize,
    /// Module-mode export names recorded during compile (§47).
    exports: Vec<String>,
    /// Module-mode import bindings (§46); no codegen, pre-bound at runtime.
    imports: Vec<ModuleImport>,
    /// Re-export edges recorded during compile (§67).
    re_exports: Vec<(String, String)>,
    /// True while compiling the body of an `async function` (§49).
    /// `await` outside an async function is a compile-time ERROR (§101).
    in_async: bool,
    /// §70d: true only in module mode (`new_module`). Gates the module-scope
    /// store classification for non-exported bindings.
    module_mode: bool,
    /// §70d/§66: export names seeded before statements compile, so a function
    /// that textually precedes its `export let` still classifies that binding
    /// as exported (exports are static declarations like imports, §78).
    exported_names: HashSet<String>,
    /// §70d: true while compiling the body of an exported function (inherited
    /// by nested functions inside it). Private module bindings stay shared
    /// for export contexts (Job 012 `private_captured_by_export`; §74 "export
    /// let tetap mutable di dalam module asal") and follow §4 (store → local)
    /// only inside plain, non-exported functions.
    fn_is_exported: bool,
}

impl Compiler {
    fn new() -> Self {
        Self {
            constants: Vec::new(),
            instructions: Vec::new(),
            functions: Vec::new(),
            function_bodies: Vec::new(),
            default_bodies: Vec::new(),
            scopes: vec![Vec::new()],
            loop_contexts: Vec::new(),
            try_contexts: Vec::new(),
            patchable_constants: HashSet::new(),
            captured_vars: Vec::new(),
            scope_depth: 0,
            function_base: 0,
            exports: Vec::new(),
            imports: Vec::new(),
            re_exports: Vec::new(),
            in_async: false,
            module_mode: false,
            fn_is_exported: false,
            exported_names: HashSet::new(),
            debug_entries: Vec::new(),
            function_bodies_debug: Vec::new(),
            default_bodies_debug: Vec::new(),
            source_file: String::new(),
        }
    }

    /// Module mode: scopes[0] is a placeholder (not in capture chain);
    /// scopes[1] is the module scope; function_base = 1 so top-level
    /// declarations use DeclareLocal (isolation, §65/§76) and nested
    /// functions capture module bindings via the existing mechanism.
    fn new_module() -> Self {
        Self {
            constants: Vec::new(),
            instructions: Vec::new(),
            functions: Vec::new(),
            function_bodies: Vec::new(),
            default_bodies: Vec::new(),
            scopes: vec![Vec::new(), Vec::new()],
            loop_contexts: Vec::new(),
            try_contexts: Vec::new(),
            patchable_constants: HashSet::new(),
            captured_vars: Vec::new(),
            scope_depth: 0,
            function_base: 1,
            exports: Vec::new(),
            imports: Vec::new(),
            re_exports: Vec::new(),
            in_async: false,
            module_mode: true,
            fn_is_exported: false,
            exported_names: HashSet::new(),
            debug_entries: Vec::new(),
            function_bodies_debug: Vec::new(),
            default_bodies_debug: Vec::new(),
            source_file: String::new(),
        }
    }

    /// §70d: seed export names up front (top-level declarations only). The
    /// per-statement `record_export` still validates and fills `exports`.
    fn seed_export_names(&mut self, program: &Program) {
        for stmt in &program.statements {
            match stmt {
                Stmt::VariableDecl {
                    name, exported: true, ..
                } => {
                    self.exported_names.insert(name.clone());
                }
                Stmt::FunctionDecl {
                    name, exported: true, ..
                } => {
                    self.exported_names.insert(name.clone());
                }
                Stmt::ExportList {
                    names,
                    from: None,
                    ..
                } => {
                    for n in names {
                        self.exported_names.insert(n.clone());
                    }
                }
                _ => {}
            }
        }
    }

    fn record_export(&mut self, name: &str) -> Result<(), CompilerError> {
        // Re-export names are recorded separately; check only local declarations.
        let declared = self.find_in_scopes(name, 0, self.scopes.len()).is_some()
            || self.scopes[0].iter().any(|v| v.name == name);
        if !declared {
            return Err(CompilerError::new(&format!(
                "Cannot export undeclared '{}'",
                name
            )));
        }
        // Export conflict detection (§68): one binding per export name.
        if self.exports.iter().any(|e| e == name) {
            return Err(CompilerError::new(&format!(
                "Duplicate export '{}'",
                name
            )));
        }
        self.exports.push(name.to_string());
        Ok(())
    }

    /// Record a re-export `export { name } from "./path"` (§67).
    /// Does not require a local declaration — the binding lives in the source module.
    fn record_re_export(&mut self, name: &str, path: &str) -> Result<(), CompilerError> {
        if self.exports.iter().any(|e| e == name) {
            return Err(CompilerError::new(&format!(
                "Duplicate export '{}'",
                name
            )));
        }
        self.exports.push(name.to_string());
        self.re_exports.push((name.to_string(), path.to_string()));
        Ok(())
    }

    fn add_constant(&mut self, constant: Constant) -> usize {
        // Deduplicate constants — but never alias a patchable marker target:
        // its value changes later and would corrupt this constant with it.
        for (i, existing) in self.constants.iter().enumerate() {
            if *existing == constant && !self.patchable_constants.contains(&i) {
                return i;
            }
        }
        let index = self.constants.len();
        self.constants.push(constant);
        index
    }

    fn emit(&mut self, opcode: Opcode) -> usize {
        let pos = self.instructions.len();
        self.instructions.push(opcode);
        pos
    }

    /// Job 036 (ISSUES §75f): emit an error-capable opcode together with its
    /// source position (the triggering token's span, parser-accurate).
    fn emit_located(&mut self, opcode: Opcode, span: &Span) -> usize {
        let pc = self.emit(opcode);
        self.debug_entries.push(DebugEntry {
            pc: pc as u32,
            line: span.line as u32,
            column: span.column as u32,
        });
        pc
    }

    fn patch_jump(&mut self, offset: usize) {
        let target = self.instructions.len();
        match &mut self.instructions[offset] {
            Opcode::Jump(addr)
            | Opcode::JumpIfTrue(addr)
            | Opcode::JumpIfFalse(addr) => {
                *addr = target;
            }
            _ => panic!("Invalid jump patch target"),
        }
    }

    fn current_scope(&self) -> &Vec<ScopeVar> {
        self.scopes.last().unwrap()
    }

    fn declare_variable(&mut self, name: &str, is_const: bool) -> Result<(), CompilerError> {
        if self
            .scopes
            .last()
            .unwrap()
            .iter()
            .any(|v| v.name == name)
        {
            return Err(CompilerError::new(&format!(
                "Duplicate declaration of '{}'",
                name
            )));
        }
        self.scopes.last_mut().unwrap().push(ScopeVar {
            name: name.to_string(),
            is_const,
        });
        Ok(())
    }

    fn find_in_scopes(&self, name: &str, start: usize, end: usize) -> Option<VarRes>
    where
    {
        // end is exclusive
        for scope in self.scopes[start..end].iter().rev() {
            if let Some(v) = scope.iter().find(|v| v.name == name) {
                return Some(if v.is_const {
                    VarRes::ConstLocal
                } else {
                    VarRes::Local
                });
            }
        }
        None
    }

    /// Resolve a name for a LOAD operation.
    fn resolve_for_load(&self, name: &str) -> VarRes {
        if self.function_base == 0 {
            // Top-level program: block scopes (1..) are local; scopes[0] is global root.
            if self.scopes.len() > 1 {
                if let Some(res) = self.find_in_scopes(name, 1, self.scopes.len()) {
                    return res;
                }
            }
            // Global root
            if let Some(v) = self.scopes[0].iter().find(|v| v.name == name) {
                return if v.is_const {
                    VarRes::ConstGlobal
                } else {
                    VarRes::Global
                };
            }
            VarRes::Global
        } else {
            // Inside a function: current function scopes are local.
            if let Some(res) = self.find_in_scopes(name, self.function_base, self.scopes.len()) {
                return res;
            }
            // Enclosing function scopes (excluding global root at index 0) are captured.
            if self.function_base > 1 {
                for (offset, scope) in
                    self.scopes[1..self.function_base].iter().enumerate().rev()
                {
                    if let Some(v) = scope.iter().find(|v| v.name == name) {
                        // §70d: scope at index 1 is the module scope. A plain
                        // function must not share-store through it merely
                        // because lexical visibility makes it capturable
                        // (§4); exported bindings keep the §66 shared store.
                        if self.module_mode
                            && offset == 0
                            && !self.exported_names.contains(name)
                            && !self.fn_is_exported
                        {
                            return if v.is_const {
                                VarRes::ConstModuleScope
                            } else {
                                VarRes::ModuleScope
                            };
                        }
                        return if v.is_const {
                            VarRes::ConstCaptured
                        } else {
                            VarRes::Captured
                        };
                    }
                }
            }
            // Global root
            if let Some(v) = self.scopes[0].iter().find(|v| v.name == name) {
                return if v.is_const {
                    VarRes::ConstGlobal
                } else {
                    VarRes::Global
                };
            }
            VarRes::Global
        }
    }

    /// Resolve a name for an ASSIGNMENT operation.
    /// Per §4, assignment inside a function to a global (or unknown) name creates a local.
    fn resolve_for_store(&self, name: &str) -> (VarRes, bool) {
        let load_res = self.resolve_for_load(name);
        match load_res {
            VarRes::Local | VarRes::ConstLocal => (load_res, false),
            VarRes::Captured | VarRes::ConstCaptured => (load_res, false),
            // §70d/§4: store to a non-exported module binding inside a
            // function creates a local — never a shared write. Const is
            // rejected by `check_assign_const` before any store is emitted.
            VarRes::ModuleScope => (VarRes::Local, true),
            VarRes::ConstModuleScope => (VarRes::ConstModuleScope, false),
            VarRes::Global | VarRes::ConstGlobal => {
                if self.function_base == 0 {
                    (load_res, false) // top-level: store to global
                } else {
                    // §4: assignment in function is local even if global exists
                    (VarRes::Local, true) // true => create local (store only, no declare of const)
                }
            }
        }
    }

    fn check_assign_const(&self, name: &str) -> Result<(), CompilerError> {
        let (res, _) = self.resolve_for_store(name);
        // Const reassignment is an error when the resolved binding is const.
        // For function-local shadow creation of a const global, the new local is mutable.
        match res {
            VarRes::ConstLocal | VarRes::ConstCaptured | VarRes::ConstModuleScope => Err(
                CompilerError::new(&format!("Cannot assign to const '{}'", name)),
            ),
            VarRes::ConstGlobal if self.function_base == 0 => Err(CompilerError::new(&format!(
                "Cannot assign to const '{}'",
                name
            ))),
            _ => Ok(()),
        }
    }

    fn track_captured(&mut self, name: &str, res: VarRes) {
        let is_const = matches!(res, VarRes::ConstCaptured | VarRes::ConstModuleScope);
        if !self.captured_vars.iter().any(|(n, _)| name == &**n) {
            self.captured_vars.push((Rc::from(name), is_const));
        }
    }

    /// Register a §4 function-local created by bare assignment (no DeclareLocal opcode).
    /// Subsequent loads in this function must resolve as Local, not Global.
    fn register_implicit_local(&mut self, name: &str) {
        let scope = self.scopes.last_mut().unwrap();
        if !scope.iter().any(|v| v.name == name) {
            scope.push(ScopeVar {
                name: name.to_string(),
                is_const: false,
            });
        }
    }

    /// Job 022 (§30): a default expression must not reference a parameter
    /// declared after it. Checked on the AST: name resolution cannot tell a
    /// later parameter from a global (unknown names fall back to globals), so
    /// the explicit walk is what turns `function bad(a = b, b)` into a
    /// compile error. Nested functions create their own scope and are not
    /// walked (a name there does not bind this function's parameters).
    fn validate_default_param_refs(
        &self,
        default_expr: &Expr,
        params: &[FunctionParam],
        current: usize,
    ) -> Result<(), CompilerError> {
        let mut refs = Vec::new();
        collect_ident_refs(default_expr, &mut refs);
        for later in &params[current + 1..] {
            if refs.iter().any(|r| r == &later.name) {
                return Err(CompilerError::new(&format!(
                    "Parameter '{}' cannot be used in the default of parameter '{}' (declared later)",
                    later.name, params[current].name
                )));
            }
        }
        Ok(())
    }

    /// Job 022 (§30): compile one default-parameter expression into its own
    /// body in `self.default_bodies` and return that body's index. Compiled
    /// against the current function-local constant pool and capture set, so
    /// outer references become captured variables exactly like body code.
    fn compile_default_body(&mut self, default_expr: &Expr) -> Result<usize, CompilerError> {
        let saved_instructions = std::mem::take(&mut self.instructions);
        let saved_debug = std::mem::take(&mut self.debug_entries);
        self.compile_expression(default_expr)?;
        self.emit(Opcode::Return);
        let body = std::mem::take(&mut self.instructions);
        let body_debug = std::mem::take(&mut self.debug_entries);
        self.instructions = saved_instructions;
        self.debug_entries = saved_debug;
        let idx = self.default_bodies.len();
        self.default_bodies.push(body);
        self.default_bodies_debug.push(body_debug);
        Ok(idx)
    }

    fn push_scope(&mut self) {
        self.scopes.push(Vec::new());
        self.emit(Opcode::PushScope);
    }

    fn pop_scope(&mut self) {
        self.scopes.pop().unwrap();
        self.emit(Opcode::PopScope);
    }

    /// Is `name` bound by user code anywhere in the current scope chain?
    /// Undeclared names resolve as globals, and for a builtin root that means
    /// the native binding installed by the VM (§145).
    fn is_declared(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .any(|scope| scope.iter().any(|v| v.name == name))
    }

    /// Resolve a callee statically to the native-call boundary (§145, §146) or
    /// to a core collection instruction (§144).
    ///
    /// Returns `None` for everything the compiler cannot prove is a builtin:
    /// user bindings, shadowing parameters, unknown methods, and namespace
    /// roots used as plain values all keep the dynamic `CALL` path, so the
    /// observable behaviour and error messages never change.
    fn static_call_target(&self, callee: &Expr) -> Option<StaticCallTarget> {
        match callee {
            Expr::Identifier(root, _) => {
                if self.is_declared(root) || !native::is_native_operation(root) {
                    return None;
                }
                Some(StaticCallTarget::Native(root.clone()))
            }
            Expr::Member {
                object, property, ..
            } => {
                let root = match object.as_ref() {
                    Expr::Identifier(root, _) => root,
                    _ => return None,
                };
                if self.is_declared(root) || !native::is_builtin_root(root) {
                    return None;
                }
                let operation = format!("{}.{}", root, property);
                match operation.as_str() {
                    "list.add" => Some(StaticCallTarget::ListAdd),
                    "list.remove" => Some(StaticCallTarget::ListRemove),
                    "object.remove" => Some(StaticCallTarget::ObjectRemove),
                    _ if native::is_native_operation(&operation) => {
                        Some(StaticCallTarget::Native(operation))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn push_loop_context(&mut self) {
        let scope_depth = self.scopes.len();
        let handler_depth = self.live_handler_count();
        self.loop_contexts.push(LoopContext {
            break_jumps: Vec::new(),
            continue_jumps: Vec::new(),
            scope_depth,
            handler_depth,
            break_record_targets: Vec::new(),
            continue_record_targets: Vec::new(),
        });
    }

    fn pop_loop_context(&mut self) -> Option<LoopContext> {
        self.loop_contexts.pop()
    }

    fn add_break_jump(&mut self, jump: usize) {
        if let Some(ctx) = self.loop_contexts.last_mut() {
            ctx.break_jumps.push(jump);
        }
    }

    fn add_continue_jump(&mut self, jump: usize) {
        if let Some(ctx) = self.loop_contexts.last_mut() {
            ctx.continue_jumps.push(jump);
        }
    }

    /// Emit PopScope ops to unwind at runtime down to `target_depth`,
    /// without modifying the compiler's scope stack (enclosing statements
    /// still own those scopes for their normal-path PopScope emission).
    fn unwind_scopes_to(&mut self, target_depth: usize) {
        let count = self.scopes.len().saturating_sub(target_depth);
        for _ in 0..count {
            self.emit(Opcode::PopScope);
        }
    }

    /// Try handlers that are live at runtime right now (ISSUES §59d): a ctx
    /// compiling its finally body corresponds to a handler that every entry
    /// path pops before entering the finally, so it does not count.
    fn live_handler_count(&self) -> usize {
        self.try_contexts
            .iter()
            .filter(|t| !t.in_finally)
            .count()
    }

    /// Add a constant without deduplication — used for marker target values
    /// that are patched in place after the destination pc becomes known.
    fn add_patchable_constant(&mut self, constant: Constant) -> usize {
        let index = self.constants.len();
        self.constants.push(constant);
        self.patchable_constants.insert(index);
        index
    }

    /// Push one four-slot continuation marker: `[scope-pops, target, handler-
    /// pops, sentinel]` (sentinel on top), consumed by `FinallyEnd`.
    fn emit_finally_marker(&mut self, scope_pops: usize, target: usize, handler_pops: usize) -> usize {
        let pops_const = self.add_constant(Constant::Number(scope_pops as f64));
        let target_const = self.add_patchable_constant(Constant::Number(target as f64));
        let handlers_const = self.add_constant(Constant::Number(handler_pops as f64));
        let marker_const = self.add_constant(Constant::Str(FINALLY_MARKER.to_string()));
        self.emit(Opcode::LoadConst(pops_const));
        self.emit(Opcode::LoadConst(target_const));
        self.emit(Opcode::LoadConst(handlers_const));
        self.emit(Opcode::LoadConst(marker_const));
        target_const
    }

    /// Compile `break` (`is_break`) or `continue` (§21, §22; ISSUES §59d).
    ///
    /// The jump must run the `finally` of every try region it leaves, so each
    /// crossed try with a finally gets a continuation marker chained innermost
    /// first: handlers are popped, scopes unwind to the first finally's entry
    /// depth, and the chain enters that finally. Every `FinallyEnd` consumes
    /// one marker — popping the next crossed handlers, unwinding scopes, and
    /// jumping to the next finally entry — until the last marker lands on the
    /// loop destination. Tries without a finally only get their handler popped.
    fn compile_early_exit(&mut self, is_break: bool) {
        let (loop_depth, handler_depth) = {
            let ctx = self.loop_contexts.last().unwrap();
            (ctx.scope_depth, ctx.handler_depth)
        };

        // Inside a finally body there is a live marker driving its FinallyEnd;
        // this jump replaces that continuation, so discard the marker first.
        if self.try_contexts.last().map_or(false, |t| t.in_finally) {
            for _ in 0..4 {
                self.emit(Opcode::Pop);
            }
        }

        // Crossed handlers = live handlers pushed after the loop began.
        // Contexts compiling a finally body belong to already-popped handlers.
        let live: Vec<usize> = (0..self.try_contexts.len())
            .filter(|&i| !self.try_contexts[i].in_finally)
            .collect();
        let crossed = live.len().saturating_sub(handler_depth);

        if crossed == 0 {
            self.unwind_scopes_to(loop_depth);
            let jump = self.emit(Opcode::Jump(0));
            if is_break {
                self.add_break_jump(jump);
            } else {
                self.add_continue_jump(jump);
            }
            return;
        }

        // Crossed tries innermost first, and the ones that own a finally with
        // their 1-based position from the inner side (positions are strictly
        // increasing, matching handler-pop counts in the chain).
        let crossing: Vec<usize> = live[live.len() - crossed..].iter().rev().copied().collect();
        let with_finally: Vec<(usize, usize)> = crossing
            .iter()
            .enumerate()
            .filter(|&(_, i)| self.try_contexts[*i].has_finally)
            .map(|(p, &i)| (p + 1, i))
            .collect();

        if with_finally.is_empty() {
            // Nothing to run: pop every crossed handler, then leave the loop.
            self.unwind_scopes_to(loop_depth);
            for _ in 0..crossed {
                self.emit(Opcode::TryPop);
            }
            let jump = self.emit(Opcode::Jump(0));
            if is_break {
                self.add_break_jump(jump);
            } else {
                self.add_continue_jump(jump);
            }
            return;
        }

        // Enter the innermost crossed finally: pop handlers up to and including
        // it, unwind to its entry depth, then chain markers for all of them.
        let first_pos = with_finally[0].0;
        let first_ctx = with_finally[0].1;
        self.unwind_scopes_to(self.try_contexts[first_ctx].scope_depth);
        for _ in 0..first_pos {
            self.emit(Opcode::TryPop);
        }

        // Outermost marker first, so the innermost finally's sits on top.
        for w in (0..with_finally.len()).rev() {
            let (pos, ctx) = with_finally[w];
            let (scope_pops, handler_pops, next_ctx) = if w + 1 < with_finally.len() {
                let (next_pos, next_ctx) = with_finally[w + 1];
                let scope_pops = self.try_contexts[ctx].scope_depth
                    - self.try_contexts[next_ctx].scope_depth;
                (scope_pops, next_pos - pos, Some(next_ctx))
            } else {
                let scope_pops = self.try_contexts[ctx].scope_depth - loop_depth;
                (scope_pops, crossed - pos, None)
            };
            let target_const = self.emit_finally_marker(scope_pops, 0, handler_pops);
            match next_ctx {
                // Intermediate target: entry of the next crossed finally
                // (patched when that try's finally pc becomes known).
                Some(next) => self.try_contexts[next].finally_target_consts.push(target_const),
                // Last target: the loop destination (patched when the loop ends).
                None => {
                    let record_targets = if is_break {
                        &mut self.loop_contexts.last_mut().unwrap().break_record_targets
                    } else {
                        &mut self.loop_contexts.last_mut().unwrap().continue_record_targets
                    };
                    record_targets.push(target_const);
                }
            }
        }

        // First jump of the chain enters the innermost crossed finally.
        let jump = self.emit(Opcode::Jump(0));
        self.try_contexts[first_ctx].finally_entry_jumps.push(jump);
    }

    fn compile_program(&mut self, program: &Program) -> Result<(), CompilerError> {
        for stmt in &program.statements {
            self.compile_statement(stmt)?;
        }
        self.emit(Opcode::Halt);

        // Append function bodies to main instruction stream
        // and update their entry points
        for (i, body) in self.function_bodies.iter().enumerate() {
            self.functions[i].entry_point = self.instructions.len();
            self.instructions.extend(body.iter().cloned());
        }

        Ok(())
    }

    fn compile_else_branch(&mut self, else_branch: &ElseBranch) -> Result<(), CompilerError> {
        match else_branch {
            ElseBranch::Block(stmts) => {
                self.push_scope();
                for stmt in stmts {
                    self.compile_statement(stmt)?;
                }
                self.pop_scope();
                Ok(())
            }
            ElseBranch::If {
                condition,
                then_body,
                else_branch,
            } => {
                self.compile_expression(condition)?;
                let jump_if_false = self.emit(Opcode::JumpIfFalse(0));
                self.emit(Opcode::Pop);

                self.push_scope();
                for stmt in then_body {
                    self.compile_statement(stmt)?;
                }
                self.pop_scope();

                let jump_to_end = self.emit(Opcode::Jump(0));
                self.patch_jump(jump_if_false);
                self.emit(Opcode::Pop);

                if let Some(inner) = else_branch {
                    self.compile_else_branch(inner)?;
                }

                self.patch_jump(jump_to_end);
                Ok(())
            }
        }
    }

    fn compile_statement(&mut self, stmt: &Stmt) -> Result<(), CompilerError> {
        match stmt {
            Stmt::VariableDecl {
                name,
                is_const,
                initializer,
                exported,
                ..
            } => {
                self.declare_variable(name, *is_const)?;

                let is_global_root = self.function_base == 0 && self.scopes.len() == 1;

                if is_global_root {
                    self.emit(Opcode::DeclareGlobal(name.clone().into(), *is_const));
                    if let Some(init) = initializer {
                        self.compile_expression(init)?;
                        self.emit(Opcode::StoreGlobal(name.clone().into()));
                    } else {
                        let idx = self.add_constant(Constant::Null);
                        self.emit(Opcode::LoadConst(idx));
                        self.emit(Opcode::StoreGlobal(name.clone().into()));
                    }
                } else {
                    self.emit(Opcode::DeclareLocal(name.clone().into(), *is_const));
                    if let Some(init) = initializer {
                        self.compile_expression(init)?;
                        self.emit(Opcode::StoreLocal(name.clone().into()));
                    } else {
                        let idx = self.add_constant(Constant::Null);
                        self.emit(Opcode::LoadConst(idx));
                        self.emit(Opcode::StoreLocal(name.clone().into()));
                    }
                }
                if *exported {
                    self.record_export(name)?;
                }
                Ok(())
            }
            Stmt::FunctionDecl {
                name,
                params,
                body,
                exported,
                is_async,
                ..
            } => {
                let func_idx = self.functions.len();

                // FunctionInfo is pushed before default/body compilation so a
                // function expression nested in a default keeps the
                // functions/function_bodies index pairing (§143). Defaults
                // are compiled into default_bodies below (§30).
                self.functions.push(FunctionInfo {
                    name: name.clone(),
                    params: params.iter().map(|p| p.name.clone()).collect(),
                    param_defaults: Vec::new(),
                    entry_point: 0,
                    local_count: 0,
                    captured_vars: Vec::new(),
                    is_async: *is_async,
                    source_file: String::new(),
                    debug: Vec::new(),
                    default_debug: Vec::new(),
                });

                let is_global_root = self.function_base == 0 && self.scopes.len() == 1;
                self.declare_variable(name, false)?;

                // Save compilation state (but NOT functions - inner functions need to be added)
                let saved_instructions = std::mem::take(&mut self.instructions);
                let saved_debug_entries = std::mem::take(&mut self.debug_entries);
                let saved_constants = std::mem::take(&mut self.constants);
                let saved_captured = std::mem::take(&mut self.captured_vars);
                let saved_scope_depth = self.scope_depth;
                let saved_scopes = std::mem::take(&mut self.scopes);
                let saved_function_base = self.function_base;
                let saved_bodies_count = self.function_bodies.len();
                let saved_loop_contexts = std::mem::take(&mut self.loop_contexts);
                let saved_try_contexts = std::mem::take(&mut self.try_contexts);
                let saved_patchable = std::mem::take(&mut self.patchable_constants);
                let saved_in_async = self.in_async;
                self.in_async = *is_async;
                // §70d: export context is inherited by anything compiled
                // inside this body (defaults, nested functions) until restored.
                let saved_fn_exported = self.fn_is_exported;
                self.fn_is_exported = saved_fn_exported || *exported;

                // Fresh scope chain for the function: scopes[0] empty (not used for captures),
                // function base at index 0 of the new chain (function_base tracks position in
                // self.scopes; we rebuild a minimal chain: parent scopes must stay visible for
                // capture detection, so keep parent scopes and append the function scope).
                self.scopes = saved_scopes.clone();
                self.scopes.push(Vec::new());
                self.scope_depth = saved_scope_depth + 1;
                self.function_base = self.scopes.len() - 1;

                // Add function name to its own scope for self-reference
                self.declare_variable(name, false)?;

                // Keep parent scopes intact for capture detection
                self.instructions = Vec::new();
                self.constants = Vec::new();
                self.captured_vars = Vec::new();
                let saved_defaults_count = self.default_bodies.len();

                // Job 022 (§30): compile each default inside the function
                // scope, left to right. Earlier parameters are already
                // declared (visible as locals); the parameter being defined
                // and later ones are not, so a default cannot see them.
                let mut param_defaults = Vec::new();
                for (i, param) in params.iter().enumerate() {
                    if let Some(ref default_expr) = param.default_value {
                        self.validate_default_param_refs(default_expr, params, i)?;
                        let default_idx = self.compile_default_body(default_expr)?;
                        param_defaults.push(Some(default_idx));
                    } else {
                        param_defaults.push(None);
                    }
                    self.declare_variable(&param.name, false)?;
                }
                self.functions[func_idx].default_debug = param_defaults
                    .iter()
                    .map(|o| o.map(|i| self.default_bodies_debug[i].clone()))
                    .collect();
                self.functions[func_idx].param_defaults = param_defaults;

                // Compile body
                for stmt in body {
                    self.compile_statement(stmt)?;
                }

                // Implicit return null
                let null_idx = self.add_constant(Constant::Null);
                self.emit(Opcode::LoadConst(null_idx));
                self.emit(Opcode::Return);

                let local_count = self.current_scope().len();
                let captured = std::mem::take(&mut self.captured_vars);

                let func_instructions = std::mem::take(&mut self.instructions);
                let mut func_debug = std::mem::take(&mut self.debug_entries);
                self.functions[func_idx].debug = func_debug.clone();
                let func_constants = std::mem::take(&mut self.constants);

                // Collect any inner function bodies that were added
                let mut inner_bodies: Vec<Vec<Opcode>> = self.function_bodies.drain(saved_bodies_count..).collect();
                let inner_debug: Vec<Vec<DebugEntry>> =
                    self.function_bodies_debug.drain(saved_bodies_count..).collect();

                // Restore outer state
                self.scopes = saved_scopes;
                self.instructions = saved_instructions;
                self.debug_entries = saved_debug_entries;
                self.constants = saved_constants;
                self.captured_vars = saved_captured;
                self.scope_depth = saved_scope_depth;
                self.function_base = saved_function_base;
                self.loop_contexts = saved_loop_contexts;
                self.try_contexts = saved_try_contexts;
                self.patchable_constants = saved_patchable;
                self.in_async = saved_in_async;
                self.fn_is_exported = saved_fn_exported;

                // Merge function constants into main pool
                let constant_offset = self.constants.len();
                let mut remapped_instructions = func_instructions;
                for func_const in &func_constants {
                    self.constants.push(func_const.clone());
                }
                for instr in &mut remapped_instructions {
                    if let Opcode::LoadConst(idx) = instr {
                        *idx += constant_offset;
                    }
                }

                // Remap inner function bodies by the same offset
                for body in &mut inner_bodies {
                    for instr in body {
                        if let Opcode::LoadConst(idx) = instr {
                            *idx += constant_offset;
                        }
                    }
                }

                // Job 022 (§30): remap this function's default bodies — plus
                // any nested function's, which stay in the table — by the
                // same constant offset (mirrors inner_bodies above).
                for body in &mut self.default_bodies[saved_defaults_count..] {
                    for instr in body {
                        if let Opcode::LoadConst(idx) = instr {
                            *idx += constant_offset;
                        }
                    }
                }

                // entry_point is 0 because we compiled into a fresh buffer
                self.functions[func_idx].entry_point = 0;
                self.functions[func_idx].local_count = local_count;
                self.functions[func_idx].captured_vars = captured.iter().map(|(n, _)| n.to_string()).collect();

                // Store function reference
                // Function metadata stays in the constant pool (§126) but the
                // value itself is pushed by LOAD_FUNCTION (§141).
                self.add_constant(Constant::Function(func_idx));
                self.emit(Opcode::LoadFunction(func_idx));

                if !captured.is_empty() {
                    self.emit(Opcode::MakeClosure(func_idx, captured));
                }

                if is_global_root {
                    self.emit(Opcode::StoreGlobal(name.clone().into()));
                } else {
                    self.emit(Opcode::StoreLocal(name.clone().into()));
                }

                // Store this function's body first, then inner bodies
                self.function_bodies.push(remapped_instructions);
                self.function_bodies_debug.push(std::mem::take(&mut func_debug));
                for body in inner_bodies {
                    self.function_bodies.push(body);
                }
                for dbg in inner_debug {
                    self.function_bodies_debug.push(dbg);
                }

                if *exported {
                    self.record_export(name)?;
                }

                Ok(())
            }
            Stmt::Import { names, path, .. } => {
                // Static declaration (§78): declare locals only; values are
                // pre-bound into the module scope by the runtime before the
                // module body runs. No codegen. Alias only renames local (§69).
                for (export_name, local_name) in names {
                    self.declare_variable(local_name, true)?;
                    self.imports.push(ModuleImport {
                        local_name: local_name.clone(),
                        export_name: export_name.clone(),
                        path: path.clone(),
                    });
                }
                Ok(())
            }
            Stmt::ExportList { names, from, .. } => {
                match from {
                    Some(path) => {
                        // Re-export (§67): no local declaration required.
                        for name in names {
                            self.record_re_export(name, path)?;
                        }
                    }
                    None => {
                        for name in names {
                            self.record_export(name)?;
                        }
                    }
                }
                Ok(())
            }
            Stmt::Return { value, .. } => {
                if let Some(val) = value {
                    self.compile_expression(val)?;
                } else {
                    let idx = self.add_constant(Constant::Null);
                    self.emit(Opcode::LoadConst(idx));
                }
                self.emit(Opcode::Return);
                Ok(())
            }
            Stmt::If {
                condition,
                then_body,
                else_branch,
                ..
            } => {
                self.compile_expression(condition)?;

                let jump_if_false = self.emit(Opcode::JumpIfFalse(0));
                self.emit(Opcode::Pop);

                self.push_scope();
                for stmt in then_body {
                    self.compile_statement(stmt)?;
                }
                self.pop_scope();

                let jump_to_end = self.emit(Opcode::Jump(0));
                self.patch_jump(jump_if_false);
                self.emit(Opcode::Pop);

                if let Some(else_branch) = else_branch {
                    self.compile_else_branch(else_branch)?;
                }

                self.patch_jump(jump_to_end);

                Ok(())
            }
            Stmt::While { condition, body, .. } => {
                let loop_start = self.instructions.len();

                self.compile_expression(condition)?;

                let jump_if_false = self.emit(Opcode::JumpIfFalse(0));
                self.emit(Opcode::Pop);

                self.push_loop_context();

                self.push_scope();
                for stmt in body {
                    self.compile_statement(stmt)?;
                }
                self.pop_scope();

                // Jump back to start
                self.emit(Opcode::Jump(loop_start));

                self.patch_jump(jump_if_false);
                self.emit(Opcode::Pop);

                let loop_ctx = self.pop_loop_context().unwrap();

                // Patch break jumps
                for jump in loop_ctx.break_jumps {
                    self.patch_jump(jump);
                }
                // Chain markers (ISSUES §59d) whose last finally lands on break
                for c in loop_ctx.break_record_targets {
                    self.constants[c] = Constant::Number(self.instructions.len() as f64);
                }
                // ... and on continue
                for c in loop_ctx.continue_record_targets {
                    self.constants[c] = Constant::Number(loop_start as f64);
                }

                // Patch continue jumps
                for jump in loop_ctx.continue_jumps {
                    self.instructions[jump] = Opcode::Jump(loop_start);
                }

                Ok(())
            }
            Stmt::Loop { body, .. } => {
                let loop_start = self.instructions.len();

                self.push_loop_context();

                self.push_scope();
                for stmt in body {
                    self.compile_statement(stmt)?;
                }
                self.pop_scope();

                // Jump back to start
                self.emit(Opcode::Jump(loop_start));

                let loop_ctx = self.pop_loop_context().unwrap();

                // Patch break jumps
                for jump in loop_ctx.break_jumps {
                    self.patch_jump(jump);
                }
                // Chain markers (ISSUES §59d) whose last finally lands on break
                for c in loop_ctx.break_record_targets {
                    self.constants[c] = Constant::Number(self.instructions.len() as f64);
                }
                // ... and on continue
                for c in loop_ctx.continue_record_targets {
                    self.constants[c] = Constant::Number(loop_start as f64);
                }

                // Patch continue jumps
                for jump in loop_ctx.continue_jumps {
                    self.instructions[jump] = Opcode::Jump(loop_start);
                }

                Ok(())
            }
            Stmt::Break { .. } => {
                if self.loop_contexts.is_empty() {
                    return Err(CompilerError::new("break outside of loop"));
                }
                self.compile_early_exit(true);
                Ok(())
            }
            Stmt::Continue { .. } => {
                if self.loop_contexts.is_empty() {
                    return Err(CompilerError::new("continue outside of loop"));
                }
                self.compile_early_exit(false);
                Ok(())
            }
            Stmt::Switch {
                value,
                cases,
                default_case,
                ..
            } => {
                self.compile_expression(value)?;

                let mut end_jumps = Vec::new();

                // Compile each case. JumpIfFalse only peeks, so each path
                // must drop the comparison result itself: the match path
                // pops result + switch value before running its body, the
                // miss path pops the result and falls into the next case's
                // Dup (otherwise the stale `false` is what gets duplicated
                // and case 2+ never matches).
                for case in cases {
                    // Duplicate the switch value
                    self.emit(Opcode::Dup);

                    // Compile case value
                    self.compile_expression(&case.value)?;

                    // Compare
                    self.emit(Opcode::Eq);

                    // Jump if not equal
                    let jump_if_false = self.emit(Opcode::JumpIfFalse(0));

                    // Match: drop comparison result and switch value
                    self.emit(Opcode::Pop);
                    self.emit(Opcode::Pop);

                    // Compile case body in its own scope
                    self.push_scope();
                    for stmt in &case.body {
                        self.compile_statement(stmt)?;
                    }
                    self.pop_scope();

                    // Jump to end
                    let jump_to_end = self.emit(Opcode::Jump(0));
                    end_jumps.push(jump_to_end);

                    // Miss: drop comparison result, keep switch value
                    self.patch_jump(jump_if_false);
                    self.emit(Opcode::Pop);
                }

                // Default case (also the no-case path): drop switch value
                self.emit(Opcode::Pop);
                if let Some(default) = default_case {
                    self.push_scope();
                    for stmt in default {
                        self.compile_statement(stmt)?;
                    }
                    self.pop_scope();
                }

                // Patch all end jumps
                for jump in end_jumps {
                    self.patch_jump(jump);
                }

                Ok(())
            }
            Stmt::Try {
                body,
                catch_param,
                catch_body,
                finally_body,
                ..
            } => {
                let has_catch = catch_param.is_some() && catch_body.is_some();
                let has_finally = finally_body.is_some();

                // No catch and no finally: plain scope (outer handlers still apply)
                if !has_catch && !has_finally {
                    self.push_scope();
                    for stmt in body {
                        self.compile_statement(stmt)?;
                    }
                    self.pop_scope();
                    return Ok(());
                }

                let push_idx = self.emit(Opcode::TryPush {
                    catch: None,
                    finally: None,
                });

                self.try_contexts.push(TryContext {
                    scope_depth: self.scopes.len(),
                    has_finally,
                    in_finally: false,
                    finally_target_consts: Vec::new(),
                    finally_entry_jumps: Vec::new(),
                });

                self.push_scope();
                for stmt in body {
                    self.compile_statement(stmt)?;
                }
                self.pop_scope();
                self.emit(Opcode::TryPop);
                // Success path: skip catch, go to finally (or after). With a
                // finally, the continuation marker is pushed right here so
                // FinallyEnd knows where to resume after the block (§43, ISSUES §59d).
                let success_marker = if has_finally {
                    Some(self.emit_finally_marker(0, 0, 0))
                } else {
                    None
                };
                let mut catch_marker = None;
                let jump_skip_catch = self.emit(Opcode::Jump(0));

                let mut catch_pc = None;
                if has_catch {
                    let cp = self.instructions.len();
                    catch_pc = Some(cp);
                    self.push_scope();
                    // VM pushes error value on stack before entering catch
                    let param = catch_param.as_ref().unwrap();
                    self.declare_variable(param, false)?;
                    self.emit(Opcode::StoreLocal(param.clone().into()));
                    for stmt in catch_body.as_ref().unwrap() {
                        self.compile_statement(stmt)?;
                    }
                    self.pop_scope();
                    self.emit(Opcode::TryPop);
                    // Catch completed normally: continue into the finally with
                    // the same continuation as the success path (§43).
                    if has_finally {
                        catch_marker = Some(self.emit_finally_marker(0, 0, 0));
                    }
                }
                let jump_to_finally = if has_catch {
                    Some(self.emit(Opcode::Jump(0)))
                } else {
                    None
                };

                let finally_pc = if has_finally {
                    let pc = self.instructions.len();
                    // Patch chain entries registered by break/continue sites in
                    // the body/catch of this try (ISSUES §59d).
                    let ctx = self.try_contexts.last_mut().unwrap();
                    for c in ctx.finally_target_consts.drain(..) {
                        self.constants[c] = Constant::Number(pc as f64);
                    }
                    for j in ctx.finally_entry_jumps.drain(..) {
                        self.instructions[j] = Opcode::Jump(pc);
                    }
                    Some(pc)
                } else {
                    None
                };

                if has_finally {
                    // From here until FinallyEnd, break/continue sites see a
                    // live marker on the stack and an already-popped handler.
                    self.try_contexts.last_mut().unwrap().in_finally = true;
                    self.push_scope();
                    for stmt in finally_body.as_ref().unwrap() {
                        self.compile_statement(stmt)?;
                    }
                    self.pop_scope();
                    self.emit(Opcode::FinallyEnd);
                }

                let after = self.instructions.len();

                // Patch jumps into finally or after
                match finally_pc {
                    Some(fin) => {
                        self.instructions[jump_skip_catch] = Opcode::Jump(fin);
                        if let Some(j) = jump_to_finally {
                            self.instructions[j] = Opcode::Jump(fin);
                        }
                    }
                    None => {
                        self.instructions[jump_skip_catch] = Opcode::Jump(after);
                        if let Some(j) = jump_to_finally {
                            self.instructions[j] = Opcode::Jump(after);
                        }
                    }
                }

                // Continuation markers resume after the whole try statement.
                if let Some(c) = success_marker {
                    self.constants[c] = Constant::Number(after as f64);
                }
                if let Some(c) = catch_marker {
                    self.constants[c] = Constant::Number(after as f64);
                }

                self.instructions[push_idx] = Opcode::TryPush {
                    catch: catch_pc,
                    finally: finally_pc,
                };

                self.try_contexts.pop();

                Ok(())
            }
            Stmt::Expression(expr) => {
                self.compile_expression(expr)?;
                // Pop the result of expression statements
                self.emit(Opcode::Pop);
                Ok(())
            }
        }
    }

    fn compile_expression(&mut self, expr: &Expr) -> Result<(), CompilerError> {
        match expr {
            Expr::NumberLiteral(n, _) => {
                let idx = self.add_constant(Constant::Number(*n));
                self.emit(Opcode::LoadConst(idx));
                Ok(())
            }
            Expr::StringLiteral(s, _) => {
                let idx = self.add_constant(Constant::Str(s.clone()));
                self.emit(Opcode::LoadConst(idx));
                Ok(())
            }
            Expr::BooleanLiteral(b, _) => {
                let idx = self.add_constant(Constant::Boolean(*b));
                self.emit(Opcode::LoadConst(idx));
                Ok(())
            }
            Expr::NullLiteral(_) => {
                let idx = self.add_constant(Constant::Null);
                self.emit(Opcode::LoadConst(idx));
                Ok(())
            }
            Expr::Identifier(name, span) => {
                let res = self.resolve_for_load(name);
                match res {
                    VarRes::Captured | VarRes::ConstCaptured => {
                        self.track_captured(name, res);
                        self.emit_located(Opcode::LoadCaptured(name.clone().into()), span);
                    }
                    VarRes::ModuleScope | VarRes::ConstModuleScope => {
                        // §70d: read through the module environment; the
                        // binding stays in the closure's captured list so the
                        // module env outlives init (§65/§130).
                        self.track_captured(name, res);
                        self.emit_located(Opcode::LoadCaptured(name.clone().into()), span);
                    }
                    VarRes::Local | VarRes::ConstLocal => {
                        self.emit_located(Opcode::LoadLocal(name.clone().into()), span);
                    }
                    VarRes::Global | VarRes::ConstGlobal => {
                        self.emit_located(Opcode::LoadGlobal(name.clone().into()), span);
                    }
                }
                Ok(())
            }
            Expr::Unary { op, operand, span } => {
                self.compile_expression(operand)?;
                match op {
                    UnaryOp::Negate => {
                        self.emit_located(Opcode::Neg, span);
                    }
                    UnaryOp::Plus => {
                        self.emit_located(Opcode::Pos, span);
                    }
                    UnaryOp::Not => {
                        self.emit(Opcode::Not);
                    }
                };
                Ok(())
            }
            Expr::Binary { op, left, right, span } => {
                // Short-circuit evaluation for and/or
                match op {
                    BinaryOp::And => {
                        self.compile_expression(left)?;
                        let jump_if_false = self.emit(Opcode::JumpIfFalse(0));
                        self.emit(Opcode::Pop);
                        self.compile_expression(right)?;
                        self.patch_jump(jump_if_false);
                        return Ok(());
                    }
                    BinaryOp::Or => {
                        self.compile_expression(left)?;
                        let jump_if_true = self.emit(Opcode::JumpIfTrue(0));
                        self.emit(Opcode::Pop);
                        self.compile_expression(right)?;
                        self.patch_jump(jump_if_true);
                        return Ok(());
                    }
                    _ => {}
                }

                self.compile_expression(left)?;
                self.compile_expression(right)?;

                match op {
                    BinaryOp::Add => {
                        self.emit_located(Opcode::Add, span);
                    }
                    BinaryOp::Subtract => {
                        self.emit_located(Opcode::Sub, span);
                    }
                    BinaryOp::Multiply => {
                        self.emit_located(Opcode::Mul, span);
                    }
                    BinaryOp::Divide => {
                        self.emit_located(Opcode::Div, span);
                    }
                    BinaryOp::Modulo => {
                        self.emit_located(Opcode::Mod, span);
                    }
                    BinaryOp::Power => {
                        self.emit_located(Opcode::Pow, span);
                    }
                    BinaryOp::Greater => {
                        self.emit_located(Opcode::Gt, span);
                    }
                    BinaryOp::Less => {
                        self.emit_located(Opcode::Lt, span);
                    }
                    BinaryOp::GreaterEqual => {
                        self.emit_located(Opcode::Gte, span);
                    }
                    BinaryOp::LessEqual => {
                        self.emit_located(Opcode::Lte, span);
                    }
                    BinaryOp::Equal => {
                        self.emit(Opcode::Eq);
                    }
                    BinaryOp::NotEqual => {
                        self.emit(Opcode::Neq);
                    }
                    BinaryOp::RawEqual => {
                        self.emit(Opcode::RawEq);
                    }
                    BinaryOp::And | BinaryOp::Or => unreachable!(),
                };

                Ok(())
            }
            Expr::Assignment { target, value, .. } => {
                match target.as_ref() {
                    Expr::Identifier(name, _) => {
                        let (res, create_local) = self.resolve_for_store(name);
                        self.check_assign_const(name)?;
                        self.compile_expression(value)?;
                        self.emit(Opcode::Dup);
                        match res {
                            VarRes::Captured | VarRes::ConstCaptured | VarRes::ConstModuleScope => {
                                self.track_captured(name, res);
                                self.emit(Opcode::StoreCaptured(name.clone().into()));
                            }
                            VarRes::Local | VarRes::ConstLocal | VarRes::ModuleScope => {
                                self.emit(Opcode::StoreLocal(name.clone().into()));
                            }
                            VarRes::Global | VarRes::ConstGlobal => {
                                self.emit(Opcode::StoreGlobal(name.clone().into()));
                            }
                        }
                        if create_local {
                            self.register_implicit_local(name);
                        }
                    }
                    Expr::Member { object, property, span } => {
                        self.compile_expression(object)?;
                        self.compile_expression(value)?;
                        self.emit_located(Opcode::SetProperty(property.clone().into()), span);
                    }
                    Expr::Index { object, index, span } => {
                        self.compile_expression(object)?;
                        self.compile_expression(index)?;
                        self.compile_expression(value)?;
                        self.emit_located(Opcode::SetIndex, span);
                    }
                    _ => {
                        return Err(CompilerError::new("Invalid assignment target"));
                    }
                }

                Ok(())
            }
            Expr::CompoundAssignment {
                op,
                target,
                value,
                ..
            } => {
                match target.as_ref() {
                    Expr::Identifier(name, _) => {
                        let load_res = self.resolve_for_load(name);
                        let (store_res, create_local) = self.resolve_for_store(name);
                        self.check_assign_const(name)?;
                        // Load current value (from outer/global if this store will create a local)
                        match load_res {
                            VarRes::Captured | VarRes::ConstCaptured => {
                                self.track_captured(name, load_res);
                                self.emit(Opcode::LoadCaptured(name.clone().into()));
                            }
                            VarRes::ModuleScope | VarRes::ConstModuleScope => {
                                // §70d: read current value from module env
                                // before a §4-local store (compound/postfix).
                                self.track_captured(name, load_res);
                                self.emit(Opcode::LoadCaptured(name.clone().into()));
                            }
                            VarRes::Local | VarRes::ConstLocal => {
                                self.emit(Opcode::LoadLocal(name.clone().into()));
                            }
                            VarRes::Global | VarRes::ConstGlobal => {
                                self.emit(Opcode::LoadGlobal(name.clone().into()));
                            }
                        }
                        // Load new value
                        self.compile_expression(value)?;
                        // Perform operation
                        match op {
                            CompoundAssignOp::Add => self.emit(Opcode::Add),
                            CompoundAssignOp::Subtract => self.emit(Opcode::Sub),
                            CompoundAssignOp::Multiply => self.emit(Opcode::Mul),
                            CompoundAssignOp::Divide => self.emit(Opcode::Div),
                            CompoundAssignOp::Modulo => self.emit(Opcode::Mod),
                        };
                        // Store result (keep on stack as expression value)
                        self.emit(Opcode::Dup);
                        match store_res {
                            VarRes::Captured | VarRes::ConstCaptured | VarRes::ConstModuleScope => {
                                self.emit(Opcode::StoreCaptured(name.clone().into()));
                            }
                            VarRes::Local | VarRes::ConstLocal | VarRes::ModuleScope => {
                                self.emit(Opcode::StoreLocal(name.clone().into()));
                            }
                            VarRes::Global | VarRes::ConstGlobal => {
                                self.emit(Opcode::StoreGlobal(name.clone().into()));
                            }
                        }
                        if create_local {
                            self.register_implicit_local(name);
                        }
                    }
                    Expr::Member { object, property, .. } => {
                        // [object, value]
                        self.compile_expression(object)?;
                        self.emit(Opcode::Dup);
                        self.emit(Opcode::GetProperty(property.clone().into()));
                        self.compile_expression(value)?;
                        match op {
                            CompoundAssignOp::Add => self.emit(Opcode::Add),
                            CompoundAssignOp::Subtract => self.emit(Opcode::Sub),
                            CompoundAssignOp::Multiply => self.emit(Opcode::Mul),
                            CompoundAssignOp::Divide => self.emit(Opcode::Div),
                            CompoundAssignOp::Modulo => self.emit(Opcode::Mod),
                        };
                        self.emit(Opcode::SetProperty(property.clone().into()));
                    }
                    Expr::Index { object, index, .. } => {
                        // [object, index, value]
                        self.compile_expression(object)?;
                        self.emit(Opcode::Dup);
                        self.compile_expression(index)?;
                        self.emit(Opcode::Dup);
                        self.emit(Opcode::GetIndex);
                        self.compile_expression(value)?;
                        match op {
                            CompoundAssignOp::Add => self.emit(Opcode::Add),
                            CompoundAssignOp::Subtract => self.emit(Opcode::Sub),
                            CompoundAssignOp::Multiply => self.emit(Opcode::Mul),
                            CompoundAssignOp::Divide => self.emit(Opcode::Div),
                            CompoundAssignOp::Modulo => self.emit(Opcode::Mod),
                        };
                        self.emit(Opcode::SetIndex);
                    }
                    _ => {
                        return Err(CompilerError::new("Invalid compound assignment target"));
                    }
                }

                Ok(())
            }
            Expr::Postfix { op, operand, .. } => {
                match operand.as_ref() {
                    Expr::Identifier(name, _) => {
                        let load_res = self.resolve_for_load(name);
                        let (store_res, create_local) = self.resolve_for_store(name);
                        self.check_assign_const(name)?;
                        // Load current value (from outer/global if this store will create a local)
                        match load_res {
                            VarRes::Captured | VarRes::ConstCaptured => {
                                self.track_captured(name, load_res);
                                self.emit(Opcode::LoadCaptured(name.clone().into()));
                            }
                            VarRes::ModuleScope | VarRes::ConstModuleScope => {
                                // §70d: read current value from module env
                                // before a §4-local store (compound/postfix).
                                self.track_captured(name, load_res);
                                self.emit(Opcode::LoadCaptured(name.clone().into()));
                            }
                            VarRes::Local | VarRes::ConstLocal => {
                                self.emit(Opcode::LoadLocal(name.clone().into()));
                            }
                            VarRes::Global | VarRes::ConstGlobal => {
                                self.emit(Opcode::LoadGlobal(name.clone().into()));
                            }
                        }

                        // Duplicate for return value
                        self.emit(Opcode::Dup);

                        // Load 1
                        let one_idx = self.add_constant(Constant::Number(1.0));
                        self.emit(Opcode::LoadConst(one_idx));

                        // Perform operation
                        match op {
                            PostfixOp::Increment => self.emit(Opcode::Add),
                            PostfixOp::Decrement => self.emit(Opcode::Sub),
                        };

                        // Store back (consumes the new value; leaves original on stack)
                        match store_res {
                            VarRes::Captured | VarRes::ConstCaptured | VarRes::ConstModuleScope => {
                                self.emit(Opcode::StoreCaptured(name.clone().into()));
                            }
                            VarRes::Local | VarRes::ConstLocal | VarRes::ModuleScope => {
                                self.emit(Opcode::StoreLocal(name.clone().into()));
                            }
                            VarRes::Global | VarRes::ConstGlobal => {
                                self.emit(Opcode::StoreGlobal(name.clone().into()));
                            }
                        }
                        if create_local {
                            self.register_implicit_local(name);
                        }

                        // Stack now has: [original_value]
                    }
                    _ => {
                        return Err(CompilerError::new(
                            "Postfix operator requires an identifier",
                        ));
                    }
                }

                Ok(())
            }
            Expr::Call { callee, args, span } => {
                // Statically resolvable builtin call: no callee is pushed, the
                // operation name travels in the operand (§145, §146).
                if let Some(target) = self.static_call_target(callee) {
                    for arg in args {
                        self.compile_expression(arg)?;
                    }
                    match target {
                        StaticCallTarget::Native(operation) => {
                            self.emit(Opcode::CallNative(operation.into(), args.len()))
                        }
                        StaticCallTarget::ListAdd => self.emit(Opcode::ListAdd(args.len())),
                        StaticCallTarget::ListRemove => self.emit(Opcode::ListRemove(args.len())),
                        StaticCallTarget::ObjectRemove => {
                            self.emit(Opcode::ObjectRemove(args.len()))
                        }
                    };
                    return Ok(());
                }

                // Load callee
                self.compile_expression(callee)?;

                // Load arguments
                for arg in args {
                    self.compile_expression(arg)?;
                }

                // Call
                self.emit_located(Opcode::Call(args.len()), span);

                Ok(())
            }
            Expr::Member { object, property, span } => {
                self.compile_expression(object)?;
                self.emit_located(Opcode::GetProperty(property.clone().into()), span);
                Ok(())
            }
            Expr::Index { object, index, span } => {
                self.compile_expression(object)?;
                self.compile_expression(index)?;
                self.emit_located(Opcode::GetIndex, span);
                Ok(())
            }
            Expr::List { elements, .. } => {
                for elem in elements {
                    self.compile_expression(elem)?;
                }
                self.emit(Opcode::MakeList(elements.len()));
                Ok(())
            }
            Expr::Object { properties, .. } => {
                let keys: Vec<Rc<str>> = properties.iter().map(|p| Rc::from(p.key.clone())).collect();
                for prop in properties {
                    self.compile_expression(&prop.value)?;
                }
                self.emit(Opcode::MakeObject(keys));
                Ok(())
            }
            Expr::Function { params, body, .. } => {
                // Create anonymous function
                let func_idx = self.functions.len();

                // Defaults are compiled into default_bodies inside the
                // function scope below (Job 022, §30). The FunctionInfo is
                // pushed first so nested function expressions keep the
                // functions/function_bodies index pairing (§143).
                self.functions.push(FunctionInfo {
                    name: String::new(),
                    params: params.iter().map(|p| p.name.clone()).collect(),
                    param_defaults: Vec::new(),
                    entry_point: 0,
                    local_count: 0,
                    captured_vars: Vec::new(),
                    is_async: false,
                    source_file: String::new(),
                    debug: Vec::new(),
                    default_debug: Vec::new(),
                });

                // Save and start new compilation context (keep parent scopes for capture detection)
                let saved_instructions = std::mem::take(&mut self.instructions);
                let saved_debug_entries = std::mem::take(&mut self.debug_entries);
                let saved_constants = std::mem::take(&mut self.constants);
                let saved_captured = std::mem::take(&mut self.captured_vars);
                let saved_scope_depth = self.scope_depth;
                let saved_scopes = std::mem::take(&mut self.scopes);
                let saved_function_base = self.function_base;
                let saved_bodies_count = self.function_bodies.len();
                let saved_loop_contexts = std::mem::take(&mut self.loop_contexts);
                let saved_try_contexts = std::mem::take(&mut self.try_contexts);
                let saved_patchable = std::mem::take(&mut self.patchable_constants);
                let saved_in_async = self.in_async;
                // Nested function/arrow expressions are never async (§49: no
                // async function expressions), so `await` inside them is an error.
                self.in_async = false;

                self.scopes = saved_scopes.clone();
                self.scopes.push(Vec::new());
                self.scope_depth += 1;
                self.function_base = self.scopes.len() - 1;
                let saved_defaults_count = self.default_bodies.len();
                // Job 022 (§30): compile defaults left to right; each
                // parameter is declared after its default.
                let mut param_defaults = Vec::new();
                for (i, param) in params.iter().enumerate() {
                    if let Some(ref default_expr) = param.default_value {
                        self.validate_default_param_refs(default_expr, params, i)?;
                        let default_idx = self.compile_default_body(default_expr)?;
                        param_defaults.push(Some(default_idx));
                    } else {
                        param_defaults.push(None);
                    }
                    self.declare_variable(&param.name, false)?;
                }
                self.functions[func_idx].default_debug = param_defaults
                    .iter()
                    .map(|o| o.map(|i| self.default_bodies_debug[i].clone()))
                    .collect();
                self.functions[func_idx].param_defaults = param_defaults;

                let entry_point = self.instructions.len();

                for stmt in body {
                    self.compile_statement(stmt)?;
                }

                let null_idx = self.add_constant(Constant::Null);
                self.emit(Opcode::LoadConst(null_idx));
                self.emit(Opcode::Return);

                let local_count = self.current_scope().len();
                let captured = std::mem::take(&mut self.captured_vars);

                let mut func_instructions = std::mem::take(&mut self.instructions);
                let mut func_debug = std::mem::take(&mut self.debug_entries);
                self.functions[func_idx].debug = func_debug.clone();
                let func_constants = std::mem::take(&mut self.constants);

                // Collect and remap any inner function bodies
                let mut inner_bodies: Vec<Vec<Opcode>> = self.function_bodies.drain(saved_bodies_count..).collect();
                let inner_debug: Vec<Vec<DebugEntry>> =
                    self.function_bodies_debug.drain(saved_bodies_count..).collect();

                self.scopes = saved_scopes;
                self.instructions = saved_instructions;
                self.debug_entries = saved_debug_entries;
                self.constants = saved_constants;
                self.captured_vars = saved_captured;
                self.scope_depth = saved_scope_depth;
                self.function_base = saved_function_base;
                self.loop_contexts = saved_loop_contexts;
                self.try_contexts = saved_try_contexts;
                self.patchable_constants = saved_patchable;
                self.in_async = saved_in_async;

                // Merge function constants into main pool and remap indices
                let constant_offset = self.constants.len();
                for func_const in &func_constants {
                    self.constants.push(func_const.clone());
                }
                for instr in &mut func_instructions {
                    if let Opcode::LoadConst(idx) = instr {
                        *idx += constant_offset;
                    }
                }
                for body in &mut inner_bodies {
                    for instr in body {
                        if let Opcode::LoadConst(idx) = instr {
                            *idx += constant_offset;
                        }
                    }
                }
                // Job 022 (§30): remap default bodies by the same offset.
                for body in &mut self.default_bodies[saved_defaults_count..] {
                    for instr in body {
                        if let Opcode::LoadConst(idx) = instr {
                            *idx += constant_offset;
                        }
                    }
                }

                self.functions[func_idx].entry_point = entry_point;
                self.functions[func_idx].local_count = local_count;
                self.functions[func_idx].captured_vars = captured.iter().map(|(n, _)| n.to_string()).collect();

                // Store function reference
                // Function metadata stays in the constant pool (§126) but the
                // value itself is pushed by LOAD_FUNCTION (§141).
                self.add_constant(Constant::Function(func_idx));
                self.emit(Opcode::LoadFunction(func_idx));

                if !captured.is_empty() {
                    self.emit(Opcode::MakeClosure(func_idx, captured));
                }

                // Store function body for VM execution
                self.function_bodies.push(func_instructions);
                self.function_bodies_debug.push(std::mem::take(&mut func_debug));
                for body in inner_bodies {
                    self.function_bodies.push(body);
                }
                for dbg in inner_debug {
                    self.function_bodies_debug.push(dbg);
                }

                Ok(())
            }
            Expr::ArrowFunction { params, body, .. } => {
                // Similar to function expression
                let func_idx = self.functions.len();

                // Defaults are compiled into default_bodies inside the
                // function scope below (Job 022, §30). FunctionInfo is
                // pushed first to keep the functions/function_bodies index
                // pairing (§143).
                self.functions.push(FunctionInfo {
                    name: String::new(),
                    params: params.iter().map(|p| p.name.clone()).collect(),
                    param_defaults: Vec::new(),
                    entry_point: 0,
                    local_count: 0,
                    captured_vars: Vec::new(),
                    is_async: false,
                    source_file: String::new(),
                    debug: Vec::new(),
                    default_debug: Vec::new(),
                });

                let saved_instructions = std::mem::take(&mut self.instructions);
                let saved_debug_entries = std::mem::take(&mut self.debug_entries);
                let saved_constants = std::mem::take(&mut self.constants);
                let saved_captured = std::mem::take(&mut self.captured_vars);
                let saved_scope_depth = self.scope_depth;
                let saved_scopes = std::mem::take(&mut self.scopes);
                let saved_function_base = self.function_base;
                let saved_bodies_count = self.function_bodies.len();
                let saved_loop_contexts = std::mem::take(&mut self.loop_contexts);
                let saved_try_contexts = std::mem::take(&mut self.try_contexts);
                let saved_patchable = std::mem::take(&mut self.patchable_constants);
                let saved_in_async = self.in_async;
                // Nested function/arrow expressions are never async (§49: no
                // async function expressions), so `await` inside them is an error.
                self.in_async = false;

                self.scopes = saved_scopes.clone();
                self.scopes.push(Vec::new());
                self.scope_depth += 1;
                self.function_base = self.scopes.len() - 1;
                let saved_defaults_count = self.default_bodies.len();
                // Job 022 (§30): compile defaults left to right; each
                // parameter is declared after its default. Declaring the
                // parameters here is also what binds them as locals for the
                // body (arrow parameters previously never declared).
                let mut param_defaults = Vec::new();
                for (i, param) in params.iter().enumerate() {
                    if let Some(ref default_expr) = param.default_value {
                        self.validate_default_param_refs(default_expr, params, i)?;
                        let default_idx = self.compile_default_body(default_expr)?;
                        param_defaults.push(Some(default_idx));
                    } else {
                        param_defaults.push(None);
                    }
                    self.declare_variable(&param.name, false)?;
                }
                self.functions[func_idx].default_debug = param_defaults
                    .iter()
                    .map(|o| o.map(|i| self.default_bodies_debug[i].clone()))
                    .collect();
                self.functions[func_idx].param_defaults = param_defaults;

                let entry_point = self.instructions.len();

                match body {
                    ArrowBody::Expression(expr) => {
                        self.compile_expression(expr)?;
                        self.emit(Opcode::Return);
                    }
                    ArrowBody::Block(stmts) => {
                        for stmt in stmts {
                            self.compile_statement(stmt)?;
                        }
                        let null_idx = self.add_constant(Constant::Null);
                        self.emit(Opcode::LoadConst(null_idx));
                        self.emit(Opcode::Return);
                    }
                }

                let local_count = self.current_scope().len();
                let captured = std::mem::take(&mut self.captured_vars);

                let mut func_instructions = std::mem::take(&mut self.instructions);
                let mut func_debug = std::mem::take(&mut self.debug_entries);
                self.functions[func_idx].debug = func_debug.clone();
                let func_constants = std::mem::take(&mut self.constants);

                // Collect and remap any inner function bodies
                let mut inner_bodies: Vec<Vec<Opcode>> = self.function_bodies.drain(saved_bodies_count..).collect();
                let inner_debug: Vec<Vec<DebugEntry>> =
                    self.function_bodies_debug.drain(saved_bodies_count..).collect();

                self.scopes = saved_scopes;
                self.instructions = saved_instructions;
                self.debug_entries = saved_debug_entries;
                self.constants = saved_constants;
                self.captured_vars = saved_captured;
                self.scope_depth = saved_scope_depth;
                self.function_base = saved_function_base;
                self.loop_contexts = saved_loop_contexts;
                self.try_contexts = saved_try_contexts;
                self.patchable_constants = saved_patchable;
                self.in_async = saved_in_async;

                // Merge function constants into main pool and remap indices
                let constant_offset = self.constants.len();
                for func_const in &func_constants {
                    self.constants.push(func_const.clone());
                }
                for instr in &mut func_instructions {
                    if let Opcode::LoadConst(idx) = instr {
                        *idx += constant_offset;
                    }
                }
                for body in &mut inner_bodies {
                    for instr in body {
                        if let Opcode::LoadConst(idx) = instr {
                            *idx += constant_offset;
                        }
                    }
                }
                // Job 022 (§30): remap default bodies by the same offset.
                for body in &mut self.default_bodies[saved_defaults_count..] {
                    for instr in body {
                        if let Opcode::LoadConst(idx) = instr {
                            *idx += constant_offset;
                        }
                    }
                }

                self.functions[func_idx].entry_point = entry_point;
                self.functions[func_idx].local_count = local_count;
                self.functions[func_idx].captured_vars = captured.iter().map(|(n, _)| n.to_string()).collect();

                // Function metadata stays in the constant pool (§126) but the
                // value itself is pushed by LOAD_FUNCTION (§141).
                self.add_constant(Constant::Function(func_idx));
                self.emit(Opcode::LoadFunction(func_idx));

                if !captured.is_empty() {
                    self.emit(Opcode::MakeClosure(func_idx, captured));
                }

                // Store function body for VM execution
                self.function_bodies.push(func_instructions);
                self.function_bodies_debug.push(std::mem::take(&mut func_debug));
                for body in inner_bodies {
                    self.function_bodies.push(body);
                }
                for dbg in inner_debug {
                    self.function_bodies_debug.push(dbg);
                }

                Ok(())
            }
            Expr::Ternary {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.compile_expression(condition)?;
                let jump_if_false = self.emit(Opcode::JumpIfFalse(0));
                self.emit(Opcode::Pop);
                self.compile_expression(then_branch)?;
                let jump_to_end = self.emit(Opcode::Jump(0));
                self.patch_jump(jump_if_false);
                self.emit(Opcode::Pop);
                self.compile_expression(else_branch)?;
                self.patch_jump(jump_to_end);
                Ok(())
            }
            Expr::Await { expr, .. } => {
                // §49/§101: await is only valid inside an async function.
                if !self.in_async {
                    return Err(CompilerError::new(
                        "'await' is only allowed inside an async function",
                    ));
                }
                self.compile_expression(expr)?;
                self.emit(Opcode::Await);
                Ok(())
            }
        }
    }
}

/// Job 022 (§30): collect every identifier read (or assigned) by an
/// expression, skipping nested function/arrow subtrees — those declare their
/// own scope, so names inside them do not bind this function's parameters.
fn collect_ident_refs(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::NumberLiteral(..)
        | Expr::StringLiteral(..)
        | Expr::BooleanLiteral(..)
        | Expr::NullLiteral(..) => {}
        Expr::Identifier(name, _) => out.push(name.clone()),
        Expr::Unary { operand, .. } | Expr::Postfix { operand, .. } => {
            collect_ident_refs(operand, out);
        }
        Expr::Binary { left, right, .. } => {
            collect_ident_refs(left, out);
            collect_ident_refs(right, out);
        }
        Expr::Assignment { target, value, .. }
        | Expr::CompoundAssignment { target, value, .. } => {
            collect_ident_refs(target, out);
            collect_ident_refs(value, out);
        }
        Expr::Call { callee, args, .. } => {
            collect_ident_refs(callee, out);
            for arg in args {
                collect_ident_refs(arg, out);
            }
        }
        Expr::Member { object, .. } => collect_ident_refs(object, out),
        Expr::Index { object, index, .. } => {
            collect_ident_refs(object, out);
            collect_ident_refs(index, out);
        }
        Expr::List { elements, .. } => {
            for element in elements {
                collect_ident_refs(element, out);
            }
        }
        Expr::Object { properties, .. } => {
            for prop in properties {
                collect_ident_refs(&prop.value, out);
            }
        }
        // New scope: names inside resolve against the nested function's own
        // parameters/locals, not this function's later parameters.
        Expr::Function { .. } | Expr::ArrowFunction { .. } => {}
        Expr::Ternary {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_ident_refs(condition, out);
            collect_ident_refs(then_branch, out);
            collect_ident_refs(else_branch, out);
        }
        Expr::Await { expr, .. } => collect_ident_refs(expr, out),
    }
}

pub fn compile(program: &Program) -> Result<BytecodeProgram, CompilerError> {
    let mut compiler = Compiler::new();
    compiler.compile_program(program)?;

    Ok(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
        constants: compiler.constants,
        instructions: compiler.instructions,
        functions: compiler.functions,
        function_bodies: compiler.function_bodies,
        default_bodies: compiler.default_bodies,
        debug: compiler.debug_entries,
    })
}

/// Compile a single file as a module (module-mode scopes, EndModule terminator).
/// Function bodies stay in `function_bodies` for the loader to merge across modules.
pub fn compile_module(path: &str, program: &Program) -> Result<ModuleUnit, CompilerError> {
    let mut compiler = Compiler::new_module();
    compiler.source_file = path.to_string();
    compiler.seed_export_names(program);
    for stmt in &program.statements {
        compiler.compile_statement(stmt)?;
    }
    compiler.emit(Opcode::EndModule);

    Ok(ModuleUnit {
        path: path.to_string(),
        instructions: compiler.instructions,
        constants: compiler.constants,
        functions: compiler.functions,
        function_bodies: compiler.function_bodies,
        default_bodies: compiler.default_bodies,
        debug: compiler.debug_entries,
        exports: compiler.exports,
        imports: compiler.imports,
        re_exports: compiler.re_exports,
    })
}

/// Merge module units into one program: instruction ranges concatenated,
/// constants/functions/bodies offset so cross-module indices resolve.
pub fn merge_modules(units: &[ModuleUnit]) -> BytecodeProgram {
    let mut combined = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
        constants: Vec::new(),
        instructions: Vec::new(),
        functions: Vec::new(),
        function_bodies: Vec::new(),
        default_bodies: Vec::new(),
        debug: Vec::new(),
    };

    for unit in units {
        let const_offset = combined.constants.len();
        let func_offset = combined.functions.len();
        let instr_offset = combined.instructions.len();

        for instr in &unit.instructions {
            combined.instructions.push(remap_instr(
                instr,
                const_offset,
                instr_offset,
                func_offset,
            ));
        }

        for c in &unit.constants {
            match c {
                Constant::Function(idx) => {
                    combined.constants.push(Constant::Function(idx + func_offset));
                }
                other => combined.constants.push(other.clone()),
            }
        }

        // Job 036 (ISSUES §75f): top-level debug entries follow the module's
        // instruction range into the merged program.
        for entry in &unit.debug {
            combined.debug.push(DebugEntry {
                pc: entry.pc + instr_offset as u32,
                line: entry.line,
                column: entry.column,
            });
        }

        for func in &unit.functions {
            let mut f = func.clone();
            // Job 036: source path for `e.location.file` (ISSUES §75f).
            f.source_file = unit.path.clone();
            // entry_point is fixed after all bodies are appended below.
            f.entry_point = 0;
            // param_defaults index default_bodies (Job 022, §30), which are
            // appended for this unit in the same pass — indices stay stable.
            combined.functions.push(f);
        }

        for body in &unit.function_bodies {
            let mut remapped: Vec<Opcode> = body
                .iter()
                .map(|instr| remap_body_instr(instr, const_offset, func_offset))
                .collect();
            combined.function_bodies.push(std::mem::take(&mut remapped));
        }

        // Job 022 (§30): default-expression bodies merge like function
        // bodies — constants and function indices offset by the same pass.
        for body in &unit.default_bodies {
            let remapped: Vec<Opcode> = body
                .iter()
                .map(|instr| remap_body_instr(instr, const_offset, func_offset))
                .collect();
            combined.default_bodies.push(remapped);
        }
    }

    // Append function bodies and set absolute entry points (mirrors compile_program).
    for (i, body) in combined.function_bodies.iter().enumerate() {
        combined.functions[i].entry_point = combined.instructions.len();
        combined.instructions.extend(body.iter().cloned());
    }

    combined
}

fn remap_instr(instr: &Opcode, const_offset: usize, instr_offset: usize, func_offset: usize) -> Opcode {
    match instr {
        Opcode::LoadConst(idx) => Opcode::LoadConst(idx + const_offset),
        Opcode::LoadFunction(idx) => Opcode::LoadFunction(idx + func_offset),
        Opcode::Jump(t) => Opcode::Jump(t + instr_offset),
        Opcode::JumpIfTrue(t) => Opcode::JumpIfTrue(t + instr_offset),
        Opcode::JumpIfFalse(t) => Opcode::JumpIfFalse(t + instr_offset),
        Opcode::TryPush { catch, finally } => Opcode::TryPush {
            catch: catch.map(|t| t + instr_offset),
            finally: finally.map(|t| t + instr_offset),
        },
        Opcode::MakeClosure(idx, captured) => Opcode::MakeClosure(idx + func_offset, captured.clone()),
        other => other.clone(),
    }
}

/// Remap inside a function body: jumps stay body-relative (pc starts at 0).
fn remap_body_instr(instr: &Opcode, const_offset: usize, func_offset: usize) -> Opcode {
    match instr {
        Opcode::LoadConst(idx) => Opcode::LoadConst(idx + const_offset),
        Opcode::LoadFunction(idx) => Opcode::LoadFunction(idx + func_offset),
        Opcode::MakeClosure(idx, captured) => Opcode::MakeClosure(idx + func_offset, captured.clone()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn compile_source(source: &str) -> BytecodeProgram {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse().unwrap();
        compile(&program).unwrap()
    }

    #[test]
    fn test_empty_program() {
        let bc = compile_source("");
        assert_eq!(bc.instructions.len(), 1);
        assert!(matches!(bc.instructions[0], Opcode::Halt));
    }

    #[test]
    fn test_number_literal() {
        let bc = compile_source("42");
        assert_eq!(bc.constants.len(), 1);
        assert_eq!(bc.constants[0], Constant::Number(42.0));
        // LoadConst + Pop + Halt
        assert_eq!(bc.instructions.len(), 3);
        assert!(matches!(bc.instructions[0], Opcode::LoadConst(0)));
        assert!(matches!(bc.instructions[1], Opcode::Pop));
        assert!(matches!(bc.instructions[2], Opcode::Halt));
    }

    #[test]
    fn test_string_literal() {
        let bc = compile_source(r#""hello""#);
        assert_eq!(bc.constants.len(), 1);
        assert_eq!(bc.constants[0], Constant::Str("hello".to_string()));
    }

    #[test]
    fn test_boolean_literal() {
        let bc = compile_source("true false");
        assert_eq!(bc.constants.len(), 2);
        assert_eq!(bc.constants[0], Constant::Boolean(true));
        assert_eq!(bc.constants[1], Constant::Boolean(false));
    }

    #[test]
    fn test_null_literal() {
        let bc = compile_source("null");
        assert_eq!(bc.constants.len(), 1);
        assert_eq!(bc.constants[0], Constant::Null);
    }

    #[test]
    fn test_let_declaration() {
        let bc = compile_source("let A = 10");
        assert!(bc.instructions.contains(&Opcode::DeclareGlobal("A".into(), false)));
        assert!(bc.instructions.contains(&Opcode::StoreGlobal("A".into())));
    }

    #[test]
    fn test_const_declaration() {
        let bc = compile_source("const A = 10");
        assert!(bc.instructions.contains(&Opcode::DeclareGlobal("A".into(), true)));
    }

    #[test]
    fn test_identifier_access() {
        let bc = compile_source("let A = 10\nA");
        // Should have LoadGlobal for A
        assert!(bc.instructions.iter().any(|op| matches!(op, Opcode::LoadGlobal(name) if name.as_ref() == "A")));
    }

    #[test]
    fn test_assignment() {
        let bc = compile_source("let A = 10\nA = 20");
        assert!(bc.instructions.contains(&Opcode::StoreGlobal("A".into())));
    }

    #[test]
    fn test_binary_add() {
        let bc = compile_source("1 + 2");
        assert!(bc.instructions.contains(&Opcode::Add));
    }

    #[test]
    fn test_binary_subtract() {
        let bc = compile_source("5 - 3");
        assert!(bc.instructions.contains(&Opcode::Sub));
    }

    #[test]
    fn test_binary_multiply() {
        let bc = compile_source("3 * 4");
        assert!(bc.instructions.contains(&Opcode::Mul));
    }

    #[test]
    fn test_binary_divide() {
        let bc = compile_source("10 / 2");
        assert!(bc.instructions.contains(&Opcode::Div));
    }

    #[test]
    fn test_binary_modulo() {
        let bc = compile_source("10 % 3");
        assert!(bc.instructions.contains(&Opcode::Mod));
    }

    #[test]
    fn test_binary_power() {
        let bc = compile_source("2 ** 3");
        assert!(bc.instructions.contains(&Opcode::Pow));
    }

    #[test]
    fn test_comparison() {
        let bc = compile_source("10 > 5");
        assert!(bc.instructions.contains(&Opcode::Gt));
    }

    #[test]
    fn test_equality() {
        let bc = compile_source("10 == 10");
        assert!(bc.instructions.contains(&Opcode::Eq));
    }

    #[test]
    fn test_strict_equality() {
        let bc = compile_source("10 === 10");
        assert!(bc.instructions.contains(&Opcode::RawEq));
    }

    #[test]
    fn test_boolean_and() {
        let bc = compile_source("true and false");
        assert!(bc.instructions.iter().any(|op| matches!(op, Opcode::JumpIfFalse(_))));
    }

    #[test]
    fn test_boolean_or() {
        let bc = compile_source("true or false");
        assert!(bc.instructions.iter().any(|op| matches!(op, Opcode::JumpIfTrue(_))));
    }

    #[test]
    fn test_unary_negate() {
        let bc = compile_source("-10");
        assert!(bc.instructions.contains(&Opcode::Neg));
    }

    #[test]
    fn test_unary_not() {
        let bc = compile_source("!true");
        assert!(bc.instructions.contains(&Opcode::Not));
    }

    #[test]
    fn test_if_statement() {
        let bc = compile_source("if true { 10 }");
        assert!(bc.instructions.iter().any(|op| matches!(op, Opcode::JumpIfFalse(_))));
    }

    #[test]
    fn test_if_else() {
        let bc = compile_source("if true { 10 } else { 20 }");
        assert!(bc.instructions.iter().any(|op| matches!(op, Opcode::JumpIfFalse(_))));
        assert!(bc.instructions.iter().any(|op| matches!(op, Opcode::Jump(_))));
    }

    #[test]
    fn test_while_loop() {
        let bc = compile_source("while true { 10 }");
        assert!(bc.instructions.iter().any(|op| matches!(op, Opcode::JumpIfFalse(_))));
        assert!(bc.instructions.iter().any(|op| matches!(op, Opcode::Jump(_))));
    }

    #[test]
    fn test_loop() {
        let bc = compile_source("loop { 10 }");
        assert!(bc.instructions.iter().any(|op| matches!(op, Opcode::Jump(_))));
    }

    #[test]
    fn test_break() {
        let bc = compile_source("loop { break }");
        assert!(bc.instructions.iter().any(|op| matches!(op, Opcode::Jump(_))));
    }

    #[test]
    fn test_continue() {
        let bc = compile_source("loop { continue }");
        assert!(bc.instructions.iter().any(|op| matches!(op, Opcode::Jump(_))));
    }

    #[test]
    fn test_switch_case() {
        let bc = compile_source("switch A { case 1 { } }");
        assert!(bc.instructions.contains(&Opcode::Eq));
    }

    #[test]
    fn test_function_declaration() {
        let bc = compile_source("function test(A, B) { return A + B }");
        assert!(bc.functions.len() >= 1);
        assert_eq!(bc.functions[0].params, vec!["A".to_string(), "B".to_string()]);
    }

    #[test]
    fn test_function_call() {
        let bc = compile_source("function test() { }\ntest()");
        assert!(bc.instructions.contains(&Opcode::Call(0)));
    }

    #[test]
    fn test_return() {
        let bc = compile_source("function test() { return 10 }");
        // Function is compiled, check that it exists
        assert!(!bc.functions.is_empty());
        // The function body is compiled separately, so Return is in the function's entry point
        // For now, just verify the function exists
    }

    #[test]
    fn test_list_literal() {
        let bc = compile_source("[10, 20, 30]");
        assert!(bc.instructions.contains(&Opcode::MakeList(3)));
    }

    #[test]
    fn test_empty_list() {
        let bc = compile_source("[]");
        assert!(bc.instructions.contains(&Opcode::MakeList(0)));
    }

    #[test]
    fn test_object_literal() {
        let bc = compile_source(r#"{ name: "Kael", age: 18 }"#);
        assert!(bc.instructions.iter().any(|op| matches!(op, Opcode::MakeObject(_))));
    }

    #[test]
    fn test_member_access() {
        let bc = compile_source("user.name");
        assert!(bc.instructions.contains(&Opcode::GetProperty("name".into())));
    }

    #[test]
    fn test_index_access() {
        let bc = compile_source("arr[0]");
        assert!(bc.instructions.contains(&Opcode::GetIndex));
    }

    #[test]
    fn test_compound_assignment() {
        let bc = compile_source("let A = 10\nA += 5");
        assert!(bc.instructions.contains(&Opcode::Add));
        assert!(bc.instructions.contains(&Opcode::StoreGlobal("A".into())));
    }

    #[test]
    fn test_postfix_increment() {
        let bc = compile_source("let A = 10\nA++");
        assert!(bc.instructions.contains(&Opcode::Add));
    }

    #[test]
    fn test_postfix_decrement() {
        let bc = compile_source("let A = 10\nA--");
        assert!(bc.instructions.contains(&Opcode::Sub));
    }

    #[test]
    fn test_ternary() {
        let bc = compile_source("true ? 1 : 2");
        assert!(bc.instructions.iter().any(|op| matches!(op, Opcode::JumpIfFalse(_))));
        assert!(bc.instructions.iter().any(|op| matches!(op, Opcode::Jump(_))));
    }

    #[test]
    fn test_constant_deduplication() {
        let bc = compile_source("10 + 10");
        // Should have only one constant for 10
        let count = bc.constants.iter().filter(|c| **c == Constant::Number(10.0)).count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_multiple_constants() {
        let bc = compile_source("10 \"hello\" true null");
        assert_eq!(bc.constants.len(), 4);
        assert!(bc.constants.contains(&Constant::Number(10.0)));
        assert!(bc.constants.contains(&Constant::Str("hello".to_string())));
        assert!(bc.constants.contains(&Constant::Boolean(true)));
        assert!(bc.constants.contains(&Constant::Null));
    }

    #[test]
    fn test_nested_expression() {
        let bc = compile_source("(1 + 2) * 3");
        assert!(bc.instructions.contains(&Opcode::Add));
        assert!(bc.instructions.contains(&Opcode::Mul));
    }

    #[test]
    fn test_arrow_function() {
        let bc = compile_source("let add = (A, B) => A + B");
        assert!(bc.functions.len() >= 1);
    }

    #[test]
    fn test_anonymous_function() {
        let bc = compile_source("let f = function(A) { return A }");
        assert!(bc.functions.len() >= 1);
    }

    #[test]
    fn test_pop_after_expression_statement() {
        let bc = compile_source("10");
        // Last two instructions should be Pop and Halt
        assert!(matches!(bc.instructions[bc.instructions.len() - 2], Opcode::Pop));
        assert!(matches!(bc.instructions[bc.instructions.len() - 1], Opcode::Halt));
    }

    #[test]
    fn test_no_pop_for_declaration() {
        let bc = compile_source("let A = 10");
        assert!(!bc.instructions.contains(&Opcode::Pop));
    }

    #[test]
    fn test_block_scope_emits_push_pop() {
        let bc = compile_source("if true { let A = 10 }");
        assert!(bc.instructions.contains(&Opcode::PushScope));
        assert!(bc.instructions.contains(&Opcode::PopScope));
    }

    #[test]
    fn test_global_load_store() {
        let bc = compile_source("let A = 1\nA = 2\nA");
        assert!(bc.instructions.contains(&Opcode::DeclareGlobal("A".into(), false)));
        assert!(bc.instructions.contains(&Opcode::StoreGlobal("A".into())));
        assert!(bc.instructions.contains(&Opcode::LoadGlobal("A".into())));
    }

    #[test]
    fn test_break_outside_loop_errors() {
        let mut lexer = Lexer::new("break");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse().unwrap();
        assert!(compile(&program).is_err());
    }

    #[test]
    fn test_continue_outside_loop_errors() {
        let mut lexer = Lexer::new("continue");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse().unwrap();
        assert!(compile(&program).is_err());
    }

    #[test]
    fn test_comparison_operators() {
        let bc = compile_source("10 > 5");
        assert!(bc.instructions.contains(&Opcode::Gt));

        let bc = compile_source("10 < 5");
        assert!(bc.instructions.contains(&Opcode::Lt));

        let bc = compile_source("10 >= 5");
        assert!(bc.instructions.contains(&Opcode::Gte));

        let bc = compile_source("10 <= 5");
        assert!(bc.instructions.contains(&Opcode::Lte));

        let bc = compile_source("10 != 5");
        assert!(bc.instructions.contains(&Opcode::Neq));
    }

    #[test]
    fn test_unary_positive() {
        let bc = compile_source("+10");
        assert!(bc.instructions.contains(&Opcode::Pos));
    }

    // ---- Job 019: bytecode header, CALL_NATIVE, collection opcodes, LOAD_FUNCTION ----

    fn parse_source(source: &str) -> Program {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        parser.parse().unwrap()
    }

    #[test]
    fn test_job019_header_and_version_written_by_compiler() {
        let bc = compile_source("print(\"x\")");
        assert_eq!(bc.header.magic, BYTECODE_MAGIC);
        assert_eq!(bc.header.version, BYTECODE_VERSION);
        assert!(bc.header.is_supported());
        assert!(SUPPORTED_BYTECODE_VERSIONS.contains(&bc.header.version));
        assert_eq!(bc.entry_point, 0);
    }

    #[test]
    fn test_job019_call_native_for_root_builtin() {
        let bc = compile_source("print(\"x\")");
        assert!(bc
            .instructions
            .contains(&Opcode::CallNative("print".into(), 1)));
        assert!(!bc.instructions.iter().any(|op| matches!(op, Opcode::Call(_))));
    }

    #[test]
    fn test_job019_call_native_for_conversions_and_type() {
        let bc = compile_source("type(1)\nnumber(\"2\")\nstring(1)\nboolean(1)\nprint.err(\"e\")");
        for operation in ["type", "number", "string", "boolean", "print.err"] {
            assert!(
                bc.instructions
                    .iter()
                    .any(|op| matches!(op, Opcode::CallNative(name, _) if &**name == operation)),
                "missing CALL_NATIVE {}",
                operation
            );
        }
        assert!(!bc.instructions.iter().any(|op| matches!(op, Opcode::Call(_))));
    }

    #[test]
    fn test_job019_call_native_for_namespace_methods() {
        let bc = compile_source(
            "string.length(\"ab\")\nlist.length([1])\ncolor.red(\"r\")\nfile.exists(\"x\")\njson.stringify(1)\nsystem.os()\ntask.state(1)\nhttp.get(\"u\")",
        );
        for operation in [
            "string.length",
            "list.length",
            "color.red",
            "file.exists",
            "json.stringify",
            "system.os",
            "task.state",
            "http.get",
        ] {
            assert!(
                bc.instructions
                    .iter()
                    .any(|op| matches!(op, Opcode::CallNative(name, _) if &**name == operation)),
                "missing CALL_NATIVE {}",
                operation
            );
        }
        assert!(!bc.instructions.iter().any(|op| matches!(op, Opcode::Call(_))));
    }

    #[test]
    fn test_job019_collection_instructions_emitted() {
        let bc = compile_source(
            "let A = [1]\nlist.add(A, 2)\nlist.remove(A, 0)\nlet O = { k: 1 }\nobject.remove(O, \"k\")",
        );
        assert!(bc.instructions.contains(&Opcode::ListAdd(2)));
        assert!(bc.instructions.contains(&Opcode::ListRemove(2)));
        assert!(bc.instructions.contains(&Opcode::ObjectRemove(2)));
        assert!(!bc.instructions.iter().any(
            |op| matches!(op, Opcode::CallNative(name, _)
                if matches!(&**name, "list.add" | "list.remove" | "object.remove"))
        ));
    }

    #[test]
    fn test_job019_make_list_and_make_object_stay_core_opcodes() {
        // §144: MAKE_LIST / MAKE_OBJECT are core instructions, not natives.
        let bc = compile_source("[1]\n{ a: 2 }");
        assert!(bc.instructions.contains(&Opcode::MakeList(1)));
        assert!(bc.instructions.iter().any(|op| matches!(op, Opcode::MakeObject(_))));
        assert!(!bc.instructions.iter().any(|op| matches!(op, Opcode::CallNative(..))));
    }

    #[test]
    fn test_job019_unknown_method_keeps_dynamic_call() {
        let bc = compile_source("string.bogus(\"x\")");
        assert!(bc.instructions.contains(&Opcode::GetProperty("bogus".into())));
        assert!(bc.instructions.contains(&Opcode::Call(1)));
        assert!(!bc.instructions.iter().any(|op| matches!(op, Opcode::CallNative(..))));
    }

    #[test]
    fn test_job019_shadowed_builtin_parameter_keeps_dynamic_call() {
        let bc = compile_source("function f(print) { print(7) }");
        let body = &bc.function_bodies[0];
        assert!(body
            .iter()
            .any(|op| matches!(op, Opcode::LoadLocal(name) if &**name == "print")));
        assert!(!body.iter().any(|op| matches!(op, Opcode::CallNative(..))));
    }

    #[test]
    fn test_job019_declared_namespace_root_keeps_dynamic_call() {
        let bc = compile_source("let string = 1\nstring(\"2\")");
        assert!(!bc.instructions.iter().any(|op| matches!(op, Opcode::CallNative(..))));
        assert!(bc.instructions.contains(&Opcode::Call(1)));
    }

    #[test]
    fn test_job019_builtin_as_value_keeps_dynamic_call() {
        let bc = compile_source("let f = string.length");
        assert!(bc.instructions.contains(&Opcode::GetProperty("length".into())));
        assert!(!bc.instructions.iter().any(|op| matches!(op, Opcode::CallNative(..))));
    }

    #[test]
    fn test_job019_load_function_for_function_reference() {
        let bc = compile_source("function f() { return 1 }\nlet g = f");
        assert!(bc.instructions.iter().any(|op| matches!(op, Opcode::LoadFunction(0))));
        // Function references are pushed by LOAD_FUNCTION, not LOAD_CONST.
        assert!(!bc.instructions.iter().any(
            |op| matches!(op, Opcode::LoadConst(idx) if matches!(bc.constants[*idx], Constant::Function(_)))
        ));
        // The metadata itself stays in the pool (§126) for index consumers.
        assert!(bc
            .constants
            .iter()
            .any(|c| matches!(c, Constant::Function(0))));
    }

    #[test]
    fn test_job019_load_function_inside_function_body() {
        let bc = compile_source("function outer() { let h = () => 5 return h() }");
        let body = &bc.function_bodies[0];
        assert!(body.iter().any(|op| matches!(op, Opcode::LoadFunction(_))));
    }

    #[test]
    fn test_job019_merge_modules_remaps_load_function_and_sets_header() {
        let a = compile_module("./a.zs", &parse_source("export function f() { return 1 }")).unwrap();
        let b = compile_module("./b.zs", &parse_source("export function g() { return 2 }")).unwrap();
        let merged = merge_modules(&[a, b]);

        let mut indices: Vec<usize> = merged
            .instructions
            .iter()
            .filter_map(|op| match op {
                Opcode::LoadFunction(i) => Some(*i),
                _ => None,
            })
            .collect();
        indices.sort();
        assert_eq!(indices, vec![0, 1]);

        assert_eq!(merged.header.magic, BYTECODE_MAGIC);
        assert_eq!(merged.header.version, BYTECODE_VERSION);
        assert_eq!(merged.entry_point, 0);
        assert_eq!(merged.functions.len(), merged.function_bodies.len());
    }

    #[test]
    fn test_job019_every_function_has_a_body_after_compile() {
        let bc = compile_source(
            "function a() { return 1 }\nfunction b() { function c() { return 2 } return c() }\nlet f = () => 3",
        );
        assert_eq!(bc.functions.len(), bc.function_bodies.len());
        for (index, function) in bc.functions.iter().enumerate() {
            let body = &bc.function_bodies[index];
            assert!(function.entry_point + body.len() <= bc.instructions.len());
            assert!(bc.instructions[function.entry_point..function.entry_point + body.len()] == *body);
        }
    }

    // ===== Job 022: default parameters encoded as call-time bodies (§30) =====

    /// §30 "Default tidak dapat menggunakan parameter yang dideklarasikan
    /// setelahnya" is a compile error (test req 4); the previous-parameter
    /// form of the same shape must still compile.
    #[test]
    fn test_job022_later_parameter_reference_is_compile_error() {
        let compile_str = |src: &str| {
            let mut lexer = Lexer::new(src);
            let tokens = lexer.tokenize().unwrap();
            let mut parser = Parser::new(tokens);
            let program = parser.parse().unwrap();
            compile(&program)
        };
        let err = compile_str("function bad(a = b, b) { }").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("'b'") && msg.contains("declared later"),
            "unexpected error message: {}",
            msg
        );
        assert!(compile_str("function ok(a, b = a + 1) { return b }").is_ok());
        // The rule holds for any parameter position, not just the first.
        assert!(compile_str("function bad2(x, y = z, z) { }").is_err());
    }

    /// Job 022 Scope A: defaults are their own bodies in `default_bodies`,
    /// indexed by `FunctionInfo.param_defaults`, not constants spliced into
    /// the caller's instruction stream.
    #[test]
    fn test_job022_default_bodies_encoded_not_in_caller_stream() {
        let bc = compile_source("function f(a = 1, b, c = a + 2) { return b }");
        let function = &bc.functions[0];
        assert_eq!(
            function.param_defaults,
            vec![Some(0), None, Some(1)],
            "param_defaults must index default_bodies"
        );
        assert_eq!(bc.default_bodies.len(), 2);
        // Each default body is a standalone stream ending in Return.
        for body in &bc.default_bodies {
            assert!(!body.is_empty());
            assert!(matches!(body.last(), Some(Opcode::Return)));
        }
        // `a = 1` compiles to exactly the literal load and the return.
        assert_eq!(bc.default_bodies[0].len(), 2);
        assert!(matches!(bc.default_bodies[0][0], Opcode::LoadConst(0)));
    }
}
