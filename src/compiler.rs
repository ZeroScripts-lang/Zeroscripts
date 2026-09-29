use crate::ast::*;
use crate::error::CompilerError;
use crate::native;

/// Magic marker of a ZeroScript bytecode image (§125 conceptual header).
pub const BYTECODE_MAGIC: &str = "ZERO";

/// Bytecode format version emitted by this compiler (§125). Distinct from the
/// ZeroScript language version and from any package/module version.
pub const BYTECODE_VERSION: u32 = 1;

/// Bytecode versions this ZeroVM accepts (§125: compatibility is stated
/// explicitly; an old bytecode is never assumed compatible).
pub const SUPPORTED_BYTECODE_VERSIONS: &[u32] = &[1];

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

#[derive(Debug, Clone, PartialEq)]
pub enum Opcode {
    // Literals
    LoadConst(usize),

    // Function value (§141 `LOAD_FUNCTION`)
    LoadFunction(usize),

    // Variables
    DeclareLocal(String, bool), // name, is_const
    LoadLocal(String),
    StoreLocal(String),
    DeclareGlobal(String, bool), // name, is_const
    LoadGlobal(String),
    StoreGlobal(String),

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
    CallNative(String, usize),
    Return,
    MakeClosure(usize, Vec<(String, bool)>), // captured: (name, is_const)
    LoadCaptured(String),
    StoreCaptured(String),

    // Async (§49, §101): wait for a task to reach a terminal state
    Await,

    // Collections
    MakeList(usize),
    MakeObject(Vec<String>),
    /// `LIST_ADD` (§144): append a value to a mutable list. Operand = argc.
    ListAdd(usize),
    /// `LIST_REMOVE` (§144): remove a list element by index. Operand = argc.
    ListRemove(usize),
    /// `OBJECT_REMOVE` (§144): remove an object property by key. Operand = argc.
    ObjectRemove(usize),
    GetIndex,
    SetIndex,
    GetProperty(String),
    SetProperty(String),

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
    pub param_defaults: Vec<Option<usize>>,
    pub entry_point: usize,
    pub local_count: usize,
    pub captured_vars: Vec<String>,
    /// `async function` (§49): calling it creates a Task (§101).
    pub is_async: bool,
}

struct LoopContext {
    break_jumps: Vec<usize>,
    continue_jumps: Vec<usize>,
    scope_depth: usize, // compiler scopes.len() before the loop body scope was pushed
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
    Global,
    ConstLocal,
    ConstCaptured,
    ConstGlobal,
}

struct Compiler {
    constants: Vec<Constant>,
    instructions: Vec<Opcode>,
    functions: Vec<FunctionInfo>,
    function_bodies: Vec<Vec<Opcode>>,
    scopes: Vec<Vec<ScopeVar>>,
    loop_contexts: Vec<LoopContext>,
    captured_vars: Vec<(String, bool)>,
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
}

impl Compiler {
    fn new() -> Self {
        Self {
            constants: Vec::new(),
            instructions: Vec::new(),
            functions: Vec::new(),
            function_bodies: Vec::new(),
            scopes: vec![Vec::new()],
            loop_contexts: Vec::new(),
            captured_vars: Vec::new(),
            scope_depth: 0,
            function_base: 0,
            exports: Vec::new(),
            imports: Vec::new(),
            re_exports: Vec::new(),
            in_async: false,
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
            scopes: vec![Vec::new(), Vec::new()],
            loop_contexts: Vec::new(),
            captured_vars: Vec::new(),
            scope_depth: 0,
            function_base: 1,
            exports: Vec::new(),
            imports: Vec::new(),
            re_exports: Vec::new(),
            in_async: false,
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
        // Deduplicate constants
        for (i, existing) in self.constants.iter().enumerate() {
            if *existing == constant {
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
                for scope in self.scopes[1..self.function_base].iter().rev() {
                    if let Some(v) = scope.iter().find(|v| v.name == name) {
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
            VarRes::ConstLocal | VarRes::ConstCaptured => Err(CompilerError::new(&format!(
                "Cannot assign to const '{}'",
                name
            ))),
            VarRes::ConstGlobal if self.function_base == 0 => Err(CompilerError::new(&format!(
                "Cannot assign to const '{}'",
                name
            ))),
            _ => Ok(()),
        }
    }

    fn track_captured(&mut self, name: &str, res: VarRes) {
        let is_const = matches!(res, VarRes::ConstCaptured);
        if !self.captured_vars.iter().any(|(n, _)| name == n) {
            self.captured_vars.push((name.to_string(), is_const));
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
        self.loop_contexts.push(LoopContext {
            break_jumps: Vec::new(),
            continue_jumps: Vec::new(),
            scope_depth,
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
                    self.emit(Opcode::DeclareGlobal(name.clone(), *is_const));
                    if let Some(init) = initializer {
                        self.compile_expression(init)?;
                        self.emit(Opcode::StoreGlobal(name.clone()));
                    } else {
                        let idx = self.add_constant(Constant::Null);
                        self.emit(Opcode::LoadConst(idx));
                        self.emit(Opcode::StoreGlobal(name.clone()));
                    }
                } else {
                    self.emit(Opcode::DeclareLocal(name.clone(), *is_const));
                    if let Some(init) = initializer {
                        self.compile_expression(init)?;
                        self.emit(Opcode::StoreLocal(name.clone()));
                    } else {
                        let idx = self.add_constant(Constant::Null);
                        self.emit(Opcode::LoadConst(idx));
                        self.emit(Opcode::StoreLocal(name.clone()));
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

                // Compile default parameter values into main constants
                let mut param_defaults = Vec::new();
                for param in params {
                    if let Some(ref default_expr) = param.default_value {
                        self.compile_expression(default_expr)?;
                        // The last constant added is the default value
                        let default_idx = self.constants.len() - 1;
                        param_defaults.push(Some(default_idx));
                        // Pop the compiled expression from the stack
                        self.emit(Opcode::Pop);
                    } else {
                        param_defaults.push(None);
                    }
                }

                self.functions.push(FunctionInfo {
                    name: name.clone(),
                    params: params.iter().map(|p| p.name.clone()).collect(),
                    param_defaults,
                    entry_point: 0,
                    local_count: 0,
                    captured_vars: Vec::new(),
                    is_async: *is_async,
                });

                let is_global_root = self.function_base == 0 && self.scopes.len() == 1;
                self.declare_variable(name, false)?;

                // Save compilation state (but NOT functions - inner functions need to be added)
                let saved_instructions = std::mem::take(&mut self.instructions);
                let saved_constants = std::mem::take(&mut self.constants);
                let saved_captured = std::mem::take(&mut self.captured_vars);
                let saved_scope_depth = self.scope_depth;
                let saved_scopes = std::mem::take(&mut self.scopes);
                let saved_function_base = self.function_base;
                let saved_bodies_count = self.function_bodies.len();
                let saved_loop_contexts = std::mem::take(&mut self.loop_contexts);
                let saved_in_async = self.in_async;
                self.in_async = *is_async;

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

                // Add parameters
                for param in params.iter() {
                    self.declare_variable(&param.name, false)?;
                }

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
                let func_constants = std::mem::take(&mut self.constants);

                // Collect any inner function bodies that were added
                let mut inner_bodies: Vec<Vec<Opcode>> = self.function_bodies.drain(saved_bodies_count..).collect();

                // Restore outer state
                self.scopes = saved_scopes;
                self.instructions = saved_instructions;
                self.constants = saved_constants;
                self.captured_vars = saved_captured;
                self.scope_depth = saved_scope_depth;
                self.function_base = saved_function_base;
                self.loop_contexts = saved_loop_contexts;
                self.in_async = saved_in_async;

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

                // entry_point is 0 because we compiled into a fresh buffer
                self.functions[func_idx].entry_point = 0;
                self.functions[func_idx].local_count = local_count;
                self.functions[func_idx].captured_vars = captured.iter().map(|(n, _)| n.clone()).collect();

                // Store function reference
                // Function metadata stays in the constant pool (§126) — index
                // based consumers such as param defaults keep their indices —
                // but the value itself is pushed by LOAD_FUNCTION (§141).
                self.add_constant(Constant::Function(func_idx));
                self.emit(Opcode::LoadFunction(func_idx));

                if !captured.is_empty() {
                    self.emit(Opcode::MakeClosure(func_idx, captured));
                }

                if is_global_root {
                    self.emit(Opcode::StoreGlobal(name.clone()));
                } else {
                    self.emit(Opcode::StoreLocal(name.clone()));
                }

                // Store this function's body first, then inner bodies
                self.function_bodies.push(remapped_instructions);
                for body in inner_bodies {
                    self.function_bodies.push(body);
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
                let target_depth = self.loop_contexts.last().unwrap().scope_depth;
                self.unwind_scopes_to(target_depth);
                let jump = self.emit(Opcode::Jump(0));
                self.add_break_jump(jump);
                Ok(())
            }
            Stmt::Continue { .. } => {
                if self.loop_contexts.is_empty() {
                    return Err(CompilerError::new("continue outside of loop"));
                }
                let target_depth = self.loop_contexts.last().unwrap().scope_depth;
                self.unwind_scopes_to(target_depth);
                let jump = self.emit(Opcode::Jump(0));
                self.add_continue_jump(jump);
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

                // Compile each case
                for case in cases {
                    // Duplicate the switch value
                    self.emit(Opcode::Dup);

                    // Compile case value
                    self.compile_expression(&case.value)?;

                    // Compare
                    self.emit(Opcode::Eq);

                    // Jump if not equal
                    let jump_if_false = self.emit(Opcode::JumpIfFalse(0));

                    // Pop the switch value
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

                    self.patch_jump(jump_if_false);
                }

                // Default case
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

                self.push_scope();
                for stmt in body {
                    self.compile_statement(stmt)?;
                }
                self.pop_scope();
                self.emit(Opcode::TryPop);
                // Success path: skip catch, go to finally (or after)
                let jump_skip_catch = self.emit(Opcode::Jump(0));

                let mut catch_pc = None;
                if has_catch {
                    let cp = self.instructions.len();
                    catch_pc = Some(cp);
                    self.push_scope();
                    // VM pushes error value on stack before entering catch
                    let param = catch_param.as_ref().unwrap();
                    self.declare_variable(param, false)?;
                    self.emit(Opcode::StoreLocal(param.clone()));
                    for stmt in catch_body.as_ref().unwrap() {
                        self.compile_statement(stmt)?;
                    }
                    self.pop_scope();
                    self.emit(Opcode::TryPop);
                }
                let jump_to_finally = if has_catch {
                    Some(self.emit(Opcode::Jump(0)))
                } else {
                    None
                };

                let finally_pc = if has_finally {
                    Some(self.instructions.len())
                } else {
                    None
                };

                if has_finally {
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

                self.instructions[push_idx] = Opcode::TryPush {
                    catch: catch_pc,
                    finally: finally_pc,
                };

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
            Expr::Identifier(name, _) => {
                let res = self.resolve_for_load(name);
                match res {
                    VarRes::Captured | VarRes::ConstCaptured => {
                        self.track_captured(name, res);
                        self.emit(Opcode::LoadCaptured(name.clone()));
                    }
                    VarRes::Local | VarRes::ConstLocal => {
                        self.emit(Opcode::LoadLocal(name.clone()));
                    }
                    VarRes::Global | VarRes::ConstGlobal => {
                        self.emit(Opcode::LoadGlobal(name.clone()));
                    }
                }
                Ok(())
            }
            Expr::Unary { op, operand, .. } => {
                self.compile_expression(operand)?;
                match op {
                    UnaryOp::Negate => self.emit(Opcode::Neg),
                    UnaryOp::Plus => self.emit(Opcode::Pos),
                    UnaryOp::Not => self.emit(Opcode::Not),
                };
                Ok(())
            }
            Expr::Binary { op, left, right, .. } => {
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
                    BinaryOp::Add => self.emit(Opcode::Add),
                    BinaryOp::Subtract => self.emit(Opcode::Sub),
                    BinaryOp::Multiply => self.emit(Opcode::Mul),
                    BinaryOp::Divide => self.emit(Opcode::Div),
                    BinaryOp::Modulo => self.emit(Opcode::Mod),
                    BinaryOp::Power => self.emit(Opcode::Pow),
                    BinaryOp::Greater => self.emit(Opcode::Gt),
                    BinaryOp::Less => self.emit(Opcode::Lt),
                    BinaryOp::GreaterEqual => self.emit(Opcode::Gte),
                    BinaryOp::LessEqual => self.emit(Opcode::Lte),
                    BinaryOp::Equal => self.emit(Opcode::Eq),
                    BinaryOp::NotEqual => self.emit(Opcode::Neq),
                    BinaryOp::RawEqual => self.emit(Opcode::RawEq),
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
                            VarRes::Captured | VarRes::ConstCaptured => {
                                self.track_captured(name, res);
                                self.emit(Opcode::StoreCaptured(name.clone()));
                            }
                            VarRes::Local | VarRes::ConstLocal => {
                                self.emit(Opcode::StoreLocal(name.clone()));
                            }
                            VarRes::Global | VarRes::ConstGlobal => {
                                self.emit(Opcode::StoreGlobal(name.clone()));
                            }
                        }
                        if create_local {
                            self.register_implicit_local(name);
                        }
                    }
                    Expr::Member { object, property, .. } => {
                        self.compile_expression(object)?;
                        self.compile_expression(value)?;
                        self.emit(Opcode::SetProperty(property.clone()));
                    }
                    Expr::Index { object, index, .. } => {
                        self.compile_expression(object)?;
                        self.compile_expression(index)?;
                        self.compile_expression(value)?;
                        self.emit(Opcode::SetIndex);
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
                                self.emit(Opcode::LoadCaptured(name.clone()));
                            }
                            VarRes::Local | VarRes::ConstLocal => {
                                self.emit(Opcode::LoadLocal(name.clone()));
                            }
                            VarRes::Global | VarRes::ConstGlobal => {
                                self.emit(Opcode::LoadGlobal(name.clone()));
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
                            VarRes::Captured | VarRes::ConstCaptured => {
                                self.emit(Opcode::StoreCaptured(name.clone()));
                            }
                            VarRes::Local | VarRes::ConstLocal => {
                                self.emit(Opcode::StoreLocal(name.clone()));
                            }
                            VarRes::Global | VarRes::ConstGlobal => {
                                self.emit(Opcode::StoreGlobal(name.clone()));
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
                        self.emit(Opcode::GetProperty(property.clone()));
                        self.compile_expression(value)?;
                        match op {
                            CompoundAssignOp::Add => self.emit(Opcode::Add),
                            CompoundAssignOp::Subtract => self.emit(Opcode::Sub),
                            CompoundAssignOp::Multiply => self.emit(Opcode::Mul),
                            CompoundAssignOp::Divide => self.emit(Opcode::Div),
                            CompoundAssignOp::Modulo => self.emit(Opcode::Mod),
                        };
                        self.emit(Opcode::SetProperty(property.clone()));
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
                                self.emit(Opcode::LoadCaptured(name.clone()));
                            }
                            VarRes::Local | VarRes::ConstLocal => {
                                self.emit(Opcode::LoadLocal(name.clone()));
                            }
                            VarRes::Global | VarRes::ConstGlobal => {
                                self.emit(Opcode::LoadGlobal(name.clone()));
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
                            VarRes::Captured | VarRes::ConstCaptured => {
                                self.emit(Opcode::StoreCaptured(name.clone()));
                            }
                            VarRes::Local | VarRes::ConstLocal => {
                                self.emit(Opcode::StoreLocal(name.clone()));
                            }
                            VarRes::Global | VarRes::ConstGlobal => {
                                self.emit(Opcode::StoreGlobal(name.clone()));
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
            Expr::Call { callee, args, .. } => {
                // Statically resolvable builtin call: no callee is pushed, the
                // operation name travels in the operand (§145, §146).
                if let Some(target) = self.static_call_target(callee) {
                    for arg in args {
                        self.compile_expression(arg)?;
                    }
                    match target {
                        StaticCallTarget::Native(operation) => {
                            self.emit(Opcode::CallNative(operation, args.len()))
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
                self.emit(Opcode::Call(args.len()));

                Ok(())
            }
            Expr::Member { object, property, .. } => {
                self.compile_expression(object)?;
                self.emit(Opcode::GetProperty(property.clone()));
                Ok(())
            }
            Expr::Index { object, index, .. } => {
                self.compile_expression(object)?;
                self.compile_expression(index)?;
                self.emit(Opcode::GetIndex);
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
                let keys: Vec<String> = properties.iter().map(|p| p.key.clone()).collect();
                for prop in properties {
                    self.compile_expression(&prop.value)?;
                }
                self.emit(Opcode::MakeObject(keys));
                Ok(())
            }
            Expr::Function { params, body, .. } => {
                // Create anonymous function
                let func_idx = self.functions.len();

                // Compile default parameter values into current constants
                let mut param_defaults = Vec::new();
                for param in params {
                    if let Some(ref default_expr) = param.default_value {
                        self.compile_expression(default_expr)?;
                        let default_idx = self.constants.len() - 1;
                        param_defaults.push(Some(default_idx));
                        self.emit(Opcode::Pop);
                    } else {
                        param_defaults.push(None);
                    }
                }

                self.functions.push(FunctionInfo {
                    name: String::new(),
                    params: params.iter().map(|p| p.name.clone()).collect(),
                    param_defaults,
                    entry_point: 0,
                    local_count: 0,
                    captured_vars: Vec::new(),
                    is_async: false,
                });

                // Save and start new compilation context (keep parent scopes for capture detection)
                let saved_instructions = std::mem::take(&mut self.instructions);
                let saved_constants = std::mem::take(&mut self.constants);
                let saved_captured = std::mem::take(&mut self.captured_vars);
                let saved_scope_depth = self.scope_depth;
                let saved_scopes = std::mem::take(&mut self.scopes);
                let saved_function_base = self.function_base;
                let saved_bodies_count = self.function_bodies.len();
                let saved_loop_contexts = std::mem::take(&mut self.loop_contexts);
                let saved_in_async = self.in_async;
                // Nested function/arrow expressions are never async (§49: no
                // async function expressions), so `await` inside them is an error.
                self.in_async = false;

                self.scopes = saved_scopes.clone();
                self.scopes.push(Vec::new());
                self.scope_depth += 1;
                self.function_base = self.scopes.len() - 1;
                for param in params {
                    self.declare_variable(&param.name, false)?;
                }

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
                let func_constants = std::mem::take(&mut self.constants);

                // Collect and remap any inner function bodies
                let mut inner_bodies: Vec<Vec<Opcode>> = self.function_bodies.drain(saved_bodies_count..).collect();

                self.scopes = saved_scopes;
                self.instructions = saved_instructions;
                self.constants = saved_constants;
                self.captured_vars = saved_captured;
                self.scope_depth = saved_scope_depth;
                self.function_base = saved_function_base;
                self.loop_contexts = saved_loop_contexts;
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

                self.functions[func_idx].entry_point = entry_point;
                self.functions[func_idx].local_count = local_count;
                self.functions[func_idx].captured_vars = captured.iter().map(|(n, _)| n.clone()).collect();

                // Store function reference
                // Function metadata stays in the constant pool (§126) — index
                // based consumers such as param defaults keep their indices —
                // but the value itself is pushed by LOAD_FUNCTION (§141).
                self.add_constant(Constant::Function(func_idx));
                self.emit(Opcode::LoadFunction(func_idx));

                if !captured.is_empty() {
                    self.emit(Opcode::MakeClosure(func_idx, captured));
                }

                // Store function body for VM execution
                self.function_bodies.push(func_instructions);
                for body in inner_bodies {
                    self.function_bodies.push(body);
                }

                Ok(())
            }
            Expr::ArrowFunction { params, body, .. } => {
                // Similar to function expression
                let func_idx = self.functions.len();

                // Compile default parameter values into current constants
                let mut param_defaults = Vec::new();
                for param in params {
                    if let Some(ref default_expr) = param.default_value {
                        self.compile_expression(default_expr)?;
                        let default_idx = self.constants.len() - 1;
                        param_defaults.push(Some(default_idx));
                        self.emit(Opcode::Pop);
                    } else {
                        param_defaults.push(None);
                    }
                }

                self.functions.push(FunctionInfo {
                    name: String::new(),
                    params: params.iter().map(|p| p.name.clone()).collect(),
                    param_defaults,
                    entry_point: 0,
                    local_count: 0,
                    captured_vars: Vec::new(),
                    is_async: false,
                });

                let saved_instructions = std::mem::take(&mut self.instructions);
                let saved_constants = std::mem::take(&mut self.constants);
                let saved_captured = std::mem::take(&mut self.captured_vars);
                let saved_scope_depth = self.scope_depth;
                let saved_scopes = std::mem::take(&mut self.scopes);
                let saved_function_base = self.function_base;
                let saved_bodies_count = self.function_bodies.len();
                let saved_loop_contexts = std::mem::take(&mut self.loop_contexts);
                let saved_in_async = self.in_async;
                // Nested function/arrow expressions are never async (§49: no
                // async function expressions), so `await` inside them is an error.
                self.in_async = false;

                self.scopes = saved_scopes.clone();
                self.scopes.push(Vec::new());
                self.scope_depth += 1;
                self.function_base = self.scopes.len() - 1;

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
                let func_constants = std::mem::take(&mut self.constants);

                // Collect and remap any inner function bodies
                let mut inner_bodies: Vec<Vec<Opcode>> = self.function_bodies.drain(saved_bodies_count..).collect();

                self.scopes = saved_scopes;
                self.instructions = saved_instructions;
                self.constants = saved_constants;
                self.captured_vars = saved_captured;
                self.scope_depth = saved_scope_depth;
                self.function_base = saved_function_base;
                self.loop_contexts = saved_loop_contexts;
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

                self.functions[func_idx].entry_point = entry_point;
                self.functions[func_idx].local_count = local_count;
                self.functions[func_idx].captured_vars = captured.iter().map(|(n, _)| n.clone()).collect();

                // Function metadata stays in the constant pool (§126) — index
                // based consumers such as param defaults keep their indices —
                // but the value itself is pushed by LOAD_FUNCTION (§141).
                self.add_constant(Constant::Function(func_idx));
                self.emit(Opcode::LoadFunction(func_idx));

                if !captured.is_empty() {
                    self.emit(Opcode::MakeClosure(func_idx, captured));
                }

                // Store function body for VM execution
                self.function_bodies.push(func_instructions);
                for body in inner_bodies {
                    self.function_bodies.push(body);
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

pub fn compile(program: &Program) -> Result<BytecodeProgram, CompilerError> {
    let mut compiler = Compiler::new();
    compiler.compile_program(program)?;

    Ok(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
        constants: compiler.constants,
        instructions: compiler.instructions,
        functions: compiler.functions,
        function_bodies: compiler.function_bodies,
    })
}

/// Compile a single file as a module (module-mode scopes, EndModule terminator).
/// Function bodies stay in `function_bodies` for the loader to merge across modules.
pub fn compile_module(path: &str, program: &Program) -> Result<ModuleUnit, CompilerError> {
    let mut compiler = Compiler::new_module();
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

        for func in &unit.functions {
            let mut f = func.clone();
            // entry_point is fixed after all bodies are appended below.
            f.entry_point = 0;
            for d in f.param_defaults.iter_mut().flatten() {
                *d += const_offset;
            }
            combined.functions.push(f);
        }

        for body in &unit.function_bodies {
            let mut remapped: Vec<Opcode> = body
                .iter()
                .map(|instr| remap_body_instr(instr, const_offset, func_offset))
                .collect();
            combined.function_bodies.push(std::mem::take(&mut remapped));
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
        assert!(bc.instructions.contains(&Opcode::DeclareGlobal("A".to_string(), false)));
        assert!(bc.instructions.contains(&Opcode::StoreGlobal("A".to_string())));
    }

    #[test]
    fn test_const_declaration() {
        let bc = compile_source("const A = 10");
        assert!(bc.instructions.contains(&Opcode::DeclareGlobal("A".to_string(), true)));
    }

    #[test]
    fn test_identifier_access() {
        let bc = compile_source("let A = 10\nA");
        // Should have LoadGlobal for A
        assert!(bc.instructions.iter().any(|op| matches!(op, Opcode::LoadGlobal(name) if name == "A")));
    }

    #[test]
    fn test_assignment() {
        let bc = compile_source("let A = 10\nA = 20");
        assert!(bc.instructions.contains(&Opcode::StoreGlobal("A".to_string())));
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
        assert!(bc.instructions.contains(&Opcode::GetProperty("name".to_string())));
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
        assert!(bc.instructions.contains(&Opcode::StoreGlobal("A".to_string())));
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
        assert!(bc.instructions.contains(&Opcode::DeclareGlobal("A".to_string(), false)));
        assert!(bc.instructions.contains(&Opcode::StoreGlobal("A".to_string())));
        assert!(bc.instructions.contains(&Opcode::LoadGlobal("A".to_string())));
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
            .contains(&Opcode::CallNative("print".to_string(), 1)));
        assert!(!bc.instructions.iter().any(|op| matches!(op, Opcode::Call(_))));
    }

    #[test]
    fn test_job019_call_native_for_conversions_and_type() {
        let bc = compile_source("type(1)\nnumber(\"2\")\nstring(1)\nboolean(1)\nprint.err(\"e\")");
        for operation in ["type", "number", "string", "boolean", "print.err"] {
            assert!(
                bc.instructions
                    .iter()
                    .any(|op| matches!(op, Opcode::CallNative(name, _) if name.as_str() == operation)),
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
                    .any(|op| matches!(op, Opcode::CallNative(name, _) if name.as_str() == operation)),
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
                if matches!(name.as_str(), "list.add" | "list.remove" | "object.remove"))
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
        assert!(bc.instructions.contains(&Opcode::GetProperty("bogus".to_string())));
        assert!(bc.instructions.contains(&Opcode::Call(1)));
        assert!(!bc.instructions.iter().any(|op| matches!(op, Opcode::CallNative(..))));
    }

    #[test]
    fn test_job019_shadowed_builtin_parameter_keeps_dynamic_call() {
        let bc = compile_source("function f(print) { print(7) }");
        let body = &bc.function_bodies[0];
        assert!(body
            .iter()
            .any(|op| matches!(op, Opcode::LoadLocal(name) if name.as_str() == "print")));
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
        assert!(bc.instructions.contains(&Opcode::GetProperty("length".to_string())));
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
}
