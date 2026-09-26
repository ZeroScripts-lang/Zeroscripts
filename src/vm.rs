use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::compiler::{BytecodeProgram, Constant, FunctionInfo, Opcode};
use crate::gc::Gc;
use crate::heap::{EnvBinding, Heap, HeapObject, HeapRef as HeapRefHandle};
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
            Value::Closure { function_index, env, param_defaults } => {
                let env_bindings = match heap.get(env) {
                    Ok(HeapObject::Environment(bindings)) => bindings.clone(),
                    _ => Vec::new(),
                };
                let env_ref = heap.allocate(HeapObject::Environment(env_bindings));
                let r = heap.allocate(HeapObject::Closure {
                    function_index: *function_index,
                    env: env_ref,
                    param_defaults: param_defaults.clone(),
                });
                RuntimeValue::HeapRef(r)
            }
            Value::Task(id) => RuntimeValue::HeapRef(heap.allocate(HeapObject::Task(*id))),
        }
    }

    fn heap_object_for_element(&self, heap: &mut Heap) -> HeapRefHandle {
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
            Value::Closure { function_index, env, param_defaults } => {
                let env_bindings = match heap.get(env) {
                    Ok(HeapObject::Environment(bindings)) => bindings.clone(),
                    _ => Vec::new(),
                };
                let env_ref = heap.allocate(HeapObject::Environment(env_bindings));
                heap.allocate(HeapObject::Closure {
                    function_index: *function_index,
                    env: env_ref,
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

    fn from_heap_ref(r: &HeapRefHandle, heap: &Heap) -> Result<Self, RuntimeError> {
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

#[derive(Debug, Clone, PartialEq)]
pub enum VmError {
    InvalidBytecode(String),
    InvalidOpcode(String),
    InvalidOperand(String),
    InvalidConstantIndex(usize),
    InvalidJumpTarget(usize),
    StackUnderflow(String),
    InvalidRuntimeOperation(String),
    InvalidExecutionState(String),
    RuntimeError(String),
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
            VmError::RuntimeError(msg) => write!(f, "Runtime error: {}", msg),
        }
    }
}

impl Error for VmError {}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionState {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
struct Binding {
    value: Value,
    is_const: bool,
    initialized: bool,
    /// Live binding (§66/§74): (source module env, export name).
    /// When set, loads read through the heap Environment so mutations
    /// by the source module are visible; stores are rejected via is_const.
    live: Option<(HeapRefHandle, String)>,
}

impl Binding {
    fn new(value: Value, is_const: bool, initialized: bool) -> Self {
        Self {
            value,
            is_const,
            initialized,
            live: None,
        }
    }
}

/// Export cache entry after a module finishes init (§66, §67).
/// `Live` points at the source module's shared Environment (same storage
/// closures use via StoreCaptured) so importers observe later mutations.
/// `Value` is a snapshot — safe for bindings no closure can mutate after init.
#[derive(Debug, Clone)]
enum ExportBinding {
    Value(Value),
    Live(HeapRefHandle, String),
}

#[derive(Debug, Clone)]
struct Frame {
    function_index: usize,
    scopes: Vec<HashMap<String, Binding>>,
    return_pc: usize,
    captured_env: Option<HeapRefHandle>,
    scope_env: Option<HeapRefHandle>,
    return_instructions: Option<Vec<Opcode>>,
    /// Set when this frame belongs to an async task's own context (§101).
    task_id: Option<usize>,
}

/// Task lifecycle states (§106). Terminal states: Completed, Failed, Cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl TaskState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskState::Pending => "pending",
            TaskState::Running => "running",
            TaskState::Completed => "completed",
            TaskState::Failed => "failed",
            TaskState::Cancelled => "cancelled",
        }
    }

    fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskState::Completed | TaskState::Failed | TaskState::Cancelled
        )
    }
}

/// What kind of task this is. `Normal` tasks execute an async function body;
/// the others are synthetic tasks created by `task.*` builtins and driven by
/// the scheduler from events and timers (§103-§105, §109-§111).
#[derive(Debug, Clone)]
enum TaskKind {
    Normal {
        func_idx: usize,
        args: Vec<Value>,
        callee: Value,
        env: Option<HeapRefHandle>,
    },
    /// `task.all` inputs — completes with results in input order (§103).
    All(Vec<usize>),
    /// `task.race` inputs — first input to reach terminal wins (§104).
    Race(Vec<usize>),
    /// `task.wait` target — completes with Null, propagates failure (§109).
    Wait(usize),
    /// `task.sleep` deadline (§111).
    Sleep { deadline_ms: u128 },
    /// `task.timeout` target + deadline (§110).
    Timeout {
        target: usize,
        deadline_ms: u128,
    },
    /// In-flight HTTP request (§50). The actual network I/O runs on a
    /// worker thread; the scheduler polls the shared slot and finishes the
    /// task when the result lands (§112 non-blocking rule).
    Http { slot: HttpSlot },
}

/// Result slot shared between an HTTP worker thread and the scheduler.
/// Written exactly once by the worker; polled (and taken) on the VM thread
/// so all task state transitions stay single-threaded.
type HttpSlot = Arc<Mutex<Option<Result<HttpOutcome, String>>>>;

/// A completed HTTP exchange as parsed by the client (§50 Response).
#[derive(Debug, Clone)]
struct HttpOutcome {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

/// A suspended task's execution context: everything the linear instruction
/// loop operates on, swapped in/out of `ZeroVM`'s active fields by the
/// cooperative scheduler (§112).
#[derive(Debug, Clone)]
struct TaskContext {
    pc: usize,
    instructions: Vec<Opcode>,
    stack: Vec<Value>,
    call_stack: Vec<Frame>,
    handlers: Vec<TryHandler>,
    pending_error: Option<String>,
    pending_return: Option<Value>,
}

/// A single asynchronous task (§101). Stored in `ZeroVM.tasks` (outside the
/// heap); heap-backed values are rooted via `live_refs` so GC keeps them
/// alive while the task exists.
#[derive(Debug, Clone)]
struct Task {
    state: TaskState,
    kind: TaskKind,
    /// Stored result of a completed task (§107).
    result: Option<Value>,
    /// Stored error of a failed task (§108).
    error: Option<String>,
    /// Saved execution context while the task is suspended; None for tasks
    /// that have not started yet or have reached a terminal state.
    context: Option<TaskContext>,
    /// Id of the task this task is suspended on (`await ...`). Combinators
    /// are themselves tasks, so every suspension is a wait on one task id.
    wait: Option<usize>,
    /// Order in which this task reached a terminal state — `task.race`
    /// compares it to find the genuine first finisher (§104).
    terminal_seq: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
enum TryPhase {
    TryBody,
    Catch,
}

#[derive(Debug, Clone)]
struct TryHandler {
    catch_pc: Option<usize>,
    finally_pc: Option<usize>,
    scope_depth: usize,
    stack_depth: usize,
    frame_depth: usize,
    phase: TryPhase,
}

pub struct ZeroVM {
    program: BytecodeProgram,
    pc: usize,
    stack: Vec<Value>,
    call_stack: Vec<Frame>,
    state: ExecutionState,
    output: Vec<String>,
    err_output: Vec<String>,
    heap: Heap,
    gc: Gc,
    live_refs: Vec<HeapRefHandle>,
    handlers: Vec<TryHandler>,
    pending_error: Option<String>,
    pending_return: Option<Value>,
    /// CLI arguments for system.args() (§52) — host strips program/script names.
    program_args: Vec<String>,
    /// All created tasks (§101). Index is the `Value::Task` handle.
    tasks: Vec<Task>,
    /// FIFO ready queue of normal task ids — round-robin scheduler order
    /// (§112 lets the scheduler choose; FIFO keeps it deterministic).
    ready_queue: VecDeque<usize>,
    /// Id of the task whose context is currently loaded into the instruction
    /// loop fields (pc/instructions/stack/call_stack/handlers).
    active_task: Option<usize>,
    /// Virtual monotonic clock (milliseconds) for sleep/timeout (§110, §111).
    /// Advanced to the earliest deadline only when nothing is runnable; the
    /// spec exposes no wall-clock API, so observable order is unaffected.
    now_ms: u128,
    /// Monotonic counter stamped onto tasks when they reach a terminal state.
    next_terminal_seq: u64,
    /// Top-level/global bindings (builtins + `let`/`const` at program scope).
    /// Lives outside every execution context so task contexts (§112), which
    /// do not contain the root frame, still resolve globals identically.
    global_scope: HashMap<String, Binding>,
}

impl ZeroVM {
    pub fn new(program: BytecodeProgram) -> Self {
        Self {
            program,
            pc: 0,
            stack: Vec::new(),
            call_stack: Vec::new(),
            state: ExecutionState::Running,
            output: Vec::new(),
            err_output: Vec::new(),
            heap: Heap::new(),
            gc: Gc::new(128),
            live_refs: Vec::new(),
            handlers: Vec::new(),
            pending_error: None,
            pending_return: None,
            program_args: Vec::new(),
            tasks: Vec::new(),
            ready_queue: VecDeque::new(),
            active_task: None,
            now_ms: 0,
            next_terminal_seq: 0,
            global_scope: HashMap::new(),
        }
    }

    /// Set program arguments for system.args(). Per §52 the host is
    /// responsible for excluding the interpreter and script file names
    /// (e.g. `zerovm app.zs hello world` → `["hello", "world"]`).
    pub fn set_program_args(&mut self, args: Vec<String>) {
        self.program_args = args;
    }

    pub fn heap(&self) -> &Heap {
        &self.heap
    }

    pub fn heap_mut(&mut self) -> &mut Heap {
        &mut self.heap
    }

    pub fn gc(&self) -> &Gc {
        &self.gc
    }

    pub fn gc_mut(&mut self) -> &mut Gc {
        &mut self.gc
    }

    pub fn live_refs(&self) -> &[HeapRefHandle] {
        &self.live_refs
    }

    pub fn add_live_ref(&mut self, reference: HeapRefHandle) {
        if !self.live_refs.iter().any(|r| r.index() == reference.index() && r.generation() == reference.generation()) {
            self.live_refs.push(reference);
        }
    }

    pub fn remove_live_ref(&mut self, reference: &HeapRefHandle) {
        self.live_refs.retain(|r| !(r.index() == reference.index() && r.generation() == reference.generation()));
    }

    pub fn maybe_gc(&mut self) {
        if self.gc.should_collect() {
            self.run_gc();
        }
    }

    pub fn run_gc(&mut self) {
        let roots: Vec<HeapRefHandle> = self.live_refs.clone();
        self.gc.collect(&mut self.heap, &roots);

        self.live_refs.retain(|r| self.heap.is_valid(r));
    }

    pub fn heap_alloc(&mut self, object: HeapObject) -> HeapRefHandle {
        let reference = self.heap.allocate(object);
        self.gc.on_allocation();
        self.add_live_ref(reference.clone());
        self.maybe_gc();
        reference
    }

    fn track_stack_top(&mut self) {
        if let Some(value) = self.stack.last().cloned() {
            self.track_value_heap_refs(&value);
        }
    }

    fn track_value_heap_refs(&mut self, value: &Value) {
        match value {
            Value::List(r) | Value::Object(r) => {
                self.add_live_ref(r.clone());
            }
            Value::Closure { env, .. } => {
                self.add_live_ref(env.clone());
            }
            _ => {}
        }
    }

    pub fn validate(&self) -> Result<(), VmError> {
        if self.program.instructions.is_empty() {
            return Ok(());
        }

        for (_i, instruction) in self.program.instructions.iter().enumerate() {
            match instruction {
                Opcode::LoadConst(idx) => {
                    if *idx >= self.program.constants.len() {
                        return Err(VmError::InvalidConstantIndex(*idx));
                    }
                }
                Opcode::Jump(target)
                | Opcode::JumpIfTrue(target)
                | Opcode::JumpIfFalse(target) => {
                    if *target >= self.program.instructions.len() {
                        return Err(VmError::InvalidJumpTarget(*target));
                    }
                }
                Opcode::TryPush { catch, finally } => {
                    if let Some(t) = catch {
                        if *t >= self.program.instructions.len() {
                            return Err(VmError::InvalidJumpTarget(*t));
                        }
                    }
                    if let Some(t) = finally {
                        if *t >= self.program.instructions.len() {
                            return Err(VmError::InvalidJumpTarget(*t));
                        }
                    }
                }
                Opcode::MakeObject(keys) => {
                    for key in keys {
                        if key.is_empty() {
                            return Err(VmError::InvalidOperand(
                                "Empty object key".to_string(),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }

        for func in &self.program.functions {
            if func.entry_point >= self.program.instructions.len() {
                return Err(VmError::InvalidJumpTarget(func.entry_point));
            }
        }

        Ok(())
    }

    /// Install built-in bindings into the shared global scope (§42-§53).
    fn install_builtins(&mut self) {
        for name in [
            "print",
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
        ] {
            self.global_scope.insert(
                name.to_string(),
                Binding {
                    value: Value::BuiltIn(name.to_string()),
                    is_const: true,
                    initialized: true,
                    live: None,
                },
            );
        }
    }

    pub fn run(&mut self) -> Result<(), VmError> {
        self.validate()?;
        self.state = ExecutionState::Running;

        // Globals live in `global_scope` (shared across task contexts); the
        // root frame only hosts the top-level lexical scope stack.
        self.global_scope.clear();
        self.install_builtins();
        self.call_stack.push(Frame {
            function_index: 0,
            scopes: vec![HashMap::new()],
            return_pc: 0,
            captured_env: None,
            scope_env: None,
            return_instructions: None,
            task_id: None,
        });

        while self.state == ExecutionState::Running {
            if self.pc >= self.program.instructions.len() {
                self.state = ExecutionState::Completed;
                break;
            }

            let instruction = self.program.instructions[self.pc].clone();
            self.pc += 1;

            if let Err(err) = self.execute_instruction(&instruction) {
                // Only ZeroScript runtime errors are catchable (§43, §154)
                if let VmError::RuntimeError(msg) = err {
                    self.raise_runtime_error(msg)?;
                } else {
                    return Err(err);
                }
            }
        }

        // Cooperative scheduler (§112): once the top level is done, run every
        // task round-robin until all reach a terminal state (§106).
        self.run_scheduler()?;

        Ok(())
    }

    pub fn get_output(&self) -> &[String] {
        &self.output
    }

    /// Run a module bundle: each module's range executes after its deps
    /// (already in `modules` topo order). Imports are pre-bound from the
    /// export cache; exports are snapshotted after each module's EndModule.
    /// Failed modules are not cached (§73).
    pub fn run_modules(
        &mut self,
        modules: &[crate::module::ModuleMeta],
    ) -> Result<(), VmError> {
        self.validate()?;
        self.state = ExecutionState::Running;

        self.global_scope.clear();
        self.install_builtins();
        self.call_stack.push(Frame {
            function_index: 0,
            scopes: vec![HashMap::new()],
            return_pc: 0,
            captured_env: None,
            scope_env: None,
            return_instructions: None,
            task_id: None,
        });

        let mut cache: HashMap<String, HashMap<String, ExportBinding>> = HashMap::new();

        for meta in modules {
            let mut module_scope = HashMap::new();
            for (local, dep_path, export_name) in &meta.imports {
                let eb = cache
                    .get(dep_path)
                    .and_then(|m| m.get(export_name))
                    .ok_or_else(|| {
                        VmError::RuntimeError(format!(
                            "Export '{}' not found in {}",
                            export_name, dep_path
                        ))
                    })?;
                let binding = match eb {
                    ExportBinding::Value(v) => Binding::new(v.clone(), true, true),
                    ExportBinding::Live(env, name) => {
                        // Initial value for binding.value; loads go through live.
                        let init = self
                            .heap
                            .get_env_binding(env, name)
                            .ok()
                            .flatten()
                            .map(|hr| Value::from_heap_ref(&hr, &self.heap))
                            .and_then(|r| r.ok())
                            .unwrap_or(Value::Null);
                        Binding {
                            value: init,
                            is_const: true,
                            initialized: true,
                            live: Some((env.clone(), name.clone())),
                        }
                    }
                };
                module_scope.insert(local.clone(), binding);
            }

            {
                let frame = self
                    .call_stack
                    .last_mut()
                    .ok_or_else(|| VmError::InvalidExecutionState("No root frame".into()))?;
                frame.scopes.push(module_scope);
                frame.scope_env = None;
            }

            let run_result = self.run_module_range(meta.start, meta.end);
            if let Err(e) = run_result {
                // §73: do not cache failed module; drop its scope.
                if let Some(frame) = self.call_stack.last_mut() {
                    if frame.scopes.len() > 1 {
                        frame.scopes.pop();
                    }
                    frame.scope_env = None;
                }
                self.handlers.clear();
                return Err(e);
            }

            // Capture shared closure env before clearing (live binding root).
            let scope_env = {
                let frame = self
                    .call_stack
                    .last()
                    .ok_or_else(|| VmError::InvalidExecutionState("No root frame".into()))?;
                frame.scope_env.clone()
            };

            let frame = self
                .call_stack
                .last_mut()
                .ok_or_else(|| VmError::InvalidExecutionState("No root frame".into()))?;
            let module_scope = frame.scopes.pop().ok_or_else(|| {
                VmError::InvalidExecutionState("Missing module scope".into())
            })?;
            frame.scope_env = None;

            let mut exports_map = HashMap::new();
            for name in &meta.exports {
                // Re-export: copy the source binding (Live ref chains, §67).
                if let Some((_, dep_path)) = meta.re_exports.iter().find(|(n, _)| n == name) {
                    let src = cache
                        .get(dep_path)
                        .and_then(|m| m.get(name))
                        .ok_or_else(|| {
                            VmError::RuntimeError(format!(
                                "Re-export '{}' not found in {}",
                                name, dep_path
                            ))
                        })?;
                    exports_map.insert(name.clone(), src.clone());
                    continue;
                }

                // Prefer live ref into the module's shared Environment when the
                // binding was captured by a closure (mutations go there, §66).
                if let Some(env) = &scope_env {
                    if let Ok(Some(_)) = self.heap.get_env_binding(env, name) {
                        self.add_live_ref(env.clone());
                        exports_map.insert(
                            name.clone(),
                            ExportBinding::Live(env.clone(), name.clone()),
                        );
                        continue;
                    }
                }

                match module_scope.get(name) {
                    Some(b) => {
                        exports_map.insert(name.clone(), ExportBinding::Value(b.value.clone()));
                    }
                    None => {
                        return Err(VmError::RuntimeError(format!(
                            "Export '{}' not available after module initialization",
                            name
                        )));
                    }
                }
            }
            cache.insert(meta.path.clone(), exports_map);
        }

        // Run tasks created during module initialization (§112 scheduler).
        self.run_scheduler()?;

        self.state = ExecutionState::Completed;
        Ok(())
    }

    fn run_module_range(&mut self, start: usize, end: usize) -> Result<(), VmError> {
        self.pc = start;
        while self.pc < end {
            if self.pc >= self.program.instructions.len() {
                break;
            }
            let instruction = self.program.instructions[self.pc].clone();
            self.pc += 1;
            if matches!(instruction, Opcode::EndModule) {
                break;
            }
            if let Err(err) = self.execute_instruction(&instruction) {
                if let VmError::RuntimeError(msg) = err {
                    self.raise_runtime_error(msg)?;
                } else {
                    return Err(err);
                }
            }
        }
        Ok(())
    }

    pub fn get_error_output(&self) -> &[String] {
        &self.err_output
    }

    pub fn get_state(&self) -> &ExecutionState {
        &self.state
    }

    pub fn get_stack(&self) -> &[Value] {
        &self.stack
    }

    fn execute_instruction(&mut self, instruction: &Opcode) -> Result<(), VmError> {
        match instruction {
            Opcode::LoadConst(idx) => self.execute_load_const(*idx),
            Opcode::DeclareLocal(name, is_const) => self.execute_declare_local(name, *is_const),
            Opcode::StoreLocal(name) => self.execute_store_local(name),
            Opcode::LoadLocal(name) => self.execute_load_local(name),
            Opcode::DeclareGlobal(name, is_const) => self.execute_declare_global(name, *is_const),
            Opcode::LoadGlobal(name) => self.execute_load_global(name),
            Opcode::StoreGlobal(name) => self.execute_store_global(name),
            Opcode::PushScope => self.execute_push_scope(),
            Opcode::PopScope => self.execute_pop_scope(),
            Opcode::Add => self.execute_binary_op(|a, b| Self::add_values(a, b)),
            Opcode::Sub => self.execute_binary_op(|a, b| Self::sub_values(a, b)),
            Opcode::Mul => self.execute_binary_op(|a, b| Self::mul_values(a, b)),
            Opcode::Div => self.execute_binary_op(|a, b| Self::div_values(a, b)),
            Opcode::Mod => self.execute_binary_op(|a, b| Self::mod_values(a, b)),
            Opcode::Pow => self.execute_binary_op(|a, b| Self::pow_values(a, b)),
            Opcode::Neg => self.execute_neg(),
            Opcode::Pos => self.execute_pos(),
            Opcode::Not => self.execute_not(),
            Opcode::Eq => self.execute_binary_op(|a, b| Ok(Value::Boolean(a == b))),
            Opcode::Neq => self.execute_binary_op(|a, b| Ok(Value::Boolean(a != b))),
            Opcode::RawEq => self.execute_binary_op(|a, b| Ok(Value::Boolean(raw_eq(a, b)))),
            Opcode::Gt => self.execute_binary_op(|a, b| Self::gt_values(a, b)),
            Opcode::Lt => self.execute_binary_op(|a, b| Self::lt_values(a, b)),
            Opcode::Gte => self.execute_binary_op(|a, b| Self::gte_values(a, b)),
            Opcode::Lte => self.execute_binary_op(|a, b| Self::lte_values(a, b)),
            Opcode::And => self.execute_binary_op(|a, b| Self::and_values(a, b)),
            Opcode::Or => self.execute_binary_op(|a, b| Self::or_values(a, b)),
            Opcode::Pop => self.execute_pop(),
            Opcode::Dup => self.execute_dup(),
            Opcode::Swap => self.execute_swap(),
            Opcode::Jump(target) => self.execute_jump(*target),
            Opcode::JumpIfTrue(target) => self.execute_jump_if_true(*target),
            Opcode::JumpIfFalse(target) => self.execute_jump_if_false(*target),
            Opcode::TryPush { catch, finally } => self.execute_try_push(*catch, *finally),
            Opcode::TryPop => self.execute_try_pop(),
            Opcode::FinallyEnd => self.execute_finally_end(),
            Opcode::Call(argc) => self.execute_call(*argc),
            Opcode::Await => self.execute_await(),
            Opcode::Return => self.execute_return(),
            Opcode::MakeClosure(func_idx, captured) => {
                self.execute_make_closure(*func_idx, captured.as_slice())
            }
            Opcode::LoadCaptured(name) => self.execute_load_captured(name),
            Opcode::StoreCaptured(name) => self.execute_store_captured(name),
            Opcode::MakeList(size) => self.execute_make_list(*size),
            Opcode::MakeObject(keys) => self.execute_make_object(keys),
            Opcode::GetIndex => self.execute_get_index(),
            Opcode::SetIndex => self.execute_set_index(),
            Opcode::GetProperty(name) => self.execute_get_property(name),
            Opcode::SetProperty(name) => self.execute_set_property(name),
            Opcode::Print => self.execute_print(),
            Opcode::Halt => {
                self.state = ExecutionState::Completed;
                Ok(())
            }
            Opcode::EndModule => {
                // Module range end is enforced by run_modules; no-op if hit.
                Ok(())
            }
        }
    }

    fn execute_load_const(&mut self, idx: usize) -> Result<(), VmError> {
        if idx >= self.program.constants.len() {
            return Err(VmError::InvalidConstantIndex(idx));
        }
        let value = self.constant_to_value(&self.program.constants[idx]);
        self.stack.push(value);
        self.track_stack_top();
        Ok(())
    }

    fn constant_to_value(&self, constant: &Constant) -> Value {
        match constant {
            Constant::Number(n) => Value::Number(*n),
            Constant::Str(s) => Value::Str(s.clone()),
            Constant::Boolean(b) => Value::Boolean(*b),
            Constant::Null => Value::Null,
            Constant::Function(idx) => Value::Function(*idx),
        }
    }

    /// Mark a heap-backed collection as const when stored into a const binding.
    fn mark_const_if_collection(&mut self, value: &Value) {
        match value {
            Value::List(r) | Value::Object(r) => {
                if let Ok(obj) = self.heap.get_mut(r) {
                    obj.mark_const_collection();
                }
            }
            _ => {}
        }
    }

    fn execute_declare_local(&mut self, name: &str, is_const: bool) -> Result<(), VmError> {
        let frame = self.call_stack.last_mut().ok_or_else(|| {
            VmError::InvalidExecutionState("No active frame".to_string())
        })?;
        let scope = frame.scopes.last_mut().ok_or_else(|| {
            VmError::InvalidExecutionState("No active scope".to_string())
        })?;
        if scope.contains_key(name) {
            return Err(VmError::RuntimeError(format!(
                "Duplicate declaration of '{}'",
                name
            )));
        }
        scope.insert(
            name.to_string(),
            Binding {
                value: Value::Null,
                is_const,
                initialized: false,
                live: None,
            },
        );
        Ok(())
    }

    fn execute_store_local(&mut self, name: &str) -> Result<(), VmError> {
        let value = self.pop_stack("store local")?;

        // Find binding in current frame's scopes (innermost first)
        let mut found = false;
        let mut was_const = false;
        let mut write_env: Option<HeapRefHandle> = None;
        if let Some(frame) = self.call_stack.last_mut() {
            for scope in frame.scopes.iter_mut().rev() {
                if let Some(binding) = scope.get_mut(name) {
                    if binding.is_const && binding.initialized {
                        return Err(VmError::RuntimeError(format!(
                            "Cannot assign to const '{}'",
                            name
                        )));
                    }
                    was_const = binding.is_const;
                    binding.value = value.clone();
                    binding.initialized = true;
                    found = true;
                    break;
                }
            }
            // Keep shared closure env in sync when this frame owns it (§66).
            if found {
                write_env = frame.scope_env.clone();
            }
            if !found {
                // §4: assignment to unknown name inside a function creates a local
                if let Some(scope) = frame.scopes.last_mut() {
                    scope.insert(
                        name.to_string(),
                        Binding {
                            value: value.clone(),
                            is_const: false,
                            initialized: true,
                            live: None,
                        },
                    );
                    return Ok(());
                }
            }
        }

        if let Some(env) = write_env {
            if let Ok(Some(_)) = self.heap.get_env_binding(&env, name) {
                self.store_in_env(&env, name, value.clone())?;
            }
        }

        if found && was_const {
            self.mark_const_if_collection(&value);
        }
        Ok(())
    }

    fn execute_load_local(&mut self, name: &str) -> Result<(), VmError> {
        if let Some(frame) = self.call_stack.last() {
            for scope in frame.scopes.iter().rev() {
                if let Some(binding) = scope.get(name) {
                    // Live import binding (§66/§74): read through source env.
                    if let Some((env, env_name)) = &binding.live {
                        if let Ok(Some(heap_ref)) = self.heap.get_env_binding(env, env_name) {
                            let value = Value::from_heap_ref(&heap_ref, &self.heap).map_err(|_| {
                                VmError::RuntimeError(format!(
                                    "Failed to convert heap value for '{}'",
                                    name
                                ))
                            })?;
                            self.stack.push(value);
                            self.track_stack_top();
                            return Ok(());
                        }
                    }
                    let value = binding.value.clone();
                    self.stack.push(value);
                    self.track_stack_top();
                    return Ok(());
                }
            }
        }
        Err(VmError::RuntimeError(format!(
            "Undefined variable '{}'",
            name
        )))
    }

    fn execute_declare_global(&mut self, name: &str, is_const: bool) -> Result<(), VmError> {
        let scope = &mut self.global_scope;
        const BUILTINS: &[&str] = &[
            "print",
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
        if scope.contains_key(name) {
            if BUILTINS.contains(&name) {
                return Err(VmError::RuntimeError(format!(
                    "Cannot redeclare built-in '{}'",
                    name
                )));
            }
            return Err(VmError::RuntimeError(format!(
                "Duplicate declaration of '{}'",
                name
            )));
        }
        scope.insert(
            name.to_string(),
            Binding {
                value: Value::Null,
                is_const,
                initialized: false,
                live: None,
            },
        );
        Ok(())
    }

    fn execute_load_global(&mut self, name: &str) -> Result<(), VmError> {
        if let Some(binding) = self.global_scope.get(name) {
            let value = binding.value.clone();
            self.stack.push(value);
            self.track_stack_top();
            return Ok(());
        }
        Err(VmError::RuntimeError(format!(
            "Undefined variable '{}'",
            name
        )))
    }

    fn execute_store_global(&mut self, name: &str) -> Result<(), VmError> {
        let value = self.pop_stack("store global")?;

        let mut was_const = false;
        let mut found = false;
        {
            let scope = &mut self.global_scope;
            {
                if let Some(binding) = scope.get_mut(name) {
                    if binding.is_const && binding.initialized {
                        return Err(VmError::RuntimeError(format!(
                            "Cannot assign to const '{}'",
                            name
                        )));
                    }
                    was_const = binding.is_const;
                    binding.value = value.clone();
                    binding.initialized = true;
                    found = true;
                } else {
                    // Bare assignment creates a mutable global
                    scope.insert(
                        name.to_string(),
                        Binding {
                            value: value.clone(),
                            is_const: false,
                            initialized: true,
                            live: None,
                        },
                    );
                }
            }
        }

        if found && was_const {
            self.mark_const_if_collection(&value);
        }
        Ok(())
    }

    fn execute_push_scope(&mut self) -> Result<(), VmError> {
        if let Some(frame) = self.call_stack.last_mut() {
            frame.scopes.push(HashMap::new());
            Ok(())
        } else {
            Err(VmError::InvalidExecutionState("No active frame".to_string()))
        }
    }

    fn execute_pop_scope(&mut self) -> Result<(), VmError> {
        if let Some(frame) = self.call_stack.last_mut() {
            if frame.scopes.len() > 1 {
                frame.scopes.pop();
                Ok(())
            } else {
                Err(VmError::RuntimeError(
                    "Cannot pop root scope".to_string(),
                ))
            }
        } else {
            Err(VmError::InvalidExecutionState("No active frame".to_string()))
        }
    }

    fn execute_binary_op(
        &mut self,
        op: fn(&Value, &Value) -> Result<Value, VmError>,
    ) -> Result<(), VmError> {
        let right = self.pop_stack("binary op")?;
        let left = self.pop_stack("binary op")?;
        let result = op(&left, &right)?;
        self.stack.push(result);
        self.track_stack_top();
        Ok(())
    }

    fn add_values(a: &Value, b: &Value) -> Result<Value, VmError> {
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

    fn sub_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        let (x, y) = Self::to_numbers(a, b, "subtract")?;
        let result = x - y;
        if !result.is_finite() {
            return Err(VmError::RuntimeError(
                "Arithmetic result is not finite".to_string(),
            ));
        }
        Ok(Value::Number(result))
    }

    fn mul_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        let (x, y) = Self::to_numbers(a, b, "multiply")?;
        let result = x * y;
        if !result.is_finite() {
            return Err(VmError::RuntimeError(
                "Arithmetic result is not finite".to_string(),
            ));
        }
        Ok(Value::Number(result))
    }

    fn div_values(a: &Value, b: &Value) -> Result<Value, VmError> {
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

    fn mod_values(a: &Value, b: &Value) -> Result<Value, VmError> {
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

    fn pow_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        let (x, y) = Self::to_numbers(a, b, "power")?;
        let result = x.powf(y);
        if !result.is_finite() {
            return Err(VmError::RuntimeError(
                "Arithmetic result is not finite".to_string(),
            ));
        }
        Ok(Value::Number(result))
    }

    fn to_numbers(a: &Value, b: &Value, op: &str) -> Result<(f64, f64), VmError> {
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

    fn execute_neg(&mut self) -> Result<(), VmError> {
        let value = self.pop_stack("negate")?;
        match &value {
            Value::Number(n) => {
                let result = -n;
                if !result.is_finite() {
                    return Err(VmError::RuntimeError(
                        "Arithmetic result is not finite".to_string(),
                    ));
                }
                self.stack.push(Value::Number(result));
                Ok(())
            }
            Value::Null => {
                self.stack.push(Value::Number(0.0));
                Ok(())
            }
            _ => Err(VmError::RuntimeError(format!(
                "Cannot negate {:?}",
                value
            ))),
        }
    }

    fn execute_pos(&mut self) -> Result<(), VmError> {
        let value = self.pop_stack("positive")?;
        match &value {
            Value::Number(_) => {
                self.stack.push(value);
                Ok(())
            }
            Value::Null => {
                self.stack.push(Value::Number(0.0));
                Ok(())
            }
            _ => Err(VmError::RuntimeError(format!(
                "Cannot apply unary + to {:?}",
                value
            ))),
        }
    }

    fn execute_not(&mut self) -> Result<(), VmError> {
        let value = self.pop_stack("not")?;
        match &value {
            Value::Boolean(b) => {
                self.stack.push(Value::Boolean(!b));
                Ok(())
            }
            _ => Err(VmError::RuntimeError(format!(
                "Cannot apply ! to {:?}",
                value
            ))),
        }
    }

    fn gt_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        let (x, y) = Self::require_numbers(a, b, "compare")?;
        Ok(Value::Boolean(x > y))
    }

    fn lt_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        let (x, y) = Self::require_numbers(a, b, "compare")?;
        Ok(Value::Boolean(x < y))
    }

    fn gte_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        let (x, y) = Self::require_numbers(a, b, "compare")?;
        Ok(Value::Boolean(x >= y))
    }

    fn lte_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        let (x, y) = Self::require_numbers(a, b, "compare")?;
        Ok(Value::Boolean(x <= y))
    }

    fn require_numbers(a: &Value, b: &Value, op: &str) -> Result<(f64, f64), VmError> {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => Ok((*x, *y)),
            _ => Err(VmError::RuntimeError(format!(
                "Cannot {} {:?} and {:?}",
                op, a, b
            ))),
        }
    }

    fn and_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        match (a, b) {
            (Value::Boolean(x), Value::Boolean(y)) => Ok(Value::Boolean(*x && *y)),
            _ => Err(VmError::RuntimeError(format!(
                "Cannot apply 'and' to {:?} and {:?}",
                a, b
            ))),
        }
    }

    fn or_values(a: &Value, b: &Value) -> Result<Value, VmError> {
        match (a, b) {
            (Value::Boolean(x), Value::Boolean(y)) => Ok(Value::Boolean(*x || *y)),
            _ => Err(VmError::RuntimeError(format!(
                "Cannot apply 'or' to {:?} and {:?}",
                a, b
            ))),
        }
    }

    fn execute_pop(&mut self) -> Result<(), VmError> {
        self.pop_stack("pop")?;
        Ok(())
    }

    fn execute_dup(&mut self) -> Result<(), VmError> {
        let value = self.peek_stack("dup")?;
        self.stack.push(value);
        self.track_stack_top();
        Ok(())
    }

    fn execute_swap(&mut self) -> Result<(), VmError> {
        if self.stack.len() < 2 {
            return Err(VmError::StackUnderflow("swap needs 2 values".to_string()));
        }
        let len = self.stack.len();
        self.stack.swap(len - 1, len - 2);
        Ok(())
    }

    fn execute_jump(&mut self, target: usize) -> Result<(), VmError> {
        if target >= self.program.instructions.len() {
            return Err(VmError::InvalidJumpTarget(target));
        }
        self.pc = target;
        Ok(())
    }

    fn execute_jump_if_true(&mut self, target: usize) -> Result<(), VmError> {
        let value = self.peek_stack("jump if true")?;
        match &value {
            Value::Boolean(true) => {
                if target >= self.program.instructions.len() {
                    return Err(VmError::InvalidJumpTarget(target));
                }
                self.pc = target;
            }
            Value::Boolean(false) => {}
            _ => {
                return Err(VmError::RuntimeError(format!(
                    "Conditional jump requires boolean, got {:?}",
                    value
                )))
            }
        }
        Ok(())
    }

    fn execute_jump_if_false(&mut self, target: usize) -> Result<(), VmError> {
        let value = self.peek_stack("jump if false")?;
        match &value {
            Value::Boolean(false) => {
                if target >= self.program.instructions.len() {
                    return Err(VmError::InvalidJumpTarget(target));
                }
                self.pc = target;
            }
            Value::Boolean(true) => {}
            _ => {
                return Err(VmError::RuntimeError(format!(
                    "Conditional jump requires boolean, got {:?}",
                    value
                )))
            }
        }
        Ok(())
    }

    fn execute_call(&mut self, argc: usize) -> Result<(), VmError> {
        if self.stack.len() < argc + 1 {
            return Err(VmError::StackUnderflow(format!(
                "Call needs {} args + function, stack has {}",
                argc,
                self.stack.len()
            )));
        }

        let func_value = self.stack[self.stack.len() - argc - 1].clone();

        match &func_value {
            Value::BuiltIn(name) => {
                let result = self.execute_builtin(name, argc)?;
                self.stack.push(result);
                self.track_stack_top();
                Ok(())
            }
            Value::Function(idx) => {
                let func_idx = *idx;

                if func_idx >= self.program.functions.len() {
                    return Err(VmError::RuntimeError(format!(
                        "Invalid function index {}",
                        func_idx
                    )));
                }

                // Clone function info to avoid borrow conflicts
                let func_info = self.program.functions[func_idx].clone();

                // Pop args first (they're on top), then the callee
                let mut args: Vec<Value> = Vec::new();
                for _ in 0..argc {
                    args.push(self.pop_stack("call arg")?);
                }
                args.reverse();
                self.pop_stack("call callee")?;

                if argc > func_info.params.len() {
                    return Err(VmError::RuntimeError(format!(
                        "Expected {} arguments, got {}",
                        func_info.params.len(),
                        argc
                    )));
                }

                if func_info.is_async {
                    // §101: calling an async function creates a task; the body
                    // does not run until the task is awaited (or the scheduler
                    // starts it).
                    return self.create_task(func_idx, args, func_value.clone(), None);
                }

                let root_scope = self.build_function_scope(&func_info, &args, &func_value);

                let return_pc = self.pc;
                let return_instructions = self.program.instructions.clone();

                if func_idx < self.program.function_bodies.len() {
                    self.program.instructions = self.program.function_bodies[func_idx].clone();
                }

                self.call_stack.push(Frame {
                    function_index: func_idx,
                    scopes: vec![root_scope],
                    return_pc,
                    captured_env: None,
                    scope_env: None,
                    return_instructions: Some(return_instructions),
                    task_id: None,
                });

                self.pc = 0;

                Ok(())
            }
            Value::Closure { function_index, env, .. } => {
                let func_idx = *function_index;

                if func_idx >= self.program.functions.len() {
                    return Err(VmError::RuntimeError(format!(
                        "Invalid function index {}",
                        func_idx
                    )));
                }

                let func_info = self.program.functions[func_idx].clone();

                let mut args: Vec<Value> = Vec::new();
                for _ in 0..argc {
                    args.push(self.pop_stack("call arg")?);
                }
                args.reverse();
                self.pop_stack("call callee")?;

                if argc > func_info.params.len() {
                    return Err(VmError::RuntimeError(format!(
                        "Expected {} arguments, got {}",
                        func_info.params.len(),
                        argc
                    )));
                }

                if func_info.is_async {
                    // §101: async closure call creates a task with its env.
                    return self.create_task(func_idx, args, func_value.clone(), Some(env.clone()));
                }

                let root_scope = self.build_function_scope(&func_info, &args, &func_value);

                let return_pc = self.pc;
                let return_instructions = self.program.instructions.clone();

                if func_idx < self.program.function_bodies.len() {
                    self.program.instructions = self.program.function_bodies[func_idx].clone();
                }

                self.call_stack.push(Frame {
                    function_index: func_idx,
                    scopes: vec![root_scope],
                    return_pc,
                    captured_env: Some(env.clone()),
                    scope_env: None,
                    return_instructions: Some(return_instructions),
                    task_id: None,
                });

                self.pc = 0;

                Ok(())
            }
            _ => Err(VmError::RuntimeError(format!(
                "Cannot call {:?}",
                func_value
            ))),
        }
    }

    /// Build a function frame's root scope: parameters (with defaults applied),
    /// then the function's own name bound to `callee` for self-reference.
    fn build_function_scope(
        &self,
        func_info: &FunctionInfo,
        args: &[Value],
        callee: &Value,
    ) -> HashMap<String, Binding> {
        let mut root_scope = HashMap::new();

        for (i, param_name) in func_info.params.iter().enumerate() {
            let value = if i < args.len() {
                args[i].clone()
            } else if let Some(Some(default_idx)) = func_info.param_defaults.get(i) {
                self.constant_to_value(&self.program.constants[*default_idx])
            } else {
                Value::Null
            };
            root_scope.insert(
                param_name.clone(),
                Binding {
                    value,
                    is_const: false,
                    initialized: true,
                    live: None,
                },
            );
        }

        // Function's own name for self-reference (named functions and closures)
        if !func_info.name.is_empty() {
            root_scope.insert(
                func_info.name.clone(),
                Binding {
                    value: callee.clone(),
                    is_const: false,
                    initialized: true,
                    live: None,
                },
            );
        }

        root_scope
    }

    /// Create a pending task for an async function call (§101). The body runs
    /// when the scheduler gives the task control (round-robin ready queue, §112).
    fn create_task(
        &mut self,
        func_idx: usize,
        args: Vec<Value>,
        callee: Value,
        env: Option<HeapRefHandle>,
    ) -> Result<(), VmError> {
        let id = self.tasks.len();
        // Root heap-backed args/callee so GC keeps them alive while pending.
        for arg in &args {
            self.track_value_heap_refs(arg);
        }
        self.track_value_heap_refs(&callee);
        if let Some(e) = &env {
            self.add_live_ref(e.clone());
        }
        self.tasks.push(Task {
            state: TaskState::Pending,
            kind: TaskKind::Normal {
                func_idx,
                args,
                callee,
                env,
            },
            result: None,
            error: None,
            context: None,
            wait: None,
            terminal_seq: None,
        });
        self.ready_queue.push_back(id);
        self.stack.push(Value::Task(id));
        self.track_stack_top();
        Ok(())
    }

    /// Create a synthetic task driven by the scheduler instead of a function
    /// body (§103, §104, §109-§111). Combinator inputs may already be
    /// terminal, so they are evaluated immediately; sleeps only complete when
    /// a timer fires (so `task.sleep(0)` still yields, §111).
    fn create_synthetic_task(&mut self, kind: TaskKind) -> Result<usize, VmError> {
        let id = self.tasks.len();
        let is_sleep = matches!(kind, TaskKind::Sleep { .. });
        self.tasks.push(Task {
            state: TaskState::Pending,
            kind,
            result: None,
            error: None,
            context: None,
            wait: None,
            terminal_seq: None,
        });
        if !is_sleep {
            self.reevaluate_synthetic(id)?;
        }
        Ok(id)
    }

    /// Stamp the next terminal sequence number on a task reaching a terminal
    /// state. `task.race` uses it to pick the genuinely first-finished input
    /// even when several were already terminal (§104 determinism).
    fn stamp_terminal(&mut self, id: usize) {
        if self.tasks[id].terminal_seq.is_none() {
            let seq = self.next_terminal_seq;
            self.next_terminal_seq += 1;
            self.tasks[id].terminal_seq = Some(seq);
        }
    }

    /// Build the initial context for a pending normal task and mark it
    /// running (§106). The scheduler loads the returned context to execute it.
    fn prepare_task_start(&mut self, task_id: usize) -> Result<TaskContext, VmError> {
        if self.tasks[task_id].state != TaskState::Pending {
            return Err(VmError::InvalidExecutionState(format!(
                "Cannot start task in state {}",
                self.tasks[task_id].state.as_str()
            )));
        }
        let (func_idx, args, callee, env) = match self.tasks[task_id].kind.clone() {
            TaskKind::Normal {
                func_idx,
                args,
                callee,
                env,
            } => (func_idx, args, callee, env),
            _ => {
                return Err(VmError::InvalidExecutionState(format!(
                    "Task {} has no body to start",
                    task_id
                )))
            }
        };
        if func_idx >= self.program.functions.len() {
            return Err(VmError::RuntimeError(format!(
                "Invalid function index {}",
                func_idx
            )));
        }
        let func_info = self.program.functions[func_idx].clone();
        let root_scope = self.build_function_scope(&func_info, &args, &callee);
        let instructions = if func_idx < self.program.function_bodies.len() {
            self.program.function_bodies[func_idx].clone()
        } else {
            Vec::new()
        };
        self.tasks[task_id].state = TaskState::Running;
        Ok(TaskContext {
            pc: 0,
            instructions,
            stack: Vec::new(),
            call_stack: vec![Frame {
                function_index: func_idx,
                scopes: vec![root_scope],
                return_pc: 0,
                captured_env: env,
                scope_env: None,
                return_instructions: None,
                task_id: Some(task_id),
            }],
            handlers: Vec::new(),
            pending_error: None,
            pending_return: None,
        })
    }

    /// Save the active instruction-loop state into `task_id`'s context.
    /// Called when a task suspends on `await` (§112).
    fn save_active_context(&mut self, task_id: usize) {
        let ctx = TaskContext {
            pc: self.pc,
            instructions: std::mem::take(&mut self.program.instructions),
            stack: std::mem::take(&mut self.stack),
            call_stack: std::mem::take(&mut self.call_stack),
            handlers: std::mem::take(&mut self.handlers),
            pending_error: self.pending_error.take(),
            pending_return: self.pending_return.take(),
        };
        self.tasks[task_id].context = Some(ctx);
    }

    /// Load a saved context into the instruction-loop fields.
    fn load_context(&mut self, ctx: TaskContext) {
        self.pc = ctx.pc;
        self.program.instructions = ctx.instructions;
        self.stack = ctx.stack;
        self.call_stack = ctx.call_stack;
        self.handlers = ctx.handlers;
        self.pending_error = ctx.pending_error;
        self.pending_return = ctx.pending_return;
    }

    /// Record a task reaching `completed` with its result (§106, §107) and
    /// wake everything waiting on it (§103, §104, §109, §110).
    fn finish_task(&mut self, task_id: usize, result: &Value) -> Result<(), VmError> {
        if !self.tasks[task_id].state.is_terminal() {
            self.tasks[task_id].state = TaskState::Completed;
            self.tasks[task_id].result = Some(result.clone());
            self.tasks[task_id].context = None;
            self.track_value_heap_refs(result);
            self.stamp_terminal(task_id);
            self.notify_dependents(task_id)?;
        }
        Ok(())
    }

    /// Mark a task `failed`, discard its context, and wake dependents (§106,
    /// §108). The error stays stored; it is thrown only where the task is
    /// awaited — an un-awaited failure must not stop the program (§108).
    fn fail_task(&mut self, task_id: usize, msg: String) -> Result<(), VmError> {
        if task_id >= self.tasks.len() {
            return Err(VmError::RuntimeError(msg));
        }
        if !self.tasks[task_id].state.is_terminal() {
            self.tasks[task_id].state = TaskState::Failed;
            self.tasks[task_id].error = Some(msg);
            self.tasks[task_id].context = None;
            self.tasks[task_id].wait = None;
            self.stamp_terminal(task_id);
            if self.active_task == Some(task_id) {
                // The failing task owns the active loop fields: clear them so
                // the step loop exits.
                self.stack.clear();
                self.call_stack.clear();
                self.handlers.clear();
                self.pending_error = None;
                self.pending_return = None;
                self.active_task = None;
            }
            self.notify_dependents(task_id)?;
        }
        Ok(())
    }

    /// Cancel a non-terminal task (§105). Already-terminal tasks are a no-op.
    /// Cancellation is cooperative: under the single-threaded scheduler other
    /// code can only reach a task while it is pending or suspended, so its
    /// saved context is simply discarded (ISSUES.md §64 for limits).
    fn cancel_task(&mut self, task_id: usize) -> Result<(), VmError> {
        if task_id >= self.tasks.len() {
            return Err(VmError::RuntimeError(format!(
                "Invalid task handle {}",
                task_id
            )));
        }
        if self.tasks[task_id].state.is_terminal() {
            return Ok(()); // §105: cancelling a finished task is a no-op.
        }
        if self.active_task == Some(task_id) {
            // Spec is silent on self-cancellation; refusing avoids inventing
            // an unwind rule (ISSUES.md §64).
            return Err(VmError::RuntimeError(
                "Cannot cancel the currently running task".to_string(),
            ));
        }
        self.tasks[task_id].state = TaskState::Cancelled;
        self.tasks[task_id].context = None;
        self.tasks[task_id].wait = None;
        self.stamp_terminal(task_id);
        self.notify_dependents(task_id)
    }

    /// Wake everything that depends on `tid` reaching a terminal state:
    /// synthetic combinators are re-evaluated; suspended awaiters are queued
    /// so the scheduler can resume them (§112).
    fn notify_dependents(&mut self, tid: usize) -> Result<(), VmError> {
        if tid >= self.tasks.len() {
            return Ok(());
        }
        let mut waiters = Vec::new();
        for i in 0..self.tasks.len() {
            if i == tid {
                continue;
            }
            let depends = match &self.tasks[i].kind {
                TaskKind::All(ids) | TaskKind::Race(ids) => ids.contains(&tid),
                TaskKind::Wait(target) | TaskKind::Timeout { target, .. } => *target == tid,
                _ => false,
            };
            if depends && !self.tasks[i].state.is_terminal() {
                self.reevaluate_synthetic(i)?;
            }
            if self.tasks[i].wait == Some(tid)
                && matches!(self.tasks[i].state, TaskState::Running)
            {
                waiters.push(i);
            }
        }
        for w in waiters {
            self.ready_queue.push_back(w);
        }
        Ok(())
    }

    /// Re-evaluate a synthetic task after an input changes state or right
    /// after creation (§103, §104, §109-§111).
    fn reevaluate_synthetic(&mut self, id: usize) -> Result<(), VmError> {
        if id >= self.tasks.len() || self.tasks[id].state.is_terminal() {
            return Ok(());
        }
        match self.tasks[id].kind.clone() {
            TaskKind::Normal { .. } => Ok(()),
            // Event-driven: completes when the worker thread's result is
            // polled by the scheduler, never by re-evaluation.
            TaskKind::Http { .. } => Ok(()),
            TaskKind::All(ids) => {
                // Fail fast: propagate the first failure without waiting for
                // the remaining tasks to finish (Scope B; §103).
                for &x in &ids {
                    match self.tasks[x].state {
                        TaskState::Failed => {
                            let msg = self.tasks[x]
                                .error
                                .clone()
                                .unwrap_or_else(|| "Task failed".to_string());
                            return self.fail_task(id, msg);
                        }
                        TaskState::Cancelled => {
                            return self.fail_task(id, "Task was cancelled".to_string());
                        }
                        _ => {}
                    }
                }
                if ids
                    .iter()
                    .all(|&x| self.tasks[x].state == TaskState::Completed)
                {
                    let mut results = Vec::with_capacity(ids.len());
                    for &x in &ids {
                        results.push(self.tasks[x].result.clone().unwrap_or(Value::Null));
                    }
                    let list = self.values_to_list(results)?;
                    return self.finish_task(id, &list);
                }
                Ok(())
            }
            TaskKind::Race(ids) => {
                // First input to have finished (by terminal order) decides —
                // deterministic under the single-threaded scheduler (§104).
                let mut winner: Option<usize> = None;
                let mut best_seq = u64::MAX;
                for &x in &ids {
                    if let Some(seq) = self.tasks[x].terminal_seq {
                        if seq < best_seq {
                            best_seq = seq;
                            winner = Some(x);
                        }
                    }
                }
                let Some(w) = winner else { return Ok(()) };
                match self.tasks[w].state {
                    TaskState::Completed => {
                        let r = self.tasks[w].result.clone().unwrap_or(Value::Null);
                        self.finish_task(id, &r)
                    }
                    TaskState::Failed => {
                        let m = self.tasks[w]
                            .error
                            .clone()
                            .unwrap_or_else(|| "Task failed".to_string());
                        self.fail_task(id, m)
                    }
                    TaskState::Cancelled => self.fail_task(id, "Task was cancelled".to_string()),
                    _ => Ok(()),
                }
            }
            TaskKind::Wait(target) => match self.tasks[target].state {
                TaskState::Completed => self.finish_task(id, &Value::Null),
                TaskState::Failed => {
                    let m = self.tasks[target]
                        .error
                        .clone()
                        .unwrap_or_else(|| "Task failed".to_string());
                    self.fail_task(id, m)
                }
                TaskState::Cancelled => self.fail_task(id, "Task was cancelled".to_string()),
                _ => Ok(()),
            },
            TaskKind::Sleep { deadline_ms } => {
                if self.now_ms >= deadline_ms {
                    self.finish_task(id, &Value::Null)
                } else {
                    Ok(())
                }
            }
            TaskKind::Timeout { target, deadline_ms } => match self.tasks[target].state {
                TaskState::Completed => {
                    let r = self.tasks[target].result.clone().unwrap_or(Value::Null);
                    self.finish_task(id, &r)
                }
                TaskState::Failed => {
                    let m = self.tasks[target]
                        .error
                        .clone()
                        .unwrap_or_else(|| "Task failed".to_string());
                    self.fail_task(id, m)
                }
                TaskState::Cancelled => self.fail_task(id, "Task was cancelled".to_string()),
                _ if self.now_ms >= deadline_ms => {
                    // §110: timeout errors the wait; the task is not cancelled.
                    self.fail_task(
                        id,
                        "task.timeout(): task did not complete before the timeout".to_string(),
                    )
                }
                _ => Ok(()),
            },
        }
    }

    /// Fire every sleep/timeout whose deadline has been reached (§110, §111).
    /// Returns true if any timer task was re-evaluated.
    fn wake_due_timers(&mut self) -> Result<bool, VmError> {
        let mut woken = false;
        for id in 0..self.tasks.len() {
            let due = match self.tasks[id].kind {
                TaskKind::Sleep { deadline_ms } | TaskKind::Timeout { deadline_ms, .. } => {
                    self.now_ms >= deadline_ms
                }
                _ => false,
            };
            if due && !self.tasks[id].state.is_terminal() {
                self.reevaluate_synthetic(id)?;
                woken = true;
            }
        }
        Ok(woken)
    }

    /// Earliest outstanding timer deadline, if any.
    fn earliest_deadline(&self) -> Option<u128> {
        self.tasks
            .iter()
            .filter(|t| !t.state.is_terminal())
            .filter_map(|t| match t.kind {
                TaskKind::Sleep { deadline_ms } => Some(deadline_ms),
                TaskKind::Timeout { deadline_ms, .. } => Some(deadline_ms),
                _ => None,
            })
            .min()
    }

    fn count_terminal(&self) -> usize {
        self.tasks.iter().filter(|t| t.state.is_terminal()).count()
    }

    /// Pop the next runnable normal task from the FIFO ready queue (§112).
    /// Stale entries (terminal or re-suspended tasks) are skipped.
    fn pop_runnable(&mut self) -> Option<usize> {
        while let Some(tid) = self.ready_queue.pop_front() {
            if tid >= self.tasks.len() {
                continue;
            }
            if !matches!(self.tasks[tid].kind, TaskKind::Normal { .. }) {
                continue; // synthetics are event-driven, never queued
            }
            match self.tasks[tid].state {
                TaskState::Pending => return Some(tid),
                TaskState::Running if self.tasks[tid].context.is_some() => return Some(tid),
                _ => {}
            }
        }
        None
    }

    /// Cooperative scheduler (§112): run ready tasks round-robin until every
    /// task reaches a terminal state. Awaiting/sleeping tasks never block
    /// other ready tasks; when nothing is runnable the virtual clock jumps
    /// straight to the next timer deadline (§110, §111).
    fn run_scheduler(&mut self) -> Result<(), VmError> {
        loop {
            // Ready queue must hold every pending normal task (heal in case
            // an entry was consumed by a stale pop).
            for id in 0..self.tasks.len() {
                if matches!(self.tasks[id].kind, TaskKind::Normal { .. })
                    && self.tasks[id].state == TaskState::Pending
                    && !self.ready_queue.contains(&id)
                {
                    self.ready_queue.push_back(id);
                }
            }

            self.wake_due_timers()?;

            if let Some(tid) = self.pop_runnable() {
                self.run_task_step(tid)?;
                continue;
            }

            // Collect finished HTTP exchanges before deciding idleness.
            if self.poll_http_completed()? {
                continue;
            }

            // Nothing runnable: jump the virtual clock to the next deadline.
            if let Some(d) = self.earliest_deadline() {
                let before = self.count_terminal();
                if d > self.now_ms {
                    self.now_ms = d;
                }
                self.wake_due_timers()?;
                if self.count_terminal() == before && self.ready_queue.is_empty() {
                    return Err(VmError::InvalidExecutionState(
                        "Scheduler made no progress waking timers".to_string(),
                    ));
                }
                continue;
            }

            // Virtual timers are done, but HTTP exchanges are still on the
            // wire: wait in real time. Virtual time must not jump here —
            // HTTP progress depends on the real clock (ISSUES §65).
            if self.http_inflight() {
                thread::park_timeout(Duration::from_millis(1));
                continue;
            }

            // No timers left. Either every task is terminal, or the remaining
            // tasks wait on each other forever (spec has no cycle rule).
            if self.tasks.iter().any(|t| !t.state.is_terminal()) {
                return Err(VmError::RuntimeError(
                    "Task deadlock: remaining tasks are waiting on each other".to_string(),
                ));
            }
            return Ok(());
        }
    }

    /// Run one task until it suspends on `await` or reaches a terminal state.
    /// Its context is saved back to the task table on suspension (§112).
    fn run_task_step(&mut self, tid: usize) -> Result<(), VmError> {
        match self.tasks[tid].state {
            TaskState::Pending => {
                let ctx = self.prepare_task_start(tid)?;
                self.active_task = Some(tid);
                self.load_context(ctx);
            }
            TaskState::Running => {
                let ctx = self.tasks[tid].context.take().ok_or_else(|| {
                    VmError::InvalidExecutionState(format!(
                        "Task {} has no saved context",
                        tid
                    ))
                })?;
                self.active_task = Some(tid);
                self.load_context(ctx);
                // Resolve an outstanding await whose target is terminal now
                // (§107: completed → result, failed/cancelled → error).
                if let Some(target) = self.tasks[tid].wait {
                    match self.tasks[target].state {
                        TaskState::Completed => {
                            let result =
                                self.tasks[target].result.clone().unwrap_or(Value::Null);
                            self.tasks[tid].wait = None;
                            self.stack.push(result);
                            self.track_stack_top();
                        }
                        TaskState::Failed => {
                            let msg = self.tasks[target]
                                .error
                                .clone()
                                .unwrap_or_else(|| "Task failed".to_string());
                            self.tasks[tid].wait = None;
                            // May dispatch to this task's try/catch or fail it.
                            self.raise_runtime_error(msg)?;
                        }
                        TaskState::Cancelled => {
                            self.tasks[tid].wait = None;
                            self.raise_runtime_error("Task was cancelled".to_string())?;
                        }
                        _ => {
                            // Stale queue entry — target not terminal yet:
                            // re-suspend without executing.
                            self.save_active_context(tid);
                            self.active_task = None;
                            return Ok(());
                        }
                    }
                }
            }
            _ => return Ok(()), // stale entry for a terminal task
        }

        while !self.call_stack.is_empty() {
            if self.tasks[tid].wait.is_some() {
                // Suspended on await: save the context, hand control back.
                self.save_active_context(tid);
                self.active_task = None;
                return Ok(());
            }
            if self.pc >= self.program.instructions.len() {
                return Err(VmError::InvalidExecutionState(
                    "Task execution ran past bytecode end".to_string(),
                ));
            }
            let instruction = self.program.instructions[self.pc].clone();
            self.pc += 1;
            if let Err(err) = self.execute_instruction(&instruction) {
                if let VmError::RuntimeError(msg) = err {
                    self.raise_runtime_error(msg)?;
                } else {
                    return Err(err);
                }
            }
        }
        // The task finished or failed; its context is gone.
        self.active_task = None;
        Ok(())
    }

    /// `await expr` (§49, §101, §107, §113): resolve a task to its result.
    ///
    /// - completed → stored result, without re-running the body
    /// - failed → stored error thrown here (catchable, §108)
    /// - cancelled → cancellation error (§105, §113)
    /// - pending/running → suspend the current task; the scheduler runs other
    ///   ready tasks until the target is terminal (§107: await waits for a
    ///   running task instead of blocking others, §112)
    /// - non-task → runtime ERROR (spec silent; ISSUES.md §63b)
    fn execute_await(&mut self) -> Result<(), VmError> {
        let value = self.pop_stack("await")?;
        let target = match value {
            Value::Task(id) => id,
            other => {
                return Err(VmError::RuntimeError(format!(
                    "await expects a task, got {}",
                    type_name(&other, &self.heap)
                )))
            }
        };

        if target >= self.tasks.len() {
            return Err(VmError::RuntimeError(format!(
                "Invalid task handle {}",
                target
            )));
        }

        match self.tasks[target].state {
            TaskState::Completed => {
                let result = self.tasks[target].result.clone().unwrap_or(Value::Null);
                self.stack.push(result);
                self.track_stack_top();
                Ok(())
            }
            TaskState::Failed => Err(VmError::RuntimeError(
                self.tasks[target]
                    .error
                    .clone()
                    .unwrap_or_else(|| "Task failed".to_string()),
            )),
            TaskState::Cancelled => Err(VmError::RuntimeError(
                "Task was cancelled".to_string(),
            )),
            TaskState::Pending | TaskState::Running => {
                // Only task contexts may suspend; top-level await is rejected
                // at compile time (§49, §101; ISSUES.md §63a).
                let Some(tid) = self.active_task else {
                    return Err(VmError::RuntimeError(
                        "await cannot suspend the top-level program".to_string(),
                    ));
                };
                if target == tid {
                    return Err(VmError::RuntimeError(
                        "Cannot await a task from within itself".to_string(),
                    ));
                }
                self.tasks[tid].wait = Some(target);
                // The step loop sees `wait` set and saves the context.
                Ok(())
            }
        }
    }

    fn execute_builtin(&mut self, name: &str, argc: usize) -> Result<Value, VmError> {
        // All BuiltIn values (root functions and namespace methods) pop args
        // and dispatch through call_builtin — the soft CALL_NATIVE boundary.
        let mut args = Vec::new();
        for _ in 0..argc {
            args.push(self.pop_stack("builtin arg")?);
        }
        args.reverse();
        self.pop_stack("builtin callee")?;
        self.call_builtin(name, &args)
    }

    fn call_builtin(&mut self, name: &str, args: &[Value]) -> Result<Value, VmError> {
        match name {
            "print" => {
                if args.is_empty() {
                    self.output.push(String::new());
                } else {
                    let output = args[0].format_value(&self.heap);
                    self.output.push(output);
                }
                Ok(Value::Null)
            }
            "print.err" => {
                if args.is_empty() {
                    self.err_output.push(String::new());
                } else {
                    let output = args[0].format_value(&self.heap);
                    self.err_output.push(output);
                }
                Ok(Value::Null)
            }
            "type" => {
                if args.len() != 1 {
                    return Err(VmError::RuntimeError(
                        "type() expects exactly 1 argument".to_string(),
                    ));
                }
                let t = type_name(&args[0], &self.heap);
                Ok(Value::Str(t.to_string()))
            }
            "number" => {
                if args.len() != 1 {
                    return Err(VmError::RuntimeError(
                        "number() expects exactly 1 argument".to_string(),
                    ));
                }
                match &args[0] {
                    Value::Number(n) => Ok(Value::Number(*n)),
                    Value::Str(s) => match s.trim().parse::<f64>() {
                        Ok(n) => Ok(Value::Number(n)),
                        Err(_) => Err(VmError::RuntimeError(format!(
                            "Invalid conversion to number: \"{}\"",
                            s
                        ))),
                    },
                    other => Err(VmError::RuntimeError(format!(
                        "Invalid conversion to number from {:?}",
                        other
                    ))),
                }
            }
            "string" => {
                if args.len() != 1 {
                    return Err(VmError::RuntimeError(
                        "string() expects exactly 1 argument".to_string(),
                    ));
                }
                match &args[0] {
                    Value::Str(s) => Ok(Value::Str(s.clone())),
                    Value::Number(n) => {
                        let s = if *n == (*n as i64) as f64 {
                            format!("{}", *n as i64)
                        } else {
                            format!("{}", n)
                        };
                        Ok(Value::Str(s))
                    }
                    Value::Boolean(b) => Ok(Value::Str(b.to_string())),
                    Value::Null => Ok(Value::Str("null".to_string())),
                    other => Err(VmError::RuntimeError(format!(
                        "Invalid conversion to string from {:?}",
                        other
                    ))),
                }
            }
            "boolean" => {
                if args.len() != 1 {
                    return Err(VmError::RuntimeError(
                        "boolean() expects exactly 1 argument".to_string(),
                    ));
                }
                match &args[0] {
                    Value::Boolean(b) => Ok(Value::Boolean(*b)),
                    other => Err(VmError::RuntimeError(format!(
                        "Invalid conversion to boolean from {:?}",
                        other
                    ))),
                }
            }
            // ---- string.* (StandardLibrary §42, §58) ----
            // Wrong argument type → RuntimeError (§58, catchable).
            "string.length" => {
                let s = expect_string_arg(&args, 0, "string.length")?;
                Ok(Value::Number(s.chars().count() as f64))
            }
            "string.contains" => {
                let s = expect_string_arg(&args, 0, "string.contains")?;
                let sub = expect_string_arg(&args, 1, "string.contains")?;
                Ok(Value::Boolean(s.contains(sub)))
            }
            "string.trim" => {
                let s = expect_string_arg(&args, 0, "string.trim")?;
                Ok(Value::Str(s.trim().to_string()))
            }
            "string.split" => {
                let s = expect_string_arg(&args, 0, "string.split")?;
                let sep = expect_string_arg(&args, 1, "string.split")?;
                let parts: Vec<Value> = s.split(sep).map(|p| Value::Str(p.to_string())).collect();
                self.values_to_list(parts)
            }
            "string.replace" => {
                let s = expect_string_arg(&args, 0, "string.replace")?;
                let target = expect_string_arg(&args, 1, "string.replace")?;
                let replacement = expect_string_arg(&args, 2, "string.replace")?;
                Ok(Value::Str(s.replace(target, replacement)))
            }
            "string.startsWith" => {
                let s = expect_string_arg(&args, 0, "string.startsWith")?;
                let prefix = expect_string_arg(&args, 1, "string.startsWith")?;
                Ok(Value::Boolean(s.starts_with(prefix)))
            }
            "string.endsWith" => {
                let s = expect_string_arg(&args, 0, "string.endsWith")?;
                let suffix = expect_string_arg(&args, 1, "string.endsWith")?;
                Ok(Value::Boolean(s.ends_with(suffix)))
            }
            "string.lower" => {
                let s = expect_string_arg(&args, 0, "string.lower")?;
                Ok(Value::Str(s.to_lowercase()))
            }
            "string.upper" => {
                let s = expect_string_arg(&args, 0, "string.upper")?;
                Ok(Value::Str(s.to_uppercase()))
            }
            "string.indexOf" => {
                let s = expect_string_arg(&args, 0, "string.indexOf")?;
                let sub = expect_string_arg(&args, 1, "string.indexOf")?;
                match s.find(sub) {
                    Some(byte_idx) => {
                        // Spec §58: index starts from 0 — char index for Unicode
                        // consistency with string indexing (§36 uses chars).
                        let char_idx = s[..byte_idx].chars().count();
                        Ok(Value::Number(char_idx as f64))
                    }
                    None => Ok(Value::Number(-1.0)),
                }
            }
            // ---- list.* (Language §34) ----
            "list.add" => {
                let r = expect_list_arg(&args, 0, "list.add")?;
                if args.len() != 2 {
                    return Err(VmError::RuntimeError(
                        "list.add expects exactly 2 arguments".to_string(),
                    ));
                }
                let elem = args[1].heap_object_for_element(&mut self.heap);
                match self.heap.get_mut(&r) {
                    Ok(HeapObject::List { items, is_const }) => {
                        if *is_const {
                            return Err(VmError::RuntimeError(
                                "Cannot modify const list".to_string(),
                            ));
                        }
                        items.push(elem);
                    }
                    _ => {
                        return Err(VmError::RuntimeError(
                            "list.add expects a list".to_string(),
                        ))
                    }
                }
                Ok(Value::Null)
            }
            "list.remove" => {
                let r = expect_list_arg(&args, 0, "list.remove")?;
                if args.len() != 2 {
                    return Err(VmError::RuntimeError(
                        "list.remove expects exactly 2 arguments".to_string(),
                    ));
                }
                let idx = match &args[1] {
                    Value::Number(n) => {
                        let i = *n as usize;
                        if *n < 0.0 || *n != i as f64 {
                            return Err(VmError::RuntimeError(format!(
                                "List index {} out of bounds",
                                n
                            )));
                        }
                        i
                    }
                    other => {
                        return Err(VmError::RuntimeError(format!(
                            "list.remove index must be number, got {:?}",
                            other
                        )))
                    }
                };
                match self.heap.get_mut(&r) {
                    Ok(HeapObject::List { items, is_const }) => {
                        if *is_const {
                            return Err(VmError::RuntimeError(
                                "Cannot modify const list".to_string(),
                            ));
                        }
                        if idx >= items.len() {
                            return Err(VmError::RuntimeError(format!(
                                "List index {} out of bounds (length {})",
                                idx,
                                items.len()
                            )));
                        }
                        items.remove(idx);
                    }
                    _ => {
                        return Err(VmError::RuntimeError(
                            "list.remove expects a list".to_string(),
                        ))
                    }
                }
                Ok(Value::Null)
            }
            "list.length" => {
                let r = expect_list_arg(&args, 0, "list.length")?;
                match self.heap.get(&r) {
                    Ok(HeapObject::List { items, .. }) => {
                        Ok(Value::Number(items.len() as f64))
                    }
                    _ => Err(VmError::RuntimeError(
                        "list.length expects a list".to_string(),
                    )),
                }
            }
            // ---- object.* (Language §39) ----
            "object.remove" => {
                let r = expect_object_arg(&args, 0, "object.remove")?;
                if args.len() != 2 {
                    return Err(VmError::RuntimeError(
                        "object.remove expects exactly 2 arguments".to_string(),
                    ));
                }
                let key = match &args[1] {
                    Value::Str(s) => s.clone(),
                    other => {
                        return Err(VmError::RuntimeError(format!(
                            "object.remove key must be string, got {:?}",
                            other
                        )))
                    }
                };
                match self.heap.get_mut(&r) {
                    Ok(HeapObject::Object { props, is_const }) => {
                        if *is_const {
                            return Err(VmError::RuntimeError(
                                "Cannot modify const object".to_string(),
                            ));
                        }
                        let before = props.len();
                        props.retain(|(k, _)| k != &key);
                        if props.len() == before {
                            // Spec silent on remove-missing-key (§39 only shows
                            // successful remove). Analogous to §38 access → ERROR.
                            // Recorded: ISSUES.md §60a.
                            return Err(VmError::RuntimeError(format!(
                                "Key '{}' not found",
                                key
                            )));
                        }
                    }
                    _ => {
                        return Err(VmError::RuntimeError(
                            "object.remove expects an object".to_string(),
                        ))
                    }
                }
                Ok(Value::Null)
            }
            // ---- color.* (StandardLibrary §45) ----
            // ANSI codes written directly; no Chalk/Node dependency (§45).
            // Specific resets (39/22) so bold(color.red(...)) nests correctly.
            "color.red" | "color.green" | "color.yellow" | "color.blue" | "color.bold" => {
                if args.len() != 1 {
                    return Err(VmError::RuntimeError(format!(
                        "{} expects exactly 1 argument",
                        name
                    )));
                }
                let text = args[0].format_value(&self.heap);
                let (open, close) = match name {
                    "color.red" => ("\x1b[31m", "\x1b[39m"),
                    "color.green" => ("\x1b[32m", "\x1b[39m"),
                    "color.yellow" => ("\x1b[33m", "\x1b[39m"),
                    "color.blue" => ("\x1b[34m", "\x1b[39m"),
                    "color.bold" => ("\x1b[1m", "\x1b[22m"),
                    _ => unreachable!(),
                };
                Ok(Value::Str(format!("{}{}{}", open, text, close)))
            }
            // ---- file.* (StandardLibrary §48) ----
            // Relative paths resolve against the process working directory.
            // IO errors → RuntimeError → catchable via try/catch (§48, Job 009).
            "file.read" => {
                let path = expect_string_arg(&args, 0, "file.read")?;
                if args.len() != 1 {
                    return Err(VmError::RuntimeError(
                        "file.read expects exactly 1 argument".to_string(),
                    ));
                }
                match std::fs::read_to_string(path) {
                    Ok(content) => Ok(Value::Str(content)),
                    Err(e) => Err(VmError::RuntimeError(format!(
                        "file.read('{}'): {}",
                        path, e
                    ))),
                }
            }
            "file.write" => {
                let path = expect_string_arg(&args, 0, "file.write")?;
                if args.len() != 2 {
                    return Err(VmError::RuntimeError(
                        "file.write expects exactly 2 arguments".to_string(),
                    ));
                }
                let content = expect_string_arg(&args, 1, "file.write")?;
                match std::fs::write(path, content) {
                    Ok(()) => Ok(Value::Null),
                    Err(e) => Err(VmError::RuntimeError(format!(
                        "file.write('{}'): {}",
                        path, e
                    ))),
                }
            }
            "file.exists" => {
                let path = expect_string_arg(&args, 0, "file.exists")?;
                if args.len() != 1 {
                    return Err(VmError::RuntimeError(
                        "file.exists expects exactly 1 argument".to_string(),
                    ));
                }
                Ok(Value::Boolean(std::path::Path::new(path).exists()))
            }
            "file.remove" => {
                let path = expect_string_arg(&args, 0, "file.remove")?;
                if args.len() != 1 {
                    return Err(VmError::RuntimeError(
                        "file.remove expects exactly 1 argument".to_string(),
                    ));
                }
                match std::fs::remove_file(path) {
                    Ok(()) => Ok(Value::Null),
                    Err(e) => Err(VmError::RuntimeError(format!(
                        "file.remove('{}'): {}",
                        path, e
                    ))),
                }
            }
            "file.mkdir" => {
                let path = expect_string_arg(&args, 0, "file.mkdir")?;
                if args.len() != 1 {
                    return Err(VmError::RuntimeError(
                        "file.mkdir expects exactly 1 argument".to_string(),
                    ));
                }
                match std::fs::create_dir(path) {
                    Ok(()) => Ok(Value::Null),
                    Err(e) => Err(VmError::RuntimeError(format!(
                        "file.mkdir('{}'): {}",
                        path, e
                    ))),
                }
            }
            // ---- system.* (StandardLibrary §52) ----
            "system.args" => {
                if !args.is_empty() {
                    return Err(VmError::RuntimeError(
                        "system.args expects exactly 0 arguments".to_string(),
                    ));
                }
                let vals: Vec<Value> = self
                    .program_args
                    .clone()
                    .into_iter()
                    .map(Value::Str)
                    .collect();
                self.values_to_list(vals)
            }
            "system.env" => {
                let key = expect_string_arg(&args, 0, "system.env")?;
                if args.len() != 1 {
                    return Err(VmError::RuntimeError(
                        "system.env expects exactly 1 argument".to_string(),
                    ));
                }
                // §52: missing variable → null. Non-UTF-8 env values also → null
                // (spec requires string; cannot represent invalid UTF-8 as string).
                match std::env::var(key) {
                    Ok(v) => Ok(Value::Str(v)),
                    Err(_) => Ok(Value::Null),
                }
            }
            "system.os" => {
                if !args.is_empty() {
                    return Err(VmError::RuntimeError(
                        "system.os expects exactly 0 arguments".to_string(),
                    ));
                }
                Ok(Value::Str(std::env::consts::OS.to_string()))
            }
            "system.arch" => {
                if !args.is_empty() {
                    return Err(VmError::RuntimeError(
                        "system.arch expects exactly 0 arguments".to_string(),
                    ));
                }
                Ok(Value::Str(std::env::consts::ARCH.to_string()))
            }
            // ---- json.* (StandardLibrary §51) ----
            // Parsing is explicit (§51); invalid input → RuntimeError, catchable.
            "json.parse" => {
                let s = expect_string_arg(&args, 0, "json.parse")?;
                if args.len() != 1 {
                    return Err(VmError::RuntimeError(
                        "json.parse expects exactly 1 argument".to_string(),
                    ));
                }
                match crate::json::parse(s) {
                    Ok(jv) => self.json_value_to_value(jv),
                    Err(e) => Err(VmError::RuntimeError(format!("Invalid JSON: {}", e))),
                }
            }
            "json.stringify" => {
                if args.len() != 1 {
                    return Err(VmError::RuntimeError(
                        "json.stringify expects exactly 1 argument".to_string(),
                    ));
                }
                // §51 shows object/list; also accept null/bool/number/string so
                // json.stringify(json.parse(s)) roundtrips any JSON value.
                // Functions/closures have no JSON form → ERROR.
                let jv = self.value_to_json_value(&args[0])?;
                match crate::json::stringify(&jv) {
                    Ok(s) => Ok(Value::Str(s)),
                    Err(e) => Err(VmError::RuntimeError(format!(
                        "Invalid JSON value: {}",
                        e
                    ))),
                }
            }
            // ---- task.* (Async.md §106) ----
            "task.state" => {
                if args.len() != 1 {
                    return Err(VmError::RuntimeError(
                        "task.state() expects exactly 1 argument".to_string(),
                    ));
                }
                match &args[0] {
                    Value::Task(id) if *id < self.tasks.len() => {
                        Ok(Value::Str(self.tasks[*id].state.as_str().to_string()))
                    }
                    // §106: task.state(A) with a non-task argument → ERROR.
                    _ => Err(VmError::RuntimeError(
                        "task.state() expects a task".to_string(),
                    )),
                }
            }
            // ---- task.* group/race/cancel/wait/timeout/sleep (§103-§105, §109-§111) ----
            "task.all" => {
                if args.len() != 1 {
                    return Err(VmError::RuntimeError(
                        "task.all() expects exactly 1 argument".to_string(),
                    ));
                }
                let ids = self.task_ids_from_list(&args[0], "task.all()")?;
                let id = self.create_synthetic_task(TaskKind::All(ids))?;
                Ok(Value::Task(id))
            }
            "task.race" => {
                if args.len() != 1 {
                    return Err(VmError::RuntimeError(
                        "task.race() expects exactly 1 argument".to_string(),
                    ));
                }
                let ids = self.task_ids_from_list(&args[0], "task.race()")?;
                if ids.is_empty() {
                    // §104: no task to wait for → ERROR.
                    return Err(VmError::RuntimeError(
                        "task.race() requires at least one task".to_string(),
                    ));
                }
                let id = self.create_synthetic_task(TaskKind::Race(ids))?;
                Ok(Value::Task(id))
            }
            "task.cancel" => {
                if args.len() != 1 {
                    return Err(VmError::RuntimeError(
                        "task.cancel() expects exactly 1 argument".to_string(),
                    ));
                }
                let tid = expect_task_arg(&args, 0, "task.cancel()")?;
                self.cancel_task(tid)?;
                Ok(Value::Null)
            }
            "task.wait" => {
                if args.len() != 1 {
                    return Err(VmError::RuntimeError(
                        "task.wait() expects exactly 1 argument".to_string(),
                    ));
                }
                let tid = expect_task_arg(&args, 0, "task.wait()")?;
                let id = self.create_synthetic_task(TaskKind::Wait(tid))?;
                Ok(Value::Task(id))
            }
            "task.timeout" => {
                if args.len() != 2 {
                    return Err(VmError::RuntimeError(
                        "task.timeout() expects exactly 2 arguments".to_string(),
                    ));
                }
                let target = expect_task_arg(&args, 0, "task.timeout()")?;
                let ms = duration_ms(&args[1], "task.timeout()")?;
                let id = self.create_synthetic_task(TaskKind::Timeout {
                    target,
                    deadline_ms: self.now_ms + ms,
                })?;
                Ok(Value::Task(id))
            }
            "task.sleep" => {
                if args.len() != 1 {
                    return Err(VmError::RuntimeError(
                        "task.sleep() expects exactly 1 argument".to_string(),
                    ));
                }
                let ms = duration_ms(&args[0], "task.sleep()")?;
                let id = self.create_synthetic_task(TaskKind::Sleep {
                    deadline_ms: self.now_ms + ms,
                })?;
                Ok(Value::Task(id))
            }
            // ---- http.* (StandardLibrary §50, §50.1, §59, §60) ----
            // Each call creates a Task (§50 + Async.md §101): the network
            // exchange runs on a worker thread and the awaiter is resumed by
            // the scheduler when the result lands.
            "http.get" => self.http_request("GET", args, "http.get"),
            "http.post" => self.http_request("POST", args, "http.post"),
            "http.put" => self.http_request("PUT", args, "http.put"),
            "http.patch" => self.http_request("PATCH", args, "http.patch"),
            "http.delete" => self.http_request("DELETE", args, "http.delete"),
            _ => Err(VmError::RuntimeError(format!(
                "Unknown built-in '{}'",
                name
            ))),
        }
    }

    /// Read a list argument as task ids (§103, §104). Every element must be
    /// an asynchronous task.
    fn task_ids_from_list(&self, value: &Value, fname: &str) -> Result<Vec<usize>, VmError> {
        let r = match value {
            Value::List(r) => r.clone(),
            _ => {
                return Err(VmError::RuntimeError(format!(
                    "{} expects a list",
                    fname
                )))
            }
        };
        match self.heap.get(&r) {
            Ok(HeapObject::List { items, .. }) => {
                let mut ids = Vec::with_capacity(items.len());
                for item_ref in items {
                    match self.heap.get(item_ref) {
                        Ok(HeapObject::Task(id)) => ids.push(*id),
                        _ => {
                            return Err(VmError::RuntimeError(format!(
                                "{} list elements must be tasks",
                                fname
                            )))
                        }
                    }
                }
                Ok(ids)
            }
            _ => Err(VmError::RuntimeError(format!("{} expects a list", fname))),
        }
    }

    /// `http.get/post/put/patch/delete` (§50, §50.1, §59, §60).
    ///
    /// Validates options synchronously (wrong types → ERROR per §59/§60),
    /// spawns a worker thread for the blocking exchange, and returns a Task
    /// the caller awaits (§101). The task is `running` from creation: the
    /// request is already in flight (§106).
    fn http_request(&mut self, method: &str, args: &[Value], fname: &str) -> Result<Value, VmError> {
        if args.is_empty() || args.len() > 2 {
            return Err(VmError::RuntimeError(format!(
                "{} expects 1 or 2 arguments",
                fname
            )));
        }
        let url = expect_string_arg(args, 0, fname)?.to_string();
        let mut query_pairs: Vec<String> = Vec::new();
        let mut headers: Vec<(String, String)> = Vec::new();
        let mut body: Option<String> = None;
        let mut timeout_ms: Option<u64> = None;

        if let Some(opt) = args.get(1) {
            let obj_ref = match opt {
                Value::Object(r) => r.clone(),
                _ => {
                    return Err(VmError::RuntimeError(format!(
                        "{} options must be an object",
                        fname
                    )))
                }
            };
            let props = match self.heap.get(&obj_ref) {
                Ok(HeapObject::Object { props, .. }) => props.clone(),
                _ => {
                    return Err(VmError::RuntimeError(format!(
                        "{} options must be an object",
                        fname
                    )))
                }
            };
            for (key, vr) in props {
                let v = Value::from_heap_ref(&vr, &self.heap)
                    .map_err(|_| VmError::RuntimeError("Failed to read options".to_string()))?;
                match key.as_str() {
                    // §59: object; string keys; string/number/boolean/null
                    // values; everything URL-encoded; null → empty value.
                    "query" => match v {
                        Value::Object(qr) => {
                            let qprops = match self.heap.get(&qr) {
                                Ok(HeapObject::Object { props, .. }) => props.clone(),
                                _ => {
                                    return Err(VmError::RuntimeError(format!(
                                        "{} query must be an object",
                                        fname
                                    )))
                                }
                            };
                            for (qk, qvr) in qprops {
                                let qv = Value::from_heap_ref(&qvr, &self.heap).map_err(|_| {
                                    VmError::RuntimeError("Failed to read query".to_string())
                                })?;
                                let encoded = match &qv {
                                    Value::Str(s) => percent_encode_component(s),
                                    Value::Number(n) => {
                                        percent_encode_component(&format!("{}", n))
                                    }
                                    Value::Boolean(b) => {
                                        percent_encode_component(if *b { "true" } else { "false" })
                                    }
                                    Value::Null => String::new(),
                                    _ => {
                                        return Err(VmError::RuntimeError(format!(
                                            "{} query values must be string, number, boolean, or null",
                                            fname
                                        )))
                                    }
                                };
                                query_pairs
                                    .push(format!("{}={}", percent_encode_component(&qk), encoded));
                            }
                        }
                        _ => {
                            return Err(VmError::RuntimeError(format!(
                                "{} query must be an object",
                                fname
                            )))
                        }
                    },
                    // §60: object; string keys; values MUST be strings.
                    "headers" => match v {
                        Value::Object(hr) => {
                            let hprops = match self.heap.get(&hr) {
                                Ok(HeapObject::Object { props, .. }) => props.clone(),
                                _ => {
                                    return Err(VmError::RuntimeError(format!(
                                        "{} headers must be an object",
                                        fname
                                    )))
                                }
                            };
                            for (hk, hvr) in hprops {
                                let hv = Value::from_heap_ref(&hvr, &self.heap).map_err(|_| {
                                    VmError::RuntimeError("Failed to read headers".to_string())
                                })?;
                                match hv {
                                    Value::Str(s) => headers.push((hk, s)),
                                    _ => {
                                        return Err(VmError::RuntimeError(format!(
                                            "{} header values must be strings",
                                            fname
                                        )))
                                    }
                                }
                            }
                        }
                        _ => {
                            return Err(VmError::RuntimeError(format!(
                                "{} headers must be an object",
                                fname
                            )))
                        }
                    },
                    // §50.1 shows body as a string (json.stringify output);
                    // the spec gives no other body form → string only.
                    "body" => match v {
                        Value::Str(s) => body = Some(s),
                        _ => {
                            return Err(VmError::RuntimeError(format!(
                                "{} body must be a string",
                                fname
                            )))
                        }
                    },
                    // §50.1: timeout in milliseconds, non-negative integer.
                    "timeout" => {
                        let ms = duration_ms(&v, fname)?;
                        timeout_ms = Some(u64::try_from(ms).unwrap_or(u64::MAX));
                    }
                    // Unknown option keys are ignored (spec is silent).
                    _ => {}
                }
            }
        }

        let final_url = append_query(&url, &query_pairs);
        let slot: HttpSlot = Arc::new(Mutex::new(None));
        let worker_slot = slot.clone();
        let worker_method = method.to_string();
        let worker_fname = fname.to_string();
        thread::spawn(move || {
            let result = http_perform(
                &worker_method,
                &final_url,
                &headers,
                body.as_deref(),
                timeout_ms,
            )
            .map_err(|e| format!("{}: {}", worker_fname, e));
            if let Ok(mut guard) = worker_slot.lock() {
                *guard = Some(result);
            }
        });

        let id = self.create_synthetic_task(TaskKind::Http { slot })?;
        // The exchange started at creation (§106 "running": sedang
        // menjalankan operasi asynchronous).
        self.tasks[id].state = TaskState::Running;
        Ok(Value::Task(id))
    }

    /// Convert a finished exchange into the §50 response object:
    /// `{ status: number, headers: object, body: string }`. The headers
    /// object is an independent copy — mutating it cannot touch the
    /// response or any future request (§60).
    fn build_http_response(&mut self, out: &HttpOutcome) -> Result<Value, VmError> {
        let mut hprops = Vec::with_capacity(out.headers.len());
        for (k, v) in &out.headers {
            let hr = Value::Str(v.clone()).heap_object_for_element(&mut self.heap);
            hprops.push((k.clone(), hr));
        }
        let headers_ref = self.heap.allocate(HeapObject::object(hprops));
        self.add_live_ref(headers_ref.clone());

        let fields = [
            ("status", Value::Number(out.status as f64)),
            ("headers", Value::Object(headers_ref)),
            ("body", Value::Str(out.body.clone())),
        ];
        let mut props = Vec::with_capacity(fields.len());
        for (name, val) in fields {
            let r = val.heap_object_for_element(&mut self.heap);
            props.push((name.to_string(), r));
        }
        let obj = self.heap.allocate(HeapObject::object(props));
        self.add_live_ref(obj.clone());
        self.gc.on_allocation();
        Ok(Value::Object(obj))
    }

    /// Finish every HTTP task whose worker result has landed. Returns true
    /// if any task transitioned (scheduler should loop again).
    fn poll_http_completed(&mut self) -> Result<bool, VmError> {
        for tid in 0..self.tasks.len() {
            if self.tasks[tid].state.is_terminal() {
                continue;
            }
            let slot = match &self.tasks[tid].kind {
                TaskKind::Http { slot } => slot.clone(),
                _ => continue,
            };
            let taken = match slot.lock() {
                Ok(mut guard) => guard.take(),
                Err(_) => None,
            };
            if let Some(res) = taken {
                match res {
                    Ok(outcome) => {
                        let value = self.build_http_response(&outcome)?;
                        self.finish_task(tid, &value)?;
                    }
                    Err(msg) => self.fail_task(tid, msg)?,
                }
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// True while at least one HTTP exchange is still on the wire. The
    /// scheduler parks in real time for these — virtual time must not jump
    /// past HTTP work, which progresses on the real clock (ISSUES §65).
    fn http_inflight(&self) -> bool {
        self.tasks
            .iter()
            .any(|t| !t.state.is_terminal() && matches!(t.kind, TaskKind::Http { .. }))
    }

    /// Recursively convert a parsed JsonValue into a VM Value (heap-backed
    /// for list/object) so GC and shared-reference semantics apply uniformly.
    fn json_value_to_value(&mut self, jv: crate::json::JsonValue) -> Result<Value, VmError> {
        use crate::json::JsonValue;
        match jv {
            JsonValue::Null => Ok(Value::Null),
            JsonValue::Bool(b) => Ok(Value::Boolean(b)),
            JsonValue::Number(n) => Ok(Value::Number(n)),
            JsonValue::String(s) => Ok(Value::Str(s)),
            JsonValue::Array(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.json_value_to_value(item)?);
                }
                self.values_to_list(values)
            }
            JsonValue::Object(entries) => {
                let mut props = Vec::with_capacity(entries.len());
                for (k, jv) in entries {
                    let v = self.json_value_to_value(jv)?;
                    let r = v.heap_object_for_element(&mut self.heap);
                    self.gc.on_allocation();
                    props.push((k, r));
                }
                let obj_ref = self.heap.allocate(HeapObject::object(props));
                self.gc.on_allocation();
                self.add_live_ref(obj_ref.clone());
                Ok(Value::Object(obj_ref))
            }
        }
    }

    /// Recursively convert a VM Value into JsonValue for json.stringify.
    /// Rejects function-like values (no JSON representation).
    fn value_to_json_value(&self, value: &Value) -> Result<crate::json::JsonValue, VmError> {
        use crate::json::JsonValue;
        match value {
            Value::Number(n) => {
                if n.is_finite() {
                    Ok(JsonValue::Number(*n))
                } else {
                    Err(VmError::RuntimeError(
                        "Cannot stringify non-finite number".to_string(),
                    ))
                }
            }
            Value::Str(s) => Ok(JsonValue::String(s.clone())),
            Value::Boolean(b) => Ok(JsonValue::Bool(*b)),
            Value::Null => Ok(JsonValue::Null),
            Value::List(r) => match self.heap.get(r) {
                Ok(HeapObject::List { items, .. }) => {
                    let mut out = Vec::with_capacity(items.len());
                    for ir in items {
                        let v = Value::from_heap_ref(ir, &self.heap)
                            .map_err(|e| VmError::RuntimeError(format!("{}", e)))?;
                        out.push(self.value_to_json_value(&v)?);
                    }
                    Ok(JsonValue::Array(out))
                }
                _ => Err(VmError::RuntimeError(
                    "json.stringify expects a list".to_string(),
                )),
            },
            Value::Object(r) => match self.heap.get(r) {
                Ok(HeapObject::Object { props, .. }) => {
                    let mut out = Vec::with_capacity(props.len());
                    for (k, vr) in props {
                        let v = Value::from_heap_ref(vr, &self.heap)
                            .map_err(|e| VmError::RuntimeError(format!("{}", e)))?;
                        out.push((k.clone(), self.value_to_json_value(&v)?));
                    }
                    Ok(JsonValue::Object(out))
                }
                _ => Err(VmError::RuntimeError(
                    "json.stringify expects an object".to_string(),
                )),
            },
            Value::Function(_) | Value::BuiltIn(_) | Value::Closure { .. } | Value::Task(_) => {
                Err(VmError::RuntimeError(
                    "Cannot stringify function (no JSON representation)".to_string(),
                ))
            }
        }
    }

    /// Convert a list of Values into a heap-backed list value.
    fn values_to_list(&mut self, values: Vec<Value>) -> Result<Value, VmError> {
        let mut items = Vec::with_capacity(values.len());
        for v in values {
            items.push(v.heap_object_for_element(&mut self.heap));
        }
        let list_ref = self.heap.allocate(HeapObject::list(items));
        self.gc.on_allocation();
        self.add_live_ref(list_ref.clone());
        Ok(Value::List(list_ref))
    }

    fn execute_try_push(&mut self, catch: Option<usize>, finally: Option<usize>) -> Result<(), VmError> {
        let frame_depth = self.call_stack.len();
        let scope_depth = self
            .call_stack
            .last()
            .map(|f| f.scopes.len())
            .unwrap_or(0);
        self.handlers.push(TryHandler {
            catch_pc: catch,
            finally_pc: finally,
            scope_depth,
            stack_depth: self.stack.len(),
            frame_depth,
            phase: TryPhase::TryBody,
        });
        Ok(())
    }

    fn execute_try_pop(&mut self) -> Result<(), VmError> {
        self.handlers.pop();
        Ok(())
    }

    fn execute_finally_end(&mut self) -> Result<(), VmError> {
        if let Some(msg) = self.pending_error.take() {
            return self.raise_runtime_error(msg);
        }
        if let Some(val) = self.pending_return.take() {
            return self.complete_pending_return(val);
        }
        Ok(())
    }

    fn raise_runtime_error(&mut self, msg: String) -> Result<(), VmError> {
        loop {
            // Discard stale handlers from already-popped frames
            while let Some(h) = self.handlers.last() {
                if h.frame_depth > self.call_stack.len() {
                    self.handlers.pop();
                } else {
                    break;
                }
            }

            // Task boundary (§108): an error raised inside an async task body
            // is handled by a try/catch inside that task. If no handler exists
            // inside the task frame, the task fails and (if awaited) the error
            // is re-raised at the await point.
            if let Some(frame_idx) = self.call_stack.iter().rposition(|f| f.task_id.is_some()) {
                let tid = self.call_stack[frame_idx].task_id.unwrap();
                // Handlers pushed inside the task have frame_depth >= frame_idx + 1.
                let handler_in_task = self
                    .handlers
                    .last()
                    .map(|h| h.frame_depth > frame_idx)
                    .unwrap_or(false);
                if !handler_in_task {
                    return self.fail_task(tid, msg);
                }
            }

            let Some(handler) = self.handlers.last().cloned() else {
                return Err(VmError::RuntimeError(msg));
            };

            match handler.phase {
                TryPhase::TryBody => {
                    if let Some(catch_pc) = handler.catch_pc {
                        // Enter catch: keep handler in Catch phase
                        self.unwind_to_handler(&handler);
                        if let Some(frame) = self.handlers.last_mut() {
                            frame.phase = TryPhase::Catch;
                        }
                        self.stack.push(Value::Str(msg));
                        self.track_stack_top();
                        self.pc = catch_pc;
                        return Ok(());
                    } else if let Some(finally_pc) = handler.finally_pc {
                        self.handlers.pop();
                        self.unwind_to_handler(&handler);
                        self.pending_error = Some(msg);
                        self.pc = finally_pc;
                        return Ok(());
                    } else {
                        self.handlers.pop();
                        // fall through: try outer handler with same msg
                    }
                }
                TryPhase::Catch => {
                    // Error inside catch: go to finally if present, else outer
                    self.handlers.pop();
                    if let Some(finally_pc) = handler.finally_pc {
                        self.unwind_to_handler(&handler);
                        self.pending_error = Some(msg);
                        self.pc = finally_pc;
                        return Ok(());
                    }
                    // continue loop with msg for outer handler
                }
            }

            // Suppress unused warning path — msg continues loop
            let _ = &msg;
        }
    }

    fn unwind_to_handler(&mut self, handler: &TryHandler) {
        // Unwind call frames down to the handler's frame
        while self.call_stack.len() > handler.frame_depth {
            if let Some(frame) = self.call_stack.pop() {
                if let Some(instrs) = frame.return_instructions {
                    self.program.instructions = instrs;
                }
            }
        }
        // Unwind scopes in current frame
        if let Some(frame) = self.call_stack.last_mut() {
            while frame.scopes.len() > handler.scope_depth && frame.scopes.len() > 1 {
                frame.scopes.pop();
            }
        }
        // Unwind value stack
        if self.stack.len() > handler.stack_depth {
            self.stack.truncate(handler.stack_depth);
        }
    }

    fn complete_pending_return(&mut self, return_value: Value) -> Result<(), VmError> {
        // Discard handlers belonging to the frame being returned from
        let frame_depth = self.call_stack.len();
        self.handlers
            .retain(|h| h.frame_depth < frame_depth);

        if self.call_stack.is_empty() {
            self.stack.push(return_value);
            self.track_stack_top();
            self.state = ExecutionState::Completed;
            return Ok(());
        }

        let frame = self.call_stack.pop().unwrap();
        self.pc = frame.return_pc;

        if let Some(instructions) = frame.return_instructions {
            self.program.instructions = instructions;
        }

        // Also discard handlers that were in the popped frame
        let new_depth = self.call_stack.len();
        self.handlers.retain(|h| h.frame_depth <= new_depth);

        if let Some(tid) = frame.task_id {
            // Base frame of a task context: the body finished (§106, §107).
            // The context is discarded, dependents are woken, and any awaiter
            // resolves its await when the scheduler resumes it (§107).
            self.finish_task(tid, &return_value)?;
        } else {
            self.stack.push(return_value);
            self.track_stack_top();
        }
        Ok(())
    }

    fn execute_return(&mut self) -> Result<(), VmError> {
        let return_value = if self.stack.is_empty() {
            Value::Null
        } else {
            self.stack.pop().unwrap()
        };

        // If returning through a try with finally, run finally first (§43)
        let frame_depth = self.call_stack.len();
        if let Some(pos) = self
            .handlers
            .iter()
            .rposition(|h| h.frame_depth == frame_depth && h.finally_pc.is_some())
        {
            let handler = self.handlers.remove(pos);
            self.handlers.truncate(pos);
            // Unwind scopes to try boundary, keep frame for finally
            if let Some(frame) = self.call_stack.last_mut() {
                while frame.scopes.len() > handler.scope_depth && frame.scopes.len() > 1 {
                    frame.scopes.pop();
                }
            }
            if self.stack.len() > handler.stack_depth {
                self.stack.truncate(handler.stack_depth);
            }
            self.pending_return = Some(return_value);
            self.pc = handler.finally_pc.unwrap();
            return Ok(());
        }

        // Discard any remaining handlers for this frame (no finally)
        self.handlers.retain(|h| h.frame_depth < frame_depth);

        if self.call_stack.is_empty() {
            self.stack.push(return_value);
            self.track_stack_top();
            self.state = ExecutionState::Completed;
            return Ok(());
        }

        let frame = self.call_stack.pop().unwrap();
        self.pc = frame.return_pc;

        if let Some(instructions) = frame.return_instructions {
            self.program.instructions = instructions;
        }

        if let Some(tid) = frame.task_id {
            // Base frame of a task context: the body finished (§106, §107).
            // The context is discarded, dependents are woken, and any awaiter
            // resolves its await when the scheduler resumes it (§107).
            self.finish_task(tid, &return_value)?;
        } else {
            self.stack.push(return_value);
            self.track_stack_top();
        }

        Ok(())
    }

    fn execute_make_closure(
        &mut self,
        func_idx: usize,
        captured: &[(String, bool)],
    ) -> Result<(), VmError> {
        let _func_value = self.pop_stack("make closure")?;

        // Check if current frame already has a shared scope env for closures
        let shared_env = if let Some(frame) = self.call_stack.last() {
            frame.scope_env.clone()
        } else {
            None
        };

        let env_ref = if let Some(shared) = shared_env {
            // Reuse existing shared env - populate any missing captured vars
            for (name, is_const) in captured {
                let already_has = match self.heap.get_env_binding(&shared, name) {
                    Ok(Some(_)) => true,
                    _ => false,
                };
                if !already_has {
                    let value = self.execute_load_variable_deep(name)?;
                    let heap_val = value.heap_object_for_element(&mut self.heap);
                    self.add_live_ref(heap_val.clone());
                    self.heap
                        .set_env_binding(&shared, name.clone(), heap_val, *is_const)
                        .map_err(|_| {
                            VmError::RuntimeError(format!("Failed to set env binding '{}'", name))
                        })?;
                }
            }
            shared
        } else {
            // Create new shared env by walking up the call stack
            let mut env_bindings: Vec<EnvBinding> = Vec::new();
            for (name, is_const) in captured {
                let value = self.execute_load_variable_deep(name)?;
                let heap_val = value.heap_object_for_element(&mut self.heap);
                self.add_live_ref(heap_val.clone());
                env_bindings.push(EnvBinding {
                    name: name.clone(),
                    value: heap_val,
                    is_const: *is_const,
                });
            }
            let new_env = self.heap.allocate(HeapObject::Environment(env_bindings));
            self.gc.on_allocation();
            self.add_live_ref(new_env.clone());
            // Store in current frame's scope_env for reuse by other closures
            if let Some(frame) = self.call_stack.last_mut() {
                frame.scope_env = Some(new_env.clone());
            }
            new_env
        };

        // Get param_defaults from function info
        let param_defaults = if func_idx < self.program.functions.len() {
            self.program.functions[func_idx].param_defaults.clone()
        } else {
            Vec::new()
        };

        let closure = Value::Closure {
            function_index: func_idx,
            env: env_ref,
            param_defaults,
        };

        self.stack.push(closure);
        self.track_stack_top();

        Ok(())
    }

    fn execute_load_variable_deep(&self, name: &str) -> Result<Value, VmError> {
        // Walk up the call stack to find captured variables from ancestor scopes
        for frame in self.call_stack.iter().rev() {
            for scope in frame.scopes.iter().rev() {
                if let Some(binding) = scope.get(name) {
                    return Ok(binding.value.clone());
                }
            }
            if let Some(ref env_ref) = frame.captured_env {
                if let Ok(Some(heap_ref)) = self.heap.get_env_binding(env_ref, name) {
                    let value = Value::from_heap_ref(&heap_ref, &self.heap).map_err(|_| {
                        VmError::RuntimeError(format!("Failed to convert heap value for '{}'", name))
                    })?;
                    return Ok(value);
                }
            }
        }

        // Fall back to the shared global scope (builtins + top-level vars).
        if let Some(binding) = self.global_scope.get(name) {
            return Ok(binding.value.clone());
        }

        Err(VmError::RuntimeError(format!(
            "Undefined variable '{}'",
            name
        )))
    }

    fn execute_load_variable(&self, name: &str) -> Result<Value, VmError> {
        if let Some(frame) = self.call_stack.last() {
            for scope in frame.scopes.iter().rev() {
                if let Some(binding) = scope.get(name) {
                    return Ok(binding.value.clone());
                }
            }

            if let Some(ref env_ref) = frame.captured_env {
                if let Ok(Some(heap_ref)) = self.heap.get_env_binding(env_ref, name) {
                    let value = Value::from_heap_ref(&heap_ref, &self.heap).map_err(|_| {
                        VmError::RuntimeError(format!("Failed to convert heap value for '{}'", name))
                    })?;
                    return Ok(value);
                }
            }
        }

        // Fall back to the shared global scope (builtins + top-level vars).
        if let Some(binding) = self.global_scope.get(name) {
            return Ok(binding.value.clone());
        }

        Err(VmError::RuntimeError(format!(
            "Undefined variable '{}'",
            name
        )))
    }

    fn lookup_in_env(&self, env_ref: &HeapRefHandle, name: &str) -> Result<Value, VmError> {
        let obj = self.heap.get(env_ref).map_err(|_| {
            VmError::RuntimeError("Invalid environment reference".to_string())
        })?;

        match obj {
            HeapObject::Environment(bindings) => {
                for b in bindings {
                    if b.name == name {
                        return Ok(Value::from_heap_ref(&b.value, &self.heap).map_err(|_| {
                            VmError::RuntimeError("Failed to convert heap value".to_string())
                        })?);
                    }
                }
                Err(VmError::RuntimeError(format!(
                    "Undefined captured variable '{}'",
                    name
                )))
            }
            _ => Err(VmError::RuntimeError(
                "Invalid environment".to_string(),
            )),
        }
    }

    fn execute_load_captured(&mut self, name: &str) -> Result<(), VmError> {
        let found = if let Some(frame) = self.call_stack.last() {
            if let Some(ref env_ref) = frame.captured_env {
                Some(self.lookup_in_env(env_ref, name)?)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(value) = found {
            self.stack.push(value);
            self.track_stack_top();
            return Ok(());
        }

        Err(VmError::RuntimeError(format!(
            "Undefined captured variable '{}'",
            name
        )))
    }

    fn execute_store_captured(&mut self, name: &str) -> Result<(), VmError> {
        let value = self.pop_stack("store captured")?;

        let env_ref = if let Some(frame) = self.call_stack.last() {
            frame.captured_env.clone()
        } else {
            None
        };

        if let Some(ref env_ref) = env_ref {
            // Const check
            if let Ok(Some(true)) = self.heap.get_env_binding_const(env_ref, name) {
                return Err(VmError::RuntimeError(format!(
                    "Cannot assign to const '{}'",
                    name
                )));
            }
            self.store_in_env(env_ref, name, value.clone())?;
            // Keep non-live Binding.value in ancestor scopes in sync (§66)
            // so same-module LoadLocal sees mutations without a live ref.
            for frame in self.call_stack.iter_mut() {
                for scope in frame.scopes.iter_mut() {
                    if let Some(b) = scope.get_mut(name) {
                        if b.live.is_none() {
                            b.value = value.clone();
                        }
                    }
                }
            }
            return Ok(());
        }

        Err(VmError::RuntimeError(format!(
            "Cannot store to captured variable '{}'",
            name
        )))
    }

    fn store_in_env(
        &mut self,
        env_ref: &HeapRefHandle,
        name: &str,
        value: Value,
    ) -> Result<(), VmError> {
        let heap_val = value.heap_object_for_element(&mut self.heap);
        self.add_live_ref(heap_val.clone());

        // If storing into a const binding and value is a collection, mark const
        let is_const_binding = matches!(
            self.heap.get_env_binding_const(env_ref, name),
            Ok(Some(true))
        );

        if let Ok(HeapObject::Environment(bindings)) = self.heap.get_mut(env_ref) {
            for b in bindings.iter_mut() {
                if b.name == name {
                    b.value = heap_val;
                    if is_const_binding {
                        // mark the collection const
                    }
                    return Ok(());
                }
            }
            bindings.push(EnvBinding {
                name: name.to_string(),
                value: heap_val,
                is_const: false,
            });
            if is_const_binding {
                // new binding shouldn't be const
            }
            return Ok(());
        }

        Err(VmError::RuntimeError(format!(
            "Failed to store captured variable '{}'",
            name
        )))
    }

    fn execute_make_list(&mut self, size: usize) -> Result<(), VmError> {
        if self.stack.len() < size {
            return Err(VmError::StackUnderflow(format!(
                "MakeList needs {} elements, stack has {}",
                size,
                self.stack.len()
            )));
        }

        let mut items = Vec::with_capacity(size);
        for _ in 0..size {
            let v = self.pop_stack("make list")?;
            let r = v.heap_object_for_element(&mut self.heap);
            self.gc.on_allocation();
            items.push(r);
        }
        items.reverse();

        let list_ref = self.heap.allocate(HeapObject::list(items));
        self.gc.on_allocation();
        self.add_live_ref(list_ref.clone());

        self.stack.push(Value::List(list_ref));
        self.maybe_gc();
        Ok(())
    }

    fn execute_make_object(&mut self, keys: &[String]) -> Result<(), VmError> {
        if self.stack.len() < keys.len() {
            return Err(VmError::StackUnderflow(format!(
                "MakeObject needs {} values, stack has {}",
                keys.len(),
                self.stack.len()
            )));
        }

        // Values are pushed in order, so pop in reverse to match keys
        let mut values = Vec::with_capacity(keys.len());
        for _ in 0..keys.len() {
            values.push(self.pop_stack("make object")?);
        }
        values.reverse();

        let mut props = Vec::with_capacity(keys.len());
        for (key, value) in keys.iter().zip(values) {
            let r = value.heap_object_for_element(&mut self.heap);
            self.gc.on_allocation();
            props.push((key.clone(), r));
        }

        let obj_ref = self.heap.allocate(HeapObject::object(props));
        self.gc.on_allocation();
        self.add_live_ref(obj_ref.clone());

        self.stack.push(Value::Object(obj_ref));
        self.maybe_gc();
        Ok(())
    }

    fn execute_get_index(&mut self) -> Result<(), VmError> {
        let index = self.pop_stack("get index")?;
        let object = self.pop_stack("get index")?;

        match (&object, &index) {
            (Value::List(list_ref), Value::Number(n)) => {
                let idx = *n as usize;
                if *n < 0.0 || *n != idx as f64 {
                    return Err(VmError::RuntimeError(format!(
                        "List index {} out of bounds",
                        n
                    )));
                }
                match self.heap.get(list_ref) {
                    Ok(HeapObject::List { items, .. }) => {
                        if idx >= items.len() {
                            return Err(VmError::RuntimeError(format!(
                                "List index {} out of bounds (length {})",
                                n,
                                items.len()
                            )));
                        }
                        let elem = items[idx].clone();
                        let value = Value::from_heap_ref(&elem, &self.heap)
                            .map_err(|_| VmError::RuntimeError("Failed to convert element".to_string()))?;
                        self.stack.push(value);
                        self.track_stack_top();
                        Ok(())
                    }
                    _ => Err(VmError::RuntimeError(
                        "Cannot index non-list value".to_string(),
                    )),
                }
            }
            (Value::Str(s), Value::Number(n)) => {
                let idx = *n as usize;
                if *n < 0.0 || *n != idx as f64 || idx >= s.len() {
                    return Err(VmError::RuntimeError(format!(
                        "String index {} out of bounds (length {})",
                        n,
                        s.len()
                    )));
                }
                let ch = s.chars().nth(idx).unwrap();
                self.stack.push(Value::Str(ch.to_string()));
                self.track_stack_top();
                Ok(())
            }
            (Value::Object(obj_ref), Value::Str(key)) => match self.heap.get(obj_ref) {
                Ok(HeapObject::Object { props, .. }) => {
                    for (k, vr) in props {
                        if k == key {
                            let value = Value::from_heap_ref(vr, &self.heap).map_err(|_| {
                                VmError::RuntimeError("Failed to convert property".to_string())
                            })?;
                            self.stack.push(value);
                            self.track_stack_top();
                            return Ok(());
                        }
                    }
                    Err(VmError::RuntimeError(format!("Key '{}' not found", key)))
                }
                _ => Err(VmError::RuntimeError(
                    "Cannot index non-object value".to_string(),
                )),
            },
            _ => Err(VmError::RuntimeError(format!(
                "Cannot index {:?} with {:?}",
                object, index
            ))),
        }
    }

    fn execute_set_index(&mut self) -> Result<(), VmError> {
        let value = self.pop_stack("set index")?;
        let index = self.pop_stack("set index")?;
        let object = self.pop_stack("set index")?;

        match (&object, &index) {
            (Value::List(list_ref), Value::Number(n)) => {
                let idx = *n as usize;
                if *n < 0.0 || *n != idx as f64 {
                    return Err(VmError::RuntimeError(format!(
                        "List index {} out of bounds",
                        n
                    )));
                }
                let heap_val = value.heap_object_for_element(&mut self.heap);
                match self.heap.get_mut(list_ref) {
                    Ok(HeapObject::List { items, is_const }) => {
                        if *is_const {
                            return Err(VmError::RuntimeError(
                                "Cannot modify const list".to_string(),
                            ));
                        }
                        if idx >= items.len() {
                            return Err(VmError::RuntimeError(format!(
                                "List index {} out of bounds (length {})",
                                n,
                                items.len()
                            )));
                        }
                        items[idx] = heap_val;
                    }
                    _ => {
                        return Err(VmError::RuntimeError(
                            "Cannot set index on non-list".to_string(),
                        ))
                    }
                }
                self.stack.push(value);
                Ok(())
            }
            (Value::Object(obj_ref), Value::Str(key)) => {
                let heap_val = value.heap_object_for_element(&mut self.heap);
                match self.heap.get_mut(obj_ref) {
                    Ok(HeapObject::Object { props, is_const }) => {
                        if *is_const {
                            return Err(VmError::RuntimeError(
                                "Cannot modify const object".to_string(),
                            ));
                        }
                        let mut found = false;
                        for (k, v) in props.iter_mut() {
                            if k == key {
                                *v = heap_val.clone();
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            props.push((key.clone(), heap_val));
                        }
                    }
                    _ => {
                        return Err(VmError::RuntimeError(
                            "Cannot set index on non-object".to_string(),
                        ))
                    }
                }
                self.stack.push(value);
                Ok(())
            }
            (Value::Str(_), _) => Err(VmError::RuntimeError(
                "String cannot be mutated".to_string(),
            )),
            _ => Err(VmError::RuntimeError(format!(
                "Cannot set index on {:?}",
                object
            ))),
        }
    }

    fn execute_get_property(&mut self, name: &str) -> Result<(), VmError> {
        let object = self.pop_stack("get property")?;

        match &object {
            Value::Object(obj_ref) => match self.heap.get(obj_ref) {
                Ok(HeapObject::Object { props, .. }) => {
                    for (k, vr) in props {
                        if k == name {
                            let value = Value::from_heap_ref(vr, &self.heap).map_err(|_| {
                                VmError::RuntimeError("Failed to convert property".to_string())
                            })?;
                            self.stack.push(value);
                            self.track_stack_top();
                            return Ok(());
                        }
                    }
                    Err(VmError::RuntimeError(format!(
                        "Property '{}' not found",
                        name
                    )))
                }
                _ => Err(VmError::RuntimeError(format!(
                    "Cannot access property '{}' on non-object",
                    name
                ))),
            },
            // Namespace method resolution: print.err, string.*, list.*, object.*
            // Soft CALL_NATIVE path (Bytecode.md §145): BuiltIn + GetProperty
            // yields a method BuiltIn that Call dispatches via call_builtin.
            Value::BuiltIn(base) => {
                let known: &[&str] = match base.as_str() {
                    "print" => &["err"],
                    "string" => &[
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
                    "list" => &["add", "remove", "length"],
                    "object" => &["remove"],
                    "color" => &["red", "green", "yellow", "blue", "bold"],
                    "file" => &["read", "write", "exists", "remove", "mkdir"],
                    "system" => &["args", "env", "os", "arch"],
                    "json" => &["parse", "stringify"],
                    "task" => &["state", "all", "race", "cancel", "wait", "timeout", "sleep"],
                    "http" => &["get", "post", "put", "patch", "delete"],
                    _ => &[],
                };
                if known.contains(&name) {
                    self.stack
                        .push(Value::BuiltIn(format!("{}.{}", base, name)));
                    Ok(())
                } else {
                    Err(VmError::RuntimeError(format!(
                        "Property '{}' not found",
                        name
                    )))
                }
            }
            _ => Err(VmError::RuntimeError(format!(
                "Cannot access property '{}' on {:?}",
                name, object
            ))),
        }
    }

    fn execute_set_property(&mut self, name: &str) -> Result<(), VmError> {
        let value = self.pop_stack("set property")?;
        let object = self.pop_stack("set property")?;

        match &object {
            Value::Object(obj_ref) => {
                let heap_val = value.heap_object_for_element(&mut self.heap);
                match self.heap.get_mut(obj_ref) {
                    Ok(HeapObject::Object { props, is_const }) => {
                        if *is_const {
                            return Err(VmError::RuntimeError(
                                "Cannot modify const object".to_string(),
                            ));
                        }
                        let mut found = false;
                        for (k, v) in props.iter_mut() {
                            if k == name {
                                *v = heap_val.clone();
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            props.push((name.to_string(), heap_val));
                        }
                    }
                    _ => {
                        return Err(VmError::RuntimeError(
                            "Cannot set property on non-object".to_string(),
                        ))
                    }
                }
                self.stack.push(value);
                Ok(())
            }
            _ => Err(VmError::RuntimeError(format!(
                "Cannot set property '{}' on {:?}",
                name, object
            ))),
        }
    }

    fn execute_print(&mut self) -> Result<(), VmError> {
        let value = self.pop_stack("print")?;
        let output = value.format_value(&self.heap);
        self.output.push(output);
        Ok(())
    }

    fn pop_stack(&mut self, context: &str) -> Result<Value, VmError> {
        self.stack.pop().ok_or_else(|| {
            VmError::StackUnderflow(format!(
                "{}: stack is empty",
                context
            ))
        })
    }

    fn peek_stack(&self, context: &str) -> Result<Value, VmError> {
        self.stack.last().cloned().ok_or_else(|| {
            VmError::StackUnderflow(format!(
                "{}: stack is empty",
                context
            ))
        })
    }
}

pub fn run_bytecode(program: BytecodeProgram) -> Result<Vec<String>, VmError> {
    let mut vm = ZeroVM::new(program);
    vm.run()?;
    Ok(vm.get_output().to_vec())
}

fn type_name(value: &Value, _heap: &Heap) -> &'static str {
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
fn expect_string_arg<'a>(args: &'a [Value], idx: usize, fname: &str) -> Result<&'a str, VmError> {
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
fn expect_list_arg(args: &[Value], idx: usize, fname: &str) -> Result<HeapRefHandle, VmError> {
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
fn expect_task_arg(args: &[Value], idx: usize, fname: &str) -> Result<usize, VmError> {
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
fn duration_ms(value: &Value, fname: &str) -> Result<u128, VmError> {
    match value {
        Value::Number(n) if n.is_finite() && *n >= 0.0 && n.fract() == 0.0 => Ok(*n as u128),
        _ => Err(VmError::RuntimeError(format!(
            "{} time must be a non-negative integer number of milliseconds",
            fname
        ))),
    }
}

/// Percent-encode one query component (§59: everything URL-encoded;
/// RFC 3986 unreserved characters are kept readable).
fn percent_encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Append encoded query pairs to a URL (§59): a URL that already has a
/// query keeps it (appended with `&`, not replaced); fragments stay last.
fn append_query(url: &str, pairs: &[String]) -> String {
    if pairs.is_empty() {
        return url.to_string();
    }
    let (base, frag) = match url.find('#') {
        Some(i) => (&url[..i], &url[i..]),
        None => (url, ""),
    };
    let sep = if base.contains('?') {
        if base.ends_with('?') || base.ends_with('&') {
            ""
        } else {
            "&"
        }
    } else {
        "?"
    };
    format!("{}{}{}{}", base, sep, pairs.join("&"), frag)
}

/// Chain an error and its sources into one message (ureq's Display prints
/// only the top-level variant).
fn error_chain(e: &dyn std::error::Error) -> String {
    let mut s = e.to_string();
    let mut src = e.source();
    while let Some(cause) = src {
        s.push_str(": ");
        s.push_str(&cause.to_string());
        src = cause.source();
    }
    s
}

/// Perform one blocking HTTP exchange on a worker thread (§50).
///
/// - 4xx/5xx are ordinary responses (`http_status_as_error(false)` per §50.1)
/// - `timeout` (§50.1) bounds the whole operation in real time
/// - response bodies are always read as (lossy UTF-8) strings — no JSON
///   parsing at the HTTP layer (§50, §51)
fn http_perform(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&str>,
    timeout_ms: Option<u64>,
) -> Result<HttpOutcome, String> {
    let timeout = timeout_ms.map(Duration::from_millis);

    // Build + run a request, applying user headers and §50.1 options.
    // GET/DELETE builders have no body unless force_send_body() is used.
    let resp = match method {
        "GET" => {
            let mut rb = ureq::get(url);
            for (k, v) in headers {
                rb = rb.header(k.as_str(), v.as_str());
            }
            let rb = rb
                .config()
                .http_status_as_error(false)
                .timeout_global(timeout)
                .build();
            match body {
                Some(b) => rb.force_send_body().send(b),
                None => rb.call(),
            }
        }
        "DELETE" => {
            let mut rb = ureq::delete(url);
            for (k, v) in headers {
                rb = rb.header(k.as_str(), v.as_str());
            }
            let rb = rb
                .config()
                .http_status_as_error(false)
                .timeout_global(timeout)
                .build();
            match body {
                Some(b) => rb.force_send_body().send(b),
                None => rb.call(),
            }
        }
        "POST" => {
            let mut rb = ureq::post(url);
            for (k, v) in headers {
                rb = rb.header(k.as_str(), v.as_str());
            }
            let rb = rb
                .config()
                .http_status_as_error(false)
                .timeout_global(timeout)
                .build();
            match body {
                Some(b) => rb.send(b),
                None => rb.send_empty(),
            }
        }
        "PUT" => {
            let mut rb = ureq::put(url);
            for (k, v) in headers {
                rb = rb.header(k.as_str(), v.as_str());
            }
            let rb = rb
                .config()
                .http_status_as_error(false)
                .timeout_global(timeout)
                .build();
            match body {
                Some(b) => rb.send(b),
                None => rb.send_empty(),
            }
        }
        "PATCH" => {
            let mut rb = ureq::patch(url);
            for (k, v) in headers {
                rb = rb.header(k.as_str(), v.as_str());
            }
            let rb = rb
                .config()
                .http_status_as_error(false)
                .timeout_global(timeout)
                .build();
            match body {
                Some(b) => rb.send(b),
                None => rb.send_empty(),
            }
        }
        other => return Err(format!("unsupported HTTP method '{}'", other)),
    };

    let mut resp = resp.map_err(|e| match &e {
        ureq::Error::Timeout(_) => match timeout_ms {
            Some(ms) => format!("request timed out after {} ms", ms),
            None => "request timed out".to_string(),
        },
        _ => error_chain(&e),
    })?;

    let status = resp.status().as_u16();
    let mut out_headers = Vec::with_capacity(resp.headers().len());
    for (name, value) in resp.headers().iter() {
        out_headers.push((
            name.as_str().to_string(),
            String::from_utf8_lossy(value.as_bytes()).into_owned(),
        ));
    }
    let resp_body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("failed to read response body: {}", error_chain(&e)))?;
    Ok(HttpOutcome {
        status,
        headers: out_headers,
        body: resp_body,
    })
}

/// Require `args[idx]` to be an object.
fn expect_object_arg(args: &[Value], idx: usize, fname: &str) -> Result<HeapRefHandle, VmError> {
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

fn raw_eq(a: &Value, b: &Value) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{compile, Constant, FunctionInfo, Opcode};
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn compile_source(source: &str) -> BytecodeProgram {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse().unwrap();
        compile(&program).unwrap()
    }

    fn run_source(source: &str) -> Result<Vec<String>, VmError> {
        let bc = compile_source(source);
        run_bytecode(bc)
    }

    #[test]
    fn test_empty_program() {
        let bc = BytecodeProgram {
            constants: vec![],
            instructions: vec![Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(*vm.get_state(), ExecutionState::Completed);
    }

    #[test]
    fn test_validate_empty() {
        let bc = BytecodeProgram {
            constants: vec![],
            instructions: vec![],
            functions: vec![],
            function_bodies: vec![],
        };
        let vm = ZeroVM::new(bc);
        assert!(vm.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_constant_index() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(5)],
            functions: vec![],
            function_bodies: vec![],
        };
        let vm = ZeroVM::new(bc);
        assert!(vm.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_jump_target() {
        let bc = BytecodeProgram {
            constants: vec![],
            instructions: vec![Opcode::Jump(10)],
            functions: vec![],
            function_bodies: vec![],
        };
        let vm = ZeroVM::new(bc);
        assert!(vm.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_function_entry() {
        let bc = BytecodeProgram {
            constants: vec![],
            instructions: vec![Opcode::Halt],
            functions: vec![FunctionInfo {
                name: "test".to_string(),
                params: vec![],
                param_defaults: vec![],
                entry_point: 10,
                local_count: 0,
                captured_vars: vec![],
                is_async: false,
            }],
            function_bodies: vec![],
        };
        let vm = ZeroVM::new(bc);
        assert!(vm.validate().is_err());
    }

    #[test]
    fn test_validate_valid() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Pop, Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
        };
        let vm = ZeroVM::new(bc);
        assert!(vm.validate().is_ok());
    }

    #[test]
    fn test_stack_push_pop() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(42.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack.len(), 1);
        assert_eq!(vm.stack[0], Value::Number(42.0));
    }

    #[test]
    fn test_stack_underflow_pop() {
        let bc = BytecodeProgram {
            constants: vec![],
            instructions: vec![Opcode::Pop],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        let result = vm.run();
        assert!(result.is_err());
    }

    #[test]
    fn test_stack_dup() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Dup, Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack.len(), 2);
        assert_eq!(vm.stack[0], Value::Number(10.0));
        assert_eq!(vm.stack[1], Value::Number(10.0));
    }

    #[test]
    fn test_stack_swap() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0), Constant::Number(20.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Swap,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(20.0));
        assert_eq!(vm.stack[1], Value::Number(10.0));
    }

    #[test]
    fn test_stack_swap_underflow() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Swap],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_constant_access() {
        let bc = BytecodeProgram {
            constants: vec![
                Constant::Number(10.0),
                Constant::Str("hello".to_string()),
                Constant::Boolean(true),
                Constant::Null,
            ],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::LoadConst(2),
                Opcode::LoadConst(3),
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(10.0));
        assert_eq!(vm.stack[1], Value::Str("hello".to_string()));
        assert_eq!(vm.stack[2], Value::Boolean(true));
        assert_eq!(vm.stack[3], Value::Null);
    }

    #[test]
    fn test_invalid_constant_index_runtime() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(5)],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_add_numbers() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0), Constant::Number(20.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Add,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(30.0));
    }

    #[test]
    fn test_subtract_numbers() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0), Constant::Number(3.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Sub,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(7.0));
    }

    #[test]
    fn test_multiply_numbers() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(3.0), Constant::Number(4.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Mul,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(12.0));
    }

    #[test]
    fn test_divide_numbers() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0), Constant::Number(2.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Div,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(5.0));
    }

    #[test]
    fn test_divide_by_zero() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0), Constant::Number(0.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Div,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_modulo_numbers() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0), Constant::Number(3.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Mod,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(1.0));
    }

    #[test]
    fn test_modulo_by_zero() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0), Constant::Number(0.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Mod,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_power_numbers() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(2.0), Constant::Number(3.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Pow,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(8.0));
    }

    #[test]
    fn test_negate() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Neg, Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(-10.0));
    }

    #[test]
    fn test_positive() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Pos, Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(10.0));
    }

    #[test]
    fn test_not() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Boolean(true)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Not, Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Boolean(false));
    }

    #[test]
    fn test_not_on_non_boolean() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Not],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_equality_equal() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0), Constant::Number(10.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Eq,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Boolean(true));
    }

    #[test]
    fn test_equality_not_equal() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0), Constant::Number(20.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Eq,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Boolean(false));
    }

    #[test]
    fn test_neq() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0), Constant::Number(20.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Neq,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Boolean(true));
    }

    #[test]
    fn test_strict_eq_same_type() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0), Constant::Number(10.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::RawEq,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Boolean(true));
    }

    #[test]
    fn test_strict_eq_different_type() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0), Constant::Str("10".to_string())],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::RawEq,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Boolean(true));
    }

    #[test]
    fn test_eq_different_type() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0), Constant::Str("10".to_string())],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Eq,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Boolean(false));
    }

    #[test]
    fn test_gt() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0), Constant::Number(5.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Gt,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Boolean(true));
    }

    #[test]
    fn test_lt() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(5.0), Constant::Number(10.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Lt,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Boolean(true));
    }

    #[test]
    fn test_gte() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0), Constant::Number(10.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Gte,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Boolean(true));
    }

    #[test]
    fn test_lte() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(5.0), Constant::Number(10.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Lte,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Boolean(true));
    }

    #[test]
    fn test_and() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Boolean(true), Constant::Boolean(false)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::And,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Boolean(false));
    }

    #[test]
    fn test_or() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Boolean(true), Constant::Boolean(false)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Or,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Boolean(true));
    }

    #[test]
    fn test_and_non_boolean() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0), Constant::Boolean(true)],
            instructions: vec![Opcode::LoadConst(0), Opcode::LoadConst(1), Opcode::And],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_jump() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0), Constant::Number(20.0)],
            instructions: vec![
                Opcode::Jump(2),
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(20.0));
    }

    #[test]
    fn test_jump_if_false() {
        let bc = BytecodeProgram {
            constants: vec![
                Constant::Boolean(false),
                Constant::Number(10.0),
                Constant::Number(20.0),
            ],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::JumpIfFalse(3),
                Opcode::LoadConst(1),
                Opcode::Pop,
                Opcode::LoadConst(2),
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(20.0));
    }

    #[test]
    fn test_jump_if_true() {
        let bc = BytecodeProgram {
            constants: vec![
                Constant::Boolean(true),
                Constant::Number(10.0),
                Constant::Number(20.0),
            ],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::JumpIfTrue(3),
                Opcode::LoadConst(1),
                Opcode::Pop,
                Opcode::LoadConst(2),
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(20.0));
    }

    #[test]
    fn test_jump_invalid_target() {
        let bc = BytecodeProgram {
            constants: vec![],
            instructions: vec![Opcode::Jump(10)],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_jump_if_false_non_boolean() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::JumpIfFalse(0)],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_variable_store_load() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(42.0)],
            instructions: vec![
                Opcode::DeclareLocal("A".to_string(), false),
                Opcode::LoadConst(0),
                Opcode::StoreLocal("A".to_string()),
                Opcode::LoadLocal("A".to_string()),
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(42.0));
    }

    #[test]
    fn test_variable_undefined() {
        let bc = BytecodeProgram {
            constants: vec![],
            instructions: vec![Opcode::LoadLocal("X".to_string())],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_print() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(42.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Print, Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_output(), &["42"]);
    }

    #[test]
    fn test_print_string() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Str("hello".to_string())],
            instructions: vec![Opcode::LoadConst(0), Opcode::Print, Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_output(), &["hello"]);
    }

    #[test]
    fn test_halt() {
        let bc = BytecodeProgram {
            constants: vec![
                Constant::Number(10.0),
                Constant::Number(20.0),
            ],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::Halt,
                Opcode::LoadConst(1),
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack.len(), 1);
        assert_eq!(vm.stack[0], Value::Number(10.0));
    }

    #[test]
    fn test_make_list() {
        let bc = BytecodeProgram {
            constants: vec![
                Constant::Number(10.0),
                Constant::Number(20.0),
                Constant::Number(30.0),
            ],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::LoadConst(2),
                Opcode::MakeList(3),
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        // List is heap-backed; check structure
        match &vm.stack[0] {
            Value::List(r) => match vm.heap.get(r).unwrap() {
                HeapObject::List { items, .. } => {
                    assert_eq!(items.len(), 3);
                    assert_eq!(
                        Value::from_heap_ref(&items[0], &vm.heap).unwrap(),
                        Value::Number(10.0)
                    );
                    assert_eq!(
                        Value::from_heap_ref(&items[1], &vm.heap).unwrap(),
                        Value::Number(20.0)
                    );
                    assert_eq!(
                        Value::from_heap_ref(&items[2], &vm.heap).unwrap(),
                        Value::Number(30.0)
                    );
                }
                _ => panic!("Expected list on heap"),
            },
            other => panic!("Expected Value::List, got {:?}", other),
        }
    }

    #[test]
    fn test_make_empty_list() {
        let bc = BytecodeProgram {
            constants: vec![],
            instructions: vec![Opcode::MakeList(0), Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        match &vm.stack[0] {
            Value::List(r) => match vm.heap.get(r).unwrap() {
                HeapObject::List { items, .. } => assert_eq!(items.len(), 0),
                _ => panic!("Expected list on heap"),
            },
            other => panic!("Expected Value::List, got {:?}", other),
        }
    }

    #[test]
    fn test_make_object() {
        let bc = BytecodeProgram {
            constants: vec![
                Constant::Str("Kael".to_string()),
                Constant::Number(18.0),
            ],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::MakeObject(vec!["name".to_string(), "age".to_string()]),
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        match &vm.stack[0] {
            Value::Object(r) => match vm.heap.get(r).unwrap() {
                HeapObject::Object { props, .. } => {
                    assert_eq!(props.len(), 2);
                    let name_ref = props.iter().find(|(k, _)| k == "name").unwrap().1.clone();
                    let age_ref = props.iter().find(|(k, _)| k == "age").unwrap().1.clone();
                    assert_eq!(
                        Value::from_heap_ref(&name_ref, &vm.heap).unwrap(),
                        Value::Str("Kael".to_string())
                    );
                    assert_eq!(
                        Value::from_heap_ref(&age_ref, &vm.heap).unwrap(),
                        Value::Number(18.0)
                    );
                }
                _ => panic!("Expected object on heap"),
            },
            other => panic!("Expected Value::Object, got {:?}", other),
        }
    }

    #[test]
    fn test_get_index_list() {
        let bc = BytecodeProgram {
            constants: vec![
                Constant::Number(10.0),
                Constant::Number(20.0),
                Constant::Number(0.0),
            ],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::MakeList(2),
                Opcode::LoadConst(2),
                Opcode::GetIndex,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(10.0));
    }

    #[test]
    fn test_get_index_out_of_bounds() {
        let bc = BytecodeProgram {
            constants: vec![
                Constant::Number(10.0),
                Constant::Number(5.0),
            ],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::MakeList(1),
                Opcode::LoadConst(1),
                Opcode::GetIndex,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_get_property() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Str("Kael".to_string())],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::MakeObject(vec!["name".to_string()]),
                Opcode::GetProperty("name".to_string()),
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Str("Kael".to_string()));
    }

    #[test]
    fn test_get_property_not_found() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Str("Kael".to_string())],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::MakeObject(vec!["name".to_string()]),
                Opcode::GetProperty("missing".to_string()),
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_add_strings() {
        let bc = BytecodeProgram {
            constants: vec![
                Constant::Str("Hello ".to_string()),
                Constant::Str("World".to_string()),
            ],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Add,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Str("Hello World".to_string()));
    }

    #[test]
    fn test_add_null_with_number() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Null, Constant::Number(10.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Add,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(10.0));
    }

    #[test]
    fn test_add_number_with_null() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0), Constant::Null],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Add,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(10.0));
    }

    #[test]
    fn test_add_null_null() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Null, Constant::Null],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Add,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(0.0));
    }

    #[test]
    fn test_negate_null() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Null],
            instructions: vec![Opcode::LoadConst(0), Opcode::Neg, Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.stack[0], Value::Number(0.0));
    }

    #[test]
    fn test_pc_out_of_bounds() {
        let bc = BytecodeProgram {
            constants: vec![],
            instructions: vec![Opcode::Jump(100)],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_call_non_function() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Call(0)],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_call_wrong_arg_count() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(0.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::Call(0),
                Opcode::Halt,
            ],
            functions: vec![FunctionInfo {
                name: "test".to_string(),
                params: vec!["A".to_string()],
                param_defaults: vec![],
                entry_point: 3,
                local_count: 1,
                captured_vars: vec![],
                is_async: false,
            }],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_execution_state_running() {
        let bc = BytecodeProgram {
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(0)],
            functions: vec![],
            function_bodies: vec![],
        };
        let vm = ZeroVM::new(bc);
        assert_eq!(*vm.get_state(), ExecutionState::Running);
    }

    #[test]
    fn test_execution_completed() {
        let bc = BytecodeProgram {
            constants: vec![],
            instructions: vec![Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(*vm.get_state(), ExecutionState::Completed);
    }

    #[test]
    fn test_value_display_number() {
        assert_eq!(format!("{}", Value::Number(42.0)), "42");
        assert_eq!(format!("{}", Value::Number(3.14)), "3.14");
    }

    #[test]
    fn test_value_display_string() {
        assert_eq!(
            format!("{}", Value::Str("hello".to_string())),
            "hello"
        );
    }

    #[test]
    fn test_value_display_boolean() {
        assert_eq!(format!("{}", Value::Boolean(true)), "true");
        assert_eq!(format!("{}", Value::Boolean(false)), "false");
    }

    #[test]
    fn test_value_display_null() {
        assert_eq!(format!("{}", Value::Null), "null");
    }

    #[test]
    fn test_value_display_list() {
        let mut heap = Heap::new();
        let n1 = heap.allocate(HeapObject::Number(1.0));
        let n2 = heap.allocate(HeapObject::Number(2.0));
        let n3 = heap.allocate(HeapObject::Number(3.0));
        let list_ref = heap.allocate(HeapObject::list(vec![n1, n2, n3]));
        let list = Value::List(list_ref);
        assert_eq!(list.format_value(&heap), "[1, 2, 3]");
        // Display without heap shows identity placeholder
        assert!(format!("{}", list).starts_with("<list "));
    }

    #[test]
    fn test_value_display_object() {
        let mut heap = Heap::new();
        let name_val = heap.allocate(HeapObject::Str("Kael".to_string()));
        let obj_ref = heap.allocate(HeapObject::object(vec![("name".to_string(), name_val)]));
        let obj = Value::Object(obj_ref);
        let formatted = obj.format_value(&heap);
        assert!(formatted.contains("name: Kael"));
    }

    #[test]
    fn test_value_display_function() {
        assert_eq!(format!("{}", Value::Function(0)), "<function 0>");
    }

    #[test]
    fn test_value_equality() {
        assert_eq!(Value::Number(10.0), Value::Number(10.0));
        assert_ne!(Value::Number(10.0), Value::Number(20.0));
        assert_eq!(
            Value::Str("hello".to_string()),
            Value::Str("hello".to_string())
        );
        assert_eq!(Value::Boolean(true), Value::Boolean(true));
        assert_eq!(Value::Null, Value::Null);
    }

    // Integration tests: full pipeline source -> lexer -> parser -> compiler -> VM

    #[test]
    fn test_integration_print_number() {
        let output = run_source("print(42)").unwrap();
        assert_eq!(output, vec!["42"]);
    }

    #[test]
    fn test_integration_print_string() {
        let output = run_source(r#"print("hello")"#).unwrap();
        assert_eq!(output, vec!["hello"]);
    }

    #[test]
    fn test_integration_print_boolean() {
        let output = run_source("print(true)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_print_null() {
        let output = run_source("print(null)").unwrap();
        assert_eq!(output, vec!["null"]);
    }

    #[test]
    fn test_integration_arithmetic() {
        let output = run_source("print(10 + 5)").unwrap();
        assert_eq!(output, vec!["15"]);
    }

    #[test]
    fn test_integration_subtract() {
        let output = run_source("print(10 - 3)").unwrap();
        assert_eq!(output, vec!["7"]);
    }

    #[test]
    fn test_integration_multiply() {
        let output = run_source("print(3 * 4)").unwrap();
        assert_eq!(output, vec!["12"]);
    }

    #[test]
    fn test_integration_divide() {
        let output = run_source("print(10 / 2)").unwrap();
        assert_eq!(output, vec!["5"]);
    }

    #[test]
    fn test_integration_modulo() {
        let output = run_source("print(10 % 3)").unwrap();
        assert_eq!(output, vec!["1"]);
    }

    #[test]
    fn test_integration_power() {
        let output = run_source("print(2 ** 3)").unwrap();
        assert_eq!(output, vec!["8"]);
    }

    #[test]
    fn test_integration_precedence() {
        let output = run_source("print(2 + 3 * 4)").unwrap();
        assert_eq!(output, vec!["14"]);
    }

    #[test]
    fn test_integration_string_concat() {
        let output = run_source(r#"print("Hello " + "World")"#).unwrap();
        assert_eq!(output, vec!["Hello World"]);
    }

    #[test]
    fn test_integration_equality() {
        let output = run_source("print(10 == 10)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_equality_false() {
        let output = run_source("print(10 == 20)").unwrap();
        assert_eq!(output, vec!["false"]);
    }

    #[test]
    fn test_integration_not_equal() {
        let output = run_source("print(10 != 20)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_strict_equality() {
        let output = run_source("print(10 === 10)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_strict_equality_cross_type() {
        let output = run_source(r#"print(10 === "10")"#).unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_equality_cross_type() {
        let output = run_source(r#"print(10 == "10")"#).unwrap();
        assert_eq!(output, vec!["false"]);
    }

    #[test]
    fn test_integration_comparison_gt() {
        let output = run_source("print(10 > 5)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_comparison_lt() {
        let output = run_source("print(5 < 10)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_comparison_gte() {
        let output = run_source("print(10 >= 10)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_comparison_lte() {
        let output = run_source("print(5 <= 10)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_boolean_and() {
        let output = run_source("print(true and false)").unwrap();
        assert_eq!(output, vec!["false"]);
    }

    #[test]
    fn test_integration_boolean_or() {
        let output = run_source("print(true or false)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_not() {
        let output = run_source("print(!true)").unwrap();
        assert_eq!(output, vec!["false"]);
    }

    #[test]
    fn test_integration_negate() {
        let output = run_source("print(-10)").unwrap();
        assert_eq!(output, vec!["-10"]);
    }

    #[test]
    fn test_integration_variable_let() {
        let output = run_source("let A = 10\nprint(A)").unwrap();
        assert_eq!(output, vec!["10"]);
    }

    #[test]
    fn test_integration_variable_assign() {
        let output = run_source("let A = 10\nA = 20\nprint(A)").unwrap();
        assert_eq!(output, vec!["20"]);
    }

    #[test]
    fn test_integration_compound_assignment() {
        let output = run_source("let A = 10\nA += 5\nprint(A)").unwrap();
        assert_eq!(output, vec!["15"]);
    }

    #[test]
    fn test_integration_if_else() {
        let output = run_source("if false { print(10) } else { print(20) }").unwrap();
        assert_eq!(output, vec!["20"]);
    }

    #[test]
    fn test_integration_while_loop() {
        let output = run_source("let A = 0\nwhile A < 3 {\n    A += 1\n    print(A)\n}").unwrap();
        assert_eq!(output, vec!["1", "2", "3"]);
    }

    #[test]
    fn test_integration_loop_break() {
        let output = run_source("let A = 0\nloop {\n    A += 1\n    if A == 3 { break }\n    print(A)\n}").unwrap();
        assert_eq!(output, vec!["1", "2"]);
    }

    #[test]
    fn test_integration_list() {
        let output = run_source("let A = [10, 20, 30]\nprint(A[0])\nprint(A[1])\nprint(A[2])").unwrap();
        assert_eq!(output, vec!["10", "20", "30"]);
    }

    #[test]
    fn test_integration_object_access() {
        let output = run_source("let user = { name: \"Kael\" }\nprint(user.name)").unwrap();
        assert_eq!(output, vec!["Kael"]);
    }

    #[test]
    fn test_integration_function_declaration_and_call() {
        let source = "function tambah(A, B) {\n    return A + B\n}\nprint(tambah(10, 20))";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["30"]);
    }

    #[test]
    fn test_integration_function_no_return() {
        let source = "function test() {\n}\nprint(test())";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["null"]);
    }

    #[test]
    fn test_integration_function_bare_return() {
        let source = "function test() {\n    return\n}\nprint(test())";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["null"]);
    }

    #[test]
    fn test_integration_division_by_zero() {
        let result = run_source("print(10 / 0)");
        assert!(result.is_err());
    }

    #[test]
    fn test_integration_string_index() {
        let output = run_source("let s = \"Hello\"\nprint(s[0])").unwrap();
        assert_eq!(output, vec!["H"]);
    }

    #[test]
    fn test_integration_postfix_increment() {
        let output = run_source("let A = 10\nprint(A++)\nprint(A)").unwrap();
        assert_eq!(output, vec!["10", "11"]);
    }

    #[test]
    fn test_integration_postfix_decrement() {
        let output = run_source("let A = 10\nprint(A--)\nprint(A)").unwrap();
        assert_eq!(output, vec!["10", "9"]);
    }

    #[test]
    fn test_integration_compound_assignment_subtract() {
        let output = run_source("let A = 10\nA -= 3\nprint(A)").unwrap();
        assert_eq!(output, vec!["7"]);
    }

    #[test]
    fn test_integration_compound_assignment_multiply() {
        let output = run_source("let A = 5\nA *= 3\nprint(A)").unwrap();
        assert_eq!(output, vec!["15"]);
    }

    #[test]
    fn test_integration_compound_assignment_divide() {
        let output = run_source("let A = 10\nA /= 2\nprint(A)").unwrap();
        assert_eq!(output, vec!["5"]);
    }

    #[test]
    fn test_integration_compound_assignment_modulo() {
        let output = run_source("let A = 10\nA %= 3\nprint(A)").unwrap();
        assert_eq!(output, vec!["1"]);
    }

    // ===== Heap-backed runtime value integration tests =====

    #[test]
    fn test_runtime_value_round_trip_number() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        let rv = Value::Number(42.5).to_runtime_value(&mut vm.heap);
        let back = Value::from_runtime_value(&rv, &vm.heap).unwrap();
        assert_eq!(back, Value::Number(42.5));
    }

    #[test]
    fn test_runtime_value_round_trip_boolean() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        let rv = Value::Boolean(true).to_runtime_value(&mut vm.heap);
        let back = Value::from_runtime_value(&rv, &vm.heap).unwrap();
        assert_eq!(back, Value::Boolean(true));
    }

    #[test]
    fn test_runtime_value_round_trip_null() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        let rv = Value::Null.to_runtime_value(&mut vm.heap);
        let back = Value::from_runtime_value(&rv, &vm.heap).unwrap();
        assert_eq!(back, Value::Null);
    }

    #[test]
    fn test_runtime_value_round_trip_string() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        let rv = Value::Str("hello heap".to_string()).to_runtime_value(&mut vm.heap);
        let back = Value::from_runtime_value(&rv, &vm.heap).unwrap();
        assert_eq!(back, Value::Str("hello heap".to_string()));
    }

    #[test]
    fn test_runtime_value_round_trip_list() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        // Build a heap list
        let n = vm.heap.allocate(HeapObject::Number(1.0));
        let s = vm.heap.allocate(HeapObject::Str("two".to_string()));
        let b = vm.heap.allocate(HeapObject::Boolean(false));
        let z = vm.heap.allocate(HeapObject::Null);
        let list_ref = vm.heap.allocate(HeapObject::list(vec![n, s, b, z]));
        let list = Value::List(list_ref.clone());
        let rv = list.to_runtime_value(&mut vm.heap);
        let back = Value::from_runtime_value(&rv, &vm.heap).unwrap();
        assert_eq!(back, list);
        // Shared identity: same handle
        match back {
            Value::List(r) => assert_eq!(r.index(), list_ref.index()),
            _ => panic!("Expected list"),
        }
    }

    #[test]
    fn test_runtime_value_round_trip_object() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        let x = vm.heap.allocate(HeapObject::Number(10.0));
        let y = vm.heap.allocate(HeapObject::Str("hello".to_string()));
        let obj_ref = vm.heap.allocate(HeapObject::object(vec![
            ("x".to_string(), x),
            ("y".to_string(), y),
        ]));
        let obj = Value::Object(obj_ref.clone());
        let rv = obj.to_runtime_value(&mut vm.heap);
        let back = Value::from_runtime_value(&rv, &vm.heap).unwrap();
        match back {
            Value::Object(back_ref) => {
                assert_eq!(back_ref.index(), obj_ref.index());
                match vm.heap.get(&back_ref).unwrap() {
                    HeapObject::Object { props, .. } => {
                        assert_eq!(props.len(), 2);
                    }
                    _ => panic!("Expected object on heap"),
                }
            }
            _ => panic!("Expected Object"),
        }
    }

    #[test]
    fn test_heap_invalidation() {
        let mut heap = Heap::new();
        let r1 = heap.allocate(HeapObject::Str("alive".to_string()));
        assert!(heap.is_valid(&r1));

        // Allocate more to push r1 into free list
        let mut refs = Vec::new();
        for _ in 0..10 {
            refs.push(heap.allocate(HeapObject::Str("filler".to_string())));
        }

        // Deallocate r1, then reallocate - generation should change
        heap.deallocate(&r1).unwrap();
        assert!(!heap.is_valid(&r1));

        let r2 = heap.allocate(HeapObject::Str("reused".to_string()));
        // r2 should have index of r1 but different generation
        assert_eq!(r2.index(), r1.index());
        assert_ne!(r2.generation(), r1.generation());
        assert!(heap.is_valid(&r2));
    }

    #[test]
    fn test_heap_multiple_allocations() {
        let mut heap = Heap::new();
        let r1 = heap.allocate(HeapObject::Str("first".to_string()));
        let r2 = heap.allocate(HeapObject::Str("second".to_string()));
        let r3 = heap.allocate(HeapObject::Str("third".to_string()));

        assert_eq!(r1.index(), 0);
        assert_eq!(r2.index(), 1);
        assert_eq!(r3.index(), 2);

        match heap.get(&r1).unwrap() {
            HeapObject::Str(s) => assert_eq!(s, "first"),
            _ => panic!("Expected Str"),
        }
        match heap.get(&r2).unwrap() {
            HeapObject::Str(s) => assert_eq!(s, "second"),
            _ => panic!("Expected Str"),
        }
        match heap.get(&r3).unwrap() {
            HeapObject::Str(s) => assert_eq!(s, "third"),
            _ => panic!("Expected Str"),
        }
    }

    #[test]
    fn test_heap_free_list_reuse() {
        let mut heap = Heap::new();
        let r1 = heap.allocate(HeapObject::Str("a".to_string()));
        let r2 = heap.allocate(HeapObject::Str("b".to_string()));
        assert_eq!(r1.index(), 0);
        assert_eq!(r2.index(), 1);

        heap.deallocate(&r1).unwrap();

        let r3 = heap.allocate(HeapObject::Str("c".to_string()));
        // r3 should reuse r1's slot
        assert_eq!(r3.index(), r1.index());
        assert_ne!(r3.generation(), r1.generation());
    }

    #[test]
    fn test_heap_contains() {
        let mut heap = Heap::new();
        let r1 = heap.allocate(HeapObject::Str("test".to_string()));
        assert!(heap.contains(&r1));
        heap.deallocate(&r1).unwrap();
        assert!(!heap.contains(&r1));
    }

    #[test]
    fn test_heap_list_allocation() {
        let mut heap = Heap::new();
        let e1 = heap.allocate(HeapObject::Str("a".to_string()));
        let e2 = heap.allocate(HeapObject::Str("b".to_string()));
        let e3 = heap.allocate(HeapObject::Str("c".to_string()));
        let list_ref = heap.allocate(HeapObject::list(vec![e1, e2, e3]));
        match heap.get(&list_ref).unwrap() {
            HeapObject::List { items, .. } => {
                assert_eq!(items.len(), 3);
            }
            _ => panic!("Expected List"),
        }
    }

    #[test]
    fn test_heap_object_allocation() {
        let mut heap = Heap::new();
        let v0 = heap.allocate(HeapObject::Str("10".to_string()));
        let v1 = heap.allocate(HeapObject::Str("20".to_string()));
        let obj_ref = heap.allocate(HeapObject::object(vec![
            ("x".to_string(), v0),
            ("y".to_string(), v1),
        ]));
        match heap.get(&obj_ref).unwrap() {
            HeapObject::Object { props, .. } => {
                assert_eq!(props.len(), 2);
                assert_eq!(props[0].0, "x");
                assert_eq!(props[1].0, "y");
            }
            _ => panic!("Expected Object"),
        }
    }

    #[test]
    fn test_heap_function_allocation() {
        let mut heap = Heap::new();
        let func_ref = heap.allocate(HeapObject::Function {
            index: 0,
            name: "test_func".to_string(),
            params: vec!["a".to_string(), "b".to_string()],
            entry_point: 42,
        });
        match heap.get(&func_ref).unwrap() {
            HeapObject::Function {
                index,
                name,
                params,
                entry_point,
            } => {
                assert_eq!(*index, 0);
                assert_eq!(name, "test_func");
                assert_eq!(params, &vec!["a".to_string(), "b".to_string()]);
                assert_eq!(*entry_point, 42);
            }
            _ => panic!("Expected Function"),
        }
    }

    // ===== GC integration tests =====

    #[test]
    fn test_gc_manual_collect() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        let r1 = vm.heap.allocate(HeapObject::Str("keep".to_string()));
        let r2 = vm.heap.allocate(HeapObject::Str("drop".to_string()));
        vm.add_live_ref(r1.clone());

        vm.run_gc();

        assert!(vm.heap.is_valid(&r1));
        assert!(!vm.heap.is_valid(&r2));
    }

    #[test]
    fn test_gc_collect_reachable_list_elements() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        let elem = vm.heap.allocate(HeapObject::Str("elem".to_string()));
        let list = vm.heap.allocate(HeapObject::list(vec![elem.clone()]));
        let orphan = vm.heap.allocate(HeapObject::Str("orphan".to_string()));

        vm.add_live_ref(list.clone());
        vm.run_gc();

        assert!(vm.heap.is_valid(&list));
        assert!(vm.heap.is_valid(&elem));
        assert!(!vm.heap.is_valid(&orphan));
    }

    #[test]
    fn test_gc_collect_reachable_object_values() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        let val = vm.heap.allocate(HeapObject::Str("val".to_string()));
        let obj = vm.heap.allocate(HeapObject::object(vec![("key".to_string(), val.clone())]));
        let orphan = vm.heap.allocate(HeapObject::Str("orphan".to_string()));

        vm.add_live_ref(obj.clone());
        vm.run_gc();

        assert!(vm.heap.is_valid(&obj));
        assert!(vm.heap.is_valid(&val));
        assert!(!vm.heap.is_valid(&orphan));
    }

    #[test]
    fn test_gc_collect_unreachable_cycle() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        let a = vm.heap.allocate(HeapObject::Str("a".to_string()));
        let b = vm.heap.allocate(HeapObject::Str("b".to_string()));

        *vm.heap.get_mut(&a).unwrap() = HeapObject::list(vec![b.clone()]);
        *vm.heap.get_mut(&b).unwrap() = HeapObject::list(vec![a.clone()]);

        vm.run_gc();

        assert!(!vm.heap.is_valid(&a));
        assert!(!vm.heap.is_valid(&b));
    }

    #[test]
    fn test_gc_reachable_cycle_survives() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        let a = vm.heap.allocate(HeapObject::Str("a".to_string()));
        let b = vm.heap.allocate(HeapObject::Str("b".to_string()));

        *vm.heap.get_mut(&a).unwrap() = HeapObject::list(vec![b.clone()]);
        *vm.heap.get_mut(&b).unwrap() = HeapObject::list(vec![a.clone()]);

        vm.add_live_ref(a.clone());
        vm.run_gc();

        assert!(vm.heap.is_valid(&a));
        assert!(vm.heap.is_valid(&b));
    }

    #[test]
    fn test_gc_stale_refs_cleaned_after_collect() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        let r1 = vm.heap.allocate(HeapObject::Str("temp".to_string()));
        vm.add_live_ref(r1.clone());

        vm.run_gc();

        assert!(vm.live_refs().iter().all(|r| vm.heap.is_valid(r)));
    }

    #[test]
    fn test_gc_trigger_on_threshold() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        vm.gc = Gc::new(3);

        let r1 = vm.heap.allocate(HeapObject::Str("a".to_string()));
        vm.add_live_ref(r1.clone());
        vm.gc.on_allocation();

        let r2 = vm.heap.allocate(HeapObject::Str("b".to_string()));
        vm.add_live_ref(r2.clone());
        vm.gc.on_allocation();

        let r3 = vm.heap.allocate(HeapObject::Str("c".to_string()));
        vm.add_live_ref(r3.clone());
        vm.gc.on_allocation();

        assert_eq!(vm.gc.stats().collections, 0);

        let r4 = vm.heap.allocate(HeapObject::Str("d".to_string()));
        vm.add_live_ref(r4.clone());
        vm.gc.on_allocation();
        vm.maybe_gc();

        assert_eq!(vm.gc.stats().collections, 1);
    }

    #[test]
    fn test_gc_diamond_graph_via_vm() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        let shared = vm.heap.allocate(HeapObject::Str("shared".to_string()));
        let left = vm.heap.allocate(HeapObject::list(vec![shared.clone()]));
        let right = vm.heap.allocate(HeapObject::list(vec![shared.clone()]));
        let root = vm.heap.allocate(HeapObject::list(vec![left.clone(), right.clone()]));
        let orphan = vm.heap.allocate(HeapObject::Str("orphan".to_string()));

        vm.add_live_ref(root.clone());
        vm.run_gc();

        assert!(vm.heap.is_valid(&root));
        assert!(vm.heap.is_valid(&left));
        assert!(vm.heap.is_valid(&right));
        assert!(vm.heap.is_valid(&shared));
        assert!(!vm.heap.is_valid(&orphan));
    }

    #[test]
    fn test_gc_multiple_live_refs() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        let r1 = vm.heap.allocate(HeapObject::Str("a".to_string()));
        let r2 = vm.heap.allocate(HeapObject::Str("b".to_string()));
        let orphan = vm.heap.allocate(HeapObject::Str("orphan".to_string()));

        vm.add_live_ref(r1.clone());
        vm.add_live_ref(r2.clone());
        vm.run_gc();

        assert!(vm.heap.is_valid(&r1));
        assert!(vm.heap.is_valid(&r2));
        assert!(!vm.heap.is_valid(&orphan));
    }

    #[test]
    fn test_gc_stats_tracking() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        let _r1 = vm.heap.allocate(HeapObject::Str("a".to_string()));
        let _r2 = vm.heap.allocate(HeapObject::Str("b".to_string()));

        vm.run_gc();

        assert_eq!(vm.gc().stats().collections, 1);
        assert_eq!(vm.gc().stats().objects_freed, 2);
        assert_eq!(vm.gc().stats().objects_survived, 0);
    }

    #[test]
    fn test_gc_integration_print_function() {
        let output = run_source("print(\"hello\")").unwrap();
        assert_eq!(output, vec!["hello"]);
    }

    #[test]
    fn test_gc_integration_list_operations() {
        let output = run_source("let L = [1, 2, 3]\nprint(L[0])\nprint(L[1])").unwrap();
        assert_eq!(output, vec!["1", "2"]);
    }

    #[test]
    fn test_gc_integration_object_operations() {
        let output = run_source("let O = { x: 10 }\nprint(O.x)").unwrap();
        assert_eq!(output, vec!["10"]);
    }

    #[test]
    fn test_gc_integration_function_calls() {
        let source = "function add(A, B) {\n    return A + B\n}\nprint(add(3, 4))";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["7"]);
    }

    #[test]
    fn test_gc_integration_string_concat() {
        let output = run_source(r#"print("Hello " + "World")"#).unwrap();
        assert_eq!(output, vec!["Hello World"]);
    }

    #[test]
    fn test_gc_automatic_trigger_during_execution() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        vm.gc = Gc::new(5);

        for i in 0..20 {
            let val = vm.heap.allocate(HeapObject::Str(format!("str_{}", i)));
            vm.add_live_ref(val);
            vm.gc.on_allocation();
            vm.maybe_gc();
        }

        assert!(vm.gc.stats().collections > 0);
        assert_eq!(vm.gc.stats().allocations_since_gc, 0);
    }

    #[test]
    fn test_gc_automatic_source_execution() {
        let source = "let A = \"first\"\nlet B = \"second\"\nlet C = \"third\"\nprint(A)\nprint(B)\nprint(C)";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["first", "second", "third"]);
    }

    #[test]
    fn test_gc_reachable_objects_survive() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        vm.gc = Gc::new(0);

        let r1 = vm.heap.allocate(HeapObject::Str("keep1".to_string()));
        let r2 = vm.heap.allocate(HeapObject::Str("keep2".to_string()));
        vm.add_live_ref(r1.clone());
        vm.add_live_ref(r2.clone());
        vm.run_gc();

        assert!(vm.heap.is_valid(&r1));
        assert!(vm.heap.is_valid(&r2));
    }

    #[test]
    fn test_gc_unreachable_objects_collected() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        vm.gc = Gc::new(0);

        let _r1 = vm.heap.allocate(HeapObject::Str("orphan1".to_string()));
        let _r2 = vm.heap.allocate(HeapObject::Str("orphan2".to_string()));

        vm.run_gc();

        assert_eq!(vm.heap.count(), 0);
    }

    #[test]
    fn test_gc_unreachable_cycle_collected() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        vm.gc = Gc::new(0);

        let a = vm.heap.allocate(HeapObject::Str("a".to_string()));
        let b = vm.heap.allocate(HeapObject::Str("b".to_string()));
        *vm.heap.get_mut(&a).unwrap() = HeapObject::list(vec![b.clone()]);
        *vm.heap.get_mut(&b).unwrap() = HeapObject::list(vec![a.clone()]);

        vm.run_gc();

        assert!(!vm.heap.is_valid(&a));
        assert!(!vm.heap.is_valid(&b));
    }

    #[test]
    fn test_gc_reachable_cycle_survives_auto() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        vm.gc = Gc::new(0);

        let a = vm.heap.allocate(HeapObject::Str("a".to_string()));
        let b = vm.heap.allocate(HeapObject::Str("b".to_string()));
        *vm.heap.get_mut(&a).unwrap() = HeapObject::list(vec![b.clone()]);
        *vm.heap.get_mut(&b).unwrap() = HeapObject::list(vec![a.clone()]);

        vm.add_live_ref(a.clone());
        vm.run_gc();

        assert!(vm.heap.is_valid(&a));
        assert!(vm.heap.is_valid(&b));
    }

    #[test]
    fn test_gc_multiple_roots_safe() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        vm.gc = Gc::new(0);

        let r1 = vm.heap.allocate(HeapObject::Str("root1".to_string()));
        let r2 = vm.heap.allocate(HeapObject::Str("root2".to_string()));
        let r3 = vm.heap.allocate(HeapObject::Str("root3".to_string()));
        let orphan = vm.heap.allocate(HeapObject::Str("orphan".to_string()));

        vm.add_live_ref(r1.clone());
        vm.add_live_ref(r2.clone());
        vm.add_live_ref(r3.clone());

        vm.run_gc();

        assert!(vm.heap.is_valid(&r1));
        assert!(vm.heap.is_valid(&r2));
        assert!(vm.heap.is_valid(&r3));
        assert!(!vm.heap.is_valid(&orphan));
    }

    #[test]
    fn test_gc_during_vm_execution_safe() {
        let source = "let A = [1, 2, 3]\nlet B = { x: 10, y: 20 }\nprint(A[0])\nprint(B.x)";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["1", "10"]);
    }

    #[test]
    fn test_gc_invalid_ref_after_collect() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        vm.gc = Gc::new(0);

        let r1 = vm.heap.allocate(HeapObject::Str("temp".to_string()));
        vm.run_gc();

        assert!(!vm.heap.is_valid(&r1));
        assert!(vm.heap.get(&r1).is_err());
    }

    #[test]
    fn test_gc_manual_debug_available() {
        let mut vm = ZeroVM::new(BytecodeProgram { constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![] });
        vm.gc = Gc::new(0);

        let _r1 = vm.heap.allocate(HeapObject::Str("a".to_string()));
        let _r2 = vm.heap.allocate(HeapObject::Str("b".to_string()));

        vm.run_gc();

        assert_eq!(vm.gc().stats().collections, 1);
        assert_eq!(vm.gc().stats().objects_freed, 2);
    }

    #[test]
    fn test_gc_regression_original_tests() {
        let output = run_source("print(10 + 20)").unwrap();
        assert_eq!(output, vec!["30"]);
    }

    // ===== Closure tests =====

    #[test]
    fn test_closure_basic_invocation() {
        let source = "function outer() {\n    return function() { return 42 }\n}\nlet f = outer()\nprint(f())";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["42"]);
    }

    #[test]
    fn test_closure_capture_variable() {
        let source = "function outer() {\n    let x = 10\n    return function() { return x }\n}\nlet f = outer()\nprint(f())";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["10"]);
    }

    #[test]
    fn test_closure_mutation_shared() {
        let source = "function outer() {\n    let count = 0\n    function increment() { count += 1 }\n    function read() { return count }\n    increment()\n    increment()\n    return read\n}\nlet f = outer()\nprint(f())";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["2"]);
    }

    #[test]
    fn test_closure_multiple_closures_same_env() {
        let source = "function outer() {\n    let x = 0\n    function a() { x += 10 }\n    function b() { return x }\n    a()\n    return b\n}\nlet f = outer()\nprint(f())";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["10"]);
    }

    #[test]
    fn test_closure_survives_outer() {
        let source = "function make_adder(n) {\n    return function(x) { return x + n }\n}\nlet add5 = make_adder(5)\nprint(add5(3))";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["8"]);
    }

    #[test]
    fn test_closure_nested() {
        let source = "function outer() {\n    let x = 1\n    function middle() {\n        let y = 2\n        function inner() { return x + y }\n        return inner\n    }\n    return middle()\n}\nlet f = outer()\nprint(f())";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["3"]);
    }

    #[test]
    fn test_closure_recursion() {
        let source = "function factorial(n) {\n    if n <= 1 { return 1 }\n    return n * factorial(n - 1)\n}\nprint(factorial(5))";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["120"]);
    }

    #[test]
    fn test_closure_with_default_params() {
        let source = "function make_greeting(greeting = \"Hello\") {\n    return function(name) { return greeting + \" \" + name }\n}\nlet greet = make_greeting()\nprint(greet(\"World\"))";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["Hello World"]);
    }

    #[test]
    fn test_closure_default_param_uses_outer() {
        let source = "function outer() {\n    let prefix = \"Hi\"\n    function make(prefix = prefix) {\n        return function(name) { return prefix + \" \" + name }\n    }\n    return make()\n}\nlet f = outer()\nprint(f(\"There\"))";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["Hi There"]);
    }

    #[test]
    fn test_closure_arrow_function() {
        let source = "function outer() {\n    let x = 10\n    return () => x\n}\nlet f = outer()\nprint(f())";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["10"]);
    }

    #[test]
    fn test_closure_print_function() {
        let output = run_source("print(\"hello\")").unwrap();
        assert_eq!(output, vec!["hello"]);
    }

    #[test]
    fn test_closure_debug_bytecode() {
        let source = "function outer() {\n    let x = 10\n    return function() { return x }\n}\nlet f = outer()\nprint(f())";
        let bc = compile_source(source);
        
        println!("\n=== Instructions ===");
        for (i, instr) in bc.instructions.iter().enumerate() {
            println!("  {}: {:?}", i, instr);
        }
        
        println!("\n=== Functions ===");
        for (i, func) in bc.functions.iter().enumerate() {
            println!("  {}: {:?}", i, func);
        }
        
        println!("\n=== Constants ===");
        for (i, c) in bc.constants.iter().enumerate() {
            println!("  {}: {:?}", i, c);
        }
    }

    #[test]
    fn test_arrow_debug_trace() {
        let source = "function outer() {\n    let x = 10\n    return () => x\n}\nlet f = outer()\nprint(f())";
        let bc = compile_source(source);

        eprintln!("\n=== Constants ===");
        for (i, c) in bc.constants.iter().enumerate() {
            eprintln!("  {}: {:?}", i, c);
        }
        eprintln!("\n=== Functions ===");
        for (i, f) in bc.functions.iter().enumerate() {
            eprintln!("  {}: {:?}", i, f);
        }
        eprintln!("\n=== Function Bodies ===");
        for (i, body) in bc.function_bodies.iter().enumerate() {
            eprintln!("  Body {}:", i);
            for (j, instr) in body.iter().enumerate() {
                eprintln!("    {}: {:?}", j, instr);
            }
        }
        eprintln!("\n=== Main Instructions (first 12) ===");
        for (i, instr) in bc.instructions.iter().enumerate().take(12) {
            eprintln!("  {}: {:?}", i, instr);
        }

        match run_source(source) {
            Ok(output) => eprintln!("Output: {:?}", output),
            Err(e) => eprintln!("Error: {:?}", e),
        }
    }

    fn compile_try(source: &str) -> Result<BytecodeProgram, crate::error::CompilerError> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse().unwrap();
        compile(&program)
    }

    fn parse_try(source: &str) -> Result<crate::ast::Program, crate::error::ParserError> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        parser.parse()
    }

    fn run_vm(source: &str) -> Result<ZeroVM, VmError> {
        let bc = compile_source(source);
        let mut vm = ZeroVM::new(bc);
        vm.run()?;
        Ok(vm)
    }

    #[test]
    fn test_job009_try_catch_index_out_of_bounds() {
        let output = run_source(
            "try {\n    let A = [10]\n    print(A[99])\n} catch e {\n    print(\"caught\")\n}\nprint(\"after\")",
        )
        .unwrap();
        assert_eq!(output, vec!["caught", "after"]);
    }

    #[test]
    fn test_job009_catch_e_contains_error_message() {
        let vm = run_vm(
            "try {\n    let A = [10]\n    print(A[99])\n} catch e {\n    print.err(e)\n}",
        )
        .unwrap();
        let err = vm.get_error_output();
        assert_eq!(err.len(), 1);
        assert!(err[0].contains("out of bounds"), "{}", err[0]);
    }

    #[test]
    fn test_job009_finally_runs_on_try_success() {
        let output = run_source(
            "try {\n    print(\"try\")\n} catch e {\n    print(\"catch\")\n} finally {\n    print(\"finally\")\n}\nprint(\"after\")",
        )
        .unwrap();
        assert_eq!(output, vec!["try", "finally", "after"]);
    }

    #[test]
    fn test_job009_finally_runs_when_catch_handles_error() {
        let output = run_source(
            "try {\n    let A = [10]\n    print(A[99])\n} catch e {\n    print(\"catch\")\n} finally {\n    print(\"finally\")\n}\nprint(\"after\")",
        )
        .unwrap();
        assert_eq!(output, vec!["catch", "finally", "after"]);
    }

    #[test]
    fn test_job009_finally_runs_when_no_catch_then_rethrows() {
        let err = run_source(
            "try {\n    let A = [10]\n    print(A[99])\n} finally {\n    print(\"finally\")\n}",
        )
        .unwrap_err();
        assert!(matches!(err, VmError::RuntimeError(_)), "{}", err);
    }

    #[test]
    fn test_job009_nested_try_catch() {
        let output = run_source(
            "try {\n    try {\n        let A = [10]\n        print(A[99])\n    } catch inner {\n        print(\"inner\")\n    }\n} catch outer {\n    print(\"outer\")\n}\nprint(\"done\")",
        )
        .unwrap();
        assert_eq!(output, vec!["inner", "done"]);
    }

    #[test]
    fn test_job009_error_outside_try_still_fails() {
        let err = run_source("let A = [10]\nprint(A[99])").unwrap_err();
        assert!(matches!(err, VmError::RuntimeError(_)), "{}", err);
    }

    #[test]
    fn test_job009_print_err_displays_error() {
        let vm = run_vm("try {\n    let A = [10]\n    print(A[99])\n} catch e {\n    print.err(e)\n}").unwrap();
        assert_eq!(vm.get_error_output().len(), 1);
        assert!(vm.get_error_output()[0].contains("out of bounds"));
        assert!(vm.get_output().is_empty());
    }

    #[test]
    fn test_job009_type_number() {
        let output = run_source("print(type(10))").unwrap();
        assert_eq!(output, vec!["number"]);
    }

    #[test]
    fn test_job009_type_string() {
        let output = run_source(r#"print(type("Hello"))"#).unwrap();
        assert_eq!(output, vec!["string"]);
    }

    #[test]
    fn test_job009_type_boolean() {
        let output = run_source("print(type(true))").unwrap();
        assert_eq!(output, vec!["boolean"]);
    }

    #[test]
    fn test_job009_type_null() {
        let output = run_source("print(type(null))").unwrap();
        assert_eq!(output, vec!["null"]);
    }

    #[test]
    fn test_job009_type_list() {
        let output = run_source("print(type([1, 2]))").unwrap();
        assert_eq!(output, vec!["list"]);
    }

    #[test]
    fn test_job009_type_object() {
        let output = run_source("print(type({}))").unwrap();
        assert_eq!(output, vec!["object"]);
    }

    #[test]
    fn test_job009_type_function() {
        let output = run_source("function test() {}\nprint(type(test))").unwrap();
        assert_eq!(output, vec!["function"]);
    }

    #[test]
    fn test_job009_type_returns_string() {
        let output = run_source("print(type(type(10)))").unwrap();
        assert_eq!(output, vec!["string"]);
    }

    #[test]
    fn test_job009_number_from_string_success() {
        let output = run_source("print(number(\"123\"))").unwrap();
        assert_eq!(output, vec!["123"]);
    }

    #[test]
    fn test_job009_number_from_decimal_string_success() {
        let output = run_source("print(number(\"3.14\"))").unwrap();
        assert_eq!(output, vec!["3.14"]);
    }

    #[test]
    fn test_job009_number_invalid_string_error() {
        let err = run_source("print(number(\"hello\"))").unwrap_err();
        assert!(matches!(err, VmError::RuntimeError(_)), "{}", err);
    }

    #[test]
    fn test_job009_string_from_number_success() {
        let output = run_source("print(string(123))").unwrap();
        assert_eq!(output, vec!["123"]);
    }

    #[test]
    fn test_job009_string_from_boolean_success() {
        let output = run_source("print(string(true))").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_job009_string_from_list_error() {
        let err = run_source("print(string([1, 2]))").unwrap_err();
        assert!(matches!(err, VmError::RuntimeError(_)), "{}", err);
    }

    #[test]
    fn test_job009_boolean_identity_success() {
        let output = run_source("print(boolean(true))").unwrap();
        assert_eq!(output, vec!["true"]);
        let output = run_source("print(boolean(false))").unwrap();
        assert_eq!(output, vec!["false"]);
    }

    #[test]
    fn test_job009_boolean_from_number_error() {
        let err = run_source("print(boolean(1))").unwrap_err();
        assert!(matches!(err, VmError::RuntimeError(_)), "{}", err);
    }

    #[test]
    fn test_job009_boolean_from_string_error() {
        let err = run_source("print(boolean(\"true\"))").unwrap_err();
        assert!(matches!(err, VmError::RuntimeError(_)), "{}", err);
    }

    #[test]
    fn test_job009_conversion_errors_catchable() {
        let output = run_source(
            "try {\n    print(number(\"hello\"))\n} catch e {\n    print(\"caught-conv\")\n}\nprint(\"after\")",
        )
        .unwrap();
        assert_eq!(output, vec!["caught-conv", "after"]);
    }

    #[test]
    fn test_job009_try_catch_exact_example_section43() {
        let vm = run_vm(
            "try {\n    let A = [10]\n    print(A[99])\n} catch e {\n    print.err(e)\n}",
        )
        .unwrap();
        assert_eq!(vm.get_error_output().len(), 1);
        assert!(vm.get_output().is_empty());
    }

    #[test]
    fn test_job009_return_from_try_runs_finally() {
        let output = run_source(
            "function f() {\n    try {\n        return 1\n    } finally {\n        print(\"fin\")\n    }\n}\nprint(f())",
        )
        .unwrap();
        assert_eq!(output, vec!["fin", "1"]);
    }

    #[test]
    fn test_job009_error_in_function_caught_by_caller_try() {
        let output = run_source(
            "function boom() {\n    let A = [10]\n    return A[99]\n}\ntry {\n    print(boom())\n} catch e {\n    print(\"caught-fn\")\n}\nprint(\"after\")",
        )
        .unwrap();
        assert_eq!(output, vec!["caught-fn", "after"]);
    }

    #[test]
    fn test_job009_parse_error_not_caught() {
        // Parse/syntax errors occur before execution and cannot be caught (§43)
        let tokens = Lexer::new("try { let = 1 } catch e { }").tokenize();
        let parse_result = match tokens {
            Ok(toks) => {
                let mut p = Parser::new(toks);
                p.parse().map(|_| ())
            }
            Err(e) => Err(crate::error::ParserError::new(
                &e.message,
                e.line,
                e.column,
            )),
        };
        assert!(parse_result.is_err());
    }

    #[test]
    fn test_job008_list_assignment_shares_reference() {
        let output = run_source("let A = [10, 20]\nlet B = A\nB[0] = 99\nprint(A[0])").unwrap();
        assert_eq!(output, vec!["99"]);
    }

    #[test]
    fn test_job008_object_assignment_shares_reference() {
        let output =
            run_source("let user = { name: \"Kael\" }\nlet u2 = user\nu2.name = \"Budi\"\nprint(user.name)")
                .unwrap();
        assert_eq!(output, vec!["Budi"]);
    }

    #[test]
    fn test_job008_list_passed_to_function_mutation_visible() {
        let source = "function mutate(L) {\n    L[0] = 99\n}\nlet A = [1, 2]\nmutate(A)\nprint(A[0])";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["99"]);
    }

    #[test]
    fn test_job008_object_passed_to_function_mutation_visible() {
        let source =
            "function mutateO(O) {\n    O.x = 99\n}\nlet o = { x: 1 }\nmutateO(o)\nprint(o.x)";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["99"]);
    }

    #[test]
    fn test_job008_function_returning_new_list_independent() {
        let source =
            "function make() { return [1, 2] }\nlet A = make()\nlet B = make()\nA[0] = 99\nprint(A[0])\nprint(B[0])";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["99", "1"]);
    }

    #[test]
    fn test_job008_function_returning_new_object_independent() {
        let source =
            "function make() { return { v: 1 } }\nlet A = make()\nlet B = make()\nA.v = 99\nprint(A.v)\nprint(B.v)";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["99", "1"]);
    }

    #[test]
    fn test_job008_set_index_mutates_heap() {
        let output = run_source("let A = [1, 2, 3]\nA[1] = 99\nprint(A[1])").unwrap();
        assert_eq!(output, vec!["99"]);
    }

    #[test]
    fn test_job008_set_property_mutates_heap_existing_key() {
        let output = run_source("let user = { name: \"Kael\" }\nuser.name = \"Budi\"\nprint(user.name)")
            .unwrap();
        assert_eq!(output, vec!["Budi"]);
    }

    #[test]
    fn test_job008_set_property_mutates_heap_new_key() {
        let output = run_source("let user = { name: \"Kael\" }\nuser.age = 18\nprint(user.age)").unwrap();
        assert_eq!(output, vec!["18"]);
    }

    #[test]
    fn test_job008_const_reassignment_compile_error() {
        let err = compile_try("const A = 10\nA = 20").unwrap_err();
        assert!(err.message.contains("Cannot assign to const 'A'"), "{}", err.message);
    }

    #[test]
    fn test_job008_const_without_value_is_null() {
        let output = run_source("const A\nprint(A)").unwrap();
        assert_eq!(output, vec!["null"]);
    }

    #[test]
    fn test_job008_const_without_value_assignment_error() {
        let err = compile_try("const A\nA = 10").unwrap_err();
        assert!(err.message.contains("Cannot assign to const 'A'"), "{}", err.message);
    }

    #[test]
    fn test_job008_const_list_element_mutation_error() {
        let err = run_source("const A = [10, 20]\nA[0] = 30").unwrap_err();
        assert!(
            err.to_string().contains("Cannot modify const list"),
            "{}",
            err
        );
    }

    #[test]
    fn test_job008_const_object_property_mutation_existing_key_error() {
        let err = run_source("const user = { name: \"Kael\" }\nuser.name = \"Budi\"").unwrap_err();
        assert!(
            err.to_string().contains("Cannot modify const object"),
            "{}",
            err
        );
    }

    #[test]
    fn test_job008_const_object_property_mutation_new_key_error() {
        let err = run_source("const user = { name: \"Kael\" }\nuser.age = 18").unwrap_err();
        assert!(
            err.to_string().contains("Cannot modify const object"),
            "{}",
            err
        );
    }

    #[test]
    fn test_job008_captured_const_mutation_error() {
        let source = "function outer() {\n    const x = 1\n    return function() { x = 2 }\n}\nouter()()";
        let err = compile_try(source).unwrap_err();
        assert!(err.message.contains("Cannot assign to const 'x'"), "{}", err.message);
    }

    #[test]
    fn test_job008_duplicate_let_let_error() {
        let err = compile_try("let A = 10\nlet A = 20").unwrap_err();
        assert!(err.message.contains("Duplicate declaration of 'A'"), "{}", err.message);
    }

    #[test]
    fn test_job008_duplicate_let_const_error() {
        let err = compile_try("let A = 10\nconst A = 20").unwrap_err();
        assert!(err.message.contains("Duplicate declaration of 'A'"), "{}", err.message);
    }

    #[test]
    fn test_job008_duplicate_const_const_error() {
        let err = compile_try("const A = 10\nconst A = 20").unwrap_err();
        assert!(err.message.contains("Duplicate declaration of 'A'"), "{}", err.message);
    }

    #[test]
    fn test_job008_duplicate_const_let_error() {
        let err = compile_try("const A = 10\nlet A = 20").unwrap_err();
        assert!(err.message.contains("Duplicate declaration of 'A'"), "{}", err.message);
    }

    #[test]
    fn test_job008_shadowing_different_scopes_valid() {
        let source = "let A = 10\nif true {\n    let A = 20\n    print(A)\n}\nprint(A)";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["20", "10"]);
    }

    #[test]
    fn test_job008_duplicate_object_literal_key_error() {
        let err = parse_try("{ a: 1, a: 2 }").unwrap_err();
        assert!(err.message.contains("Duplicate object key 'a'"), "{}", err.message);
    }

    #[test]
    fn test_job008_inner_block_variable_does_not_leak() {
        let err = run_source("if true {\n    let A = 10\n}\nprint(A)").unwrap_err();
        assert!(
            err.to_string().contains("Undefined variable 'A'"),
            "{}",
            err
        );
    }

    #[test]
    fn test_job008_outer_variable_readable_from_inner() {
        let source = "let A = 10\nif true {\n    print(A)\n}";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["10"]);
    }

    #[test]
    fn test_job008_function_scope_assignment_does_not_change_outer() {
        let source = "let A = 10\nfunction test() {\n    A = 20\n    print(A)\n}\ntest()\nprint(A)";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["20", "10"]);
    }

    #[test]
    fn test_job008_function_reads_global_without_capture() {
        let source = "let A = 10\nfunction test() {\n    print(A)\n}\ntest()";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["10"]);
    }

    #[test]
    fn test_job008_else_if_chain_depth_gt_2() {
        let source = "if false {\n    print(1)\n} else if false {\n    print(2)\n} else if false {\n    print(3)\n} else if true {\n    print(4)\n} else {\n    print(5)\n}";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["4"]);
    }

    #[test]
    fn test_job008_break_outside_loop_compile_error() {
        let err = compile_try("break").unwrap_err();
        assert!(err.message.contains("break outside of loop"), "{}", err.message);
    }

    #[test]
    fn test_job008_continue_outside_loop_compile_error() {
        let err = compile_try("continue").unwrap_err();
        assert!(err.message.contains("continue outside of loop"), "{}", err.message);
    }

    #[test]
    fn test_job008_break_does_not_cross_function_boundary() {
        let err = compile_try("while true {\n    function f() {\n        break\n    }\n}").unwrap_err();
        assert!(err.message.contains("break outside of loop"), "{}", err.message);
    }

    #[test]
    fn test_job008_continue_does_not_cross_function_boundary() {
        let err =
            compile_try("while true {\n    function f() {\n        continue\n    }\n}").unwrap_err();
        assert!(err.message.contains("continue outside of loop"), "{}", err.message);
    }

    #[test]
    fn test_job008_ampersand_and_alias() {
        let output = run_source("print(true && false)\nprint(true || false)\nprint(not true)").unwrap();
        assert_eq!(output, vec!["false", "true", "false"]);
    }

    #[test]
    fn test_job008_and_or_not_identical_to_keywords() {
        let a = run_source("print(true and false)").unwrap();
        let b = run_source("print(true && false)").unwrap();
        assert_eq!(a, b);

        let a = run_source("print(false or true)").unwrap();
        let b = run_source("print(false || true)").unwrap();
        assert_eq!(a, b);

        let a = run_source("print(!true)").unwrap();
        let b = run_source("print(not true)").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_job008_no_truthy_and_non_boolean() {
        let err = run_source("print(10 and true)").unwrap_err();
        assert!(err.to_string().contains("Conditional jump requires boolean"), "{}", err);
    }

    #[test]
    fn test_job008_no_truthy_or_non_boolean() {
        let err = run_source("print(\"hello\" || false)").unwrap_err();
        assert!(err.to_string().contains("Conditional jump requires boolean"), "{}", err);
    }

    #[test]
    fn test_job008_not_non_boolean_error() {
        let err = run_source("print(not 10)").unwrap_err();
        assert!(err.to_string().contains("Cannot apply !"), "{}", err);
    }

    #[test]
    fn test_job008_raw_eq_still_true_for_number_string() {
        let output = run_source("print(10 === \"10\")\nprint(10 == \"10\")").unwrap();
        assert_eq!(output, vec!["true", "false"]);
    }

    #[test]
    fn test_job008_shared_reference_via_raw_eq() {
        let output = run_source("let A = [1]\nlet B = A\nprint(A === B)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    // ---- Job 010: string.* ----

    #[test]
    fn test_job010_string_length() {
        let output = run_source(r#"print(string.length("Hello"))"#).unwrap();
        assert_eq!(output, vec!["5"]);
    }

    #[test]
    fn test_job010_string_length_wrong_type_error() {
        let err = run_source("print(string.length(10))").unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_string_contains_true() {
        let output = run_source(r#"print(string.contains("Hello World", "World"))"#).unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_job010_string_contains_false() {
        let output = run_source(r#"print(string.contains("Hello", "xyz"))"#).unwrap();
        assert_eq!(output, vec!["false"]);
    }

    #[test]
    fn test_job010_string_contains_wrong_type_error() {
        let err = run_source(r#"print(string.contains("Hello", 10))"#).unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_string_trim() {
        let output = run_source(r#"print("[" + string.trim("  Hello  ") + "]")"#).unwrap();
        assert_eq!(output, vec!["[Hello]"]);
    }

    #[test]
    fn test_job010_string_trim_wrong_type_error() {
        let err = run_source("print(string.trim(true))").unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_string_split_spec_example() {
        let output = run_source(r#"print(string.split("a,b,c", ","))"#).unwrap();
        assert_eq!(output, vec!["[a, b, c]"]);
    }

    #[test]
    fn test_job010_string_split_wrong_type_error() {
        let err = run_source(r#"print(string.split(10, ","))"#).unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_string_replace_spec_example() {
        let output = run_source(r#"print(string.replace("Hello World", "World", "ZeroScript"))"#)
            .unwrap();
        assert_eq!(output, vec!["Hello ZeroScript"]);
    }

    #[test]
    fn test_job010_string_replace_wrong_type_error() {
        let err = run_source(r#"print(string.replace("a", 1, "b"))"#).unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_string_startswith_true() {
        let output = run_source(r#"print(string.startsWith("Hello World", "Hello"))"#).unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_job010_string_startswith_false() {
        let output = run_source(r#"print(string.startsWith("Hello", "World"))"#).unwrap();
        assert_eq!(output, vec!["false"]);
    }

    #[test]
    fn test_job010_string_startswith_wrong_type_error() {
        let err = run_source(r#"print(string.startsWith(10, "H"))"#).unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_string_endswith_true() {
        let output = run_source(r#"print(string.endsWith("Hello World", "World"))"#).unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_job010_string_endswith_false() {
        let output = run_source(r#"print(string.endsWith("Hello", "World"))"#).unwrap();
        assert_eq!(output, vec!["false"]);
    }

    #[test]
    fn test_job010_string_endswith_wrong_type_error() {
        let err = run_source(r#"print(string.endsWith("Hello", 10))"#).unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_string_lower() {
        let output = run_source(r#"print(string.lower("Hello World"))"#).unwrap();
        assert_eq!(output, vec!["hello world"]);
    }

    #[test]
    fn test_job010_string_lower_does_not_mutate_original() {
        let output = run_source(
            r#"let A = "Hello"
print(string.lower(A))
print(A)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["hello", "Hello"]);
    }

    #[test]
    fn test_job010_string_upper() {
        let output = run_source(r#"print(string.upper("Hello World"))"#).unwrap();
        assert_eq!(output, vec!["HELLO WORLD"]);
    }

    #[test]
    fn test_job010_string_upper_does_not_mutate_original() {
        let output = run_source(
            r#"let A = "Hello"
print(string.upper(A))
print(A)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["HELLO", "Hello"]);
    }

    #[test]
    fn test_job010_string_upper_wrong_type_error() {
        let err = run_source("print(string.upper(null))").unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_string_indexof_found() {
        let output = run_source(r#"print(string.indexOf("Hello World", "World"))"#).unwrap();
        assert_eq!(output, vec!["6"]);
    }

    #[test]
    fn test_job010_string_indexof_not_found() {
        let output = run_source(r#"print(string.indexOf("Hello", "xyz"))"#).unwrap();
        assert_eq!(output, vec!["-1"]);
    }

    #[test]
    fn test_job010_string_indexof_wrong_type_error() {
        let err = run_source(r#"print(string.indexOf("Hello", true))"#).unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_string_ops_catchable() {
        let output = run_source(
            r#"try {
    print(string.length(10))
} catch e {
    print("caught")
}
print("after")"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught", "after"]);
    }

    #[test]
    fn test_job010_string_operations_do_not_change_original() {
        let output = run_source(
            r#"let A = "  Hello World  "
print(string.trim(A))
print(string.replace(A, "World", "Zero"))
print(string.lower(A))
print(A)"#,
        )
        .unwrap();
        assert_eq!(
            output,
            vec!["Hello World", "  Hello Zero  ", "  hello world  ", "  Hello World  "]
        );
    }

    // ---- Job 010: list.* ----

    #[test]
    fn test_job010_list_add() {
        let output = run_source(
            r#"let A = [10, 20]
list.add(A, 30)
print(A)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["[10, 20, 30]"]);
    }

    #[test]
    fn test_job010_list_add_wrong_type_error() {
        let err = run_source("list.add(10, 1)").unwrap_err();
        assert!(err.to_string().contains("must be list"), "{}", err);
    }

    #[test]
    fn test_job010_list_add_const_error() {
        let err = run_source("const A = [10]\nlist.add(A, 20)").unwrap_err();
        assert!(err.to_string().contains("const"), "{}", err);
    }

    #[test]
    fn test_job010_list_remove() {
        let output = run_source(
            r#"let A = [10, 20, 30]
list.remove(A, 1)
print(A)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["[10, 30]"]);
    }

    #[test]
    fn test_job010_list_remove_invalid_index_error() {
        let err = run_source("let A = [10]\nlist.remove(A, 5)").unwrap_err();
        assert!(err.to_string().contains("out of bounds"), "{}", err);
    }

    #[test]
    fn test_job010_list_remove_const_error() {
        let err = run_source("const A = [10, 20]\nlist.remove(A, 0)").unwrap_err();
        assert!(err.to_string().contains("const"), "{}", err);
    }

    #[test]
    fn test_job010_list_remove_wrong_index_type_error() {
        let err = run_source("let A = [10]\nlist.remove(A, \"x\")").unwrap_err();
        assert!(err.to_string().contains("must be number"), "{}", err);
    }

    #[test]
    fn test_job010_list_length() {
        let output = run_source("print(list.length([10, 20, 30]))").unwrap();
        assert_eq!(output, vec!["3"]);
    }

    #[test]
    fn test_job010_list_length_empty() {
        let output = run_source("print(list.length([]))").unwrap();
        assert_eq!(output, vec!["0"]);
    }

    #[test]
    fn test_job010_list_length_wrong_type_error() {
        let err = run_source("print(list.length(\"abc\"))").unwrap_err();
        assert!(err.to_string().contains("must be list"), "{}", err);
    }

    #[test]
    fn test_job010_list_add_mutation_visible_to_caller_section35() {
        // Language §35 exact example
        let output = run_source(
            r#"function tambah(A) {
    list.add(A, 40)
}
let X = [10, 20, 30]
tambah(X)
print(X)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["[10, 20, 30, 40]"]);
    }

    #[test]
    fn test_job010_list_const_mutation_via_function_error() {
        // §35: if source is const, mutation through function also errors
        let err = run_source(
            r#"function tambah(A) {
    list.add(A, 40)
}
const X = [10]
tambah(X)"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("const"), "{}", err);
    }

    #[test]
    fn test_job010_list_add_catchable() {
        let output = run_source(
            r#"try {
    const A = [10]
    list.add(A, 20)
} catch e {
    print("caught")
}
print("after")"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught", "after"]);
    }

    // ---- Job 010: object.* ----

    #[test]
    fn test_job010_object_remove() {
        let output = run_source(
            r#"let user = { nama: "Kael", umur: 18 }
object.remove(user, "umur")
print(user)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["{nama: Kael}"]);
    }

    #[test]
    fn test_job010_object_remove_missing_key_error() {
        // §39 silent; analogous to §38 access → ERROR (ISSUES §60a)
        let err = run_source(r#"let o = { a: 1 }
object.remove(o, "missing")"#)
            .unwrap_err();
        assert!(err.to_string().contains("not found"), "{}", err);
    }

    #[test]
    fn test_job010_object_remove_non_string_key_error() {
        let err = run_source("let o = { a: 1 }\nobject.remove(o, 10)").unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_object_remove_const_error() {
        let err = run_source(r#"const o = { a: 1 }
object.remove(o, "a")"#)
            .unwrap_err();
        assert!(err.to_string().contains("const"), "{}", err);
    }

    #[test]
    fn test_job010_object_remove_wrong_type_error() {
        let err = run_source(r#"object.remove([1], "a")"#).unwrap_err();
        assert!(err.to_string().contains("must be object"), "{}", err);
    }

    #[test]
    fn test_job010_object_remove_catchable() {
        let output = run_source(
            r#"try {
    const o = { a: 1 }
    object.remove(o, "a")
} catch e {
    print("caught")
}
print("after")"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught", "after"]);
    }

    #[test]
    fn test_job010_object_remove_mutation_visible_in_function() {
        let output = run_source(
            r#"function strip(o) {
    object.remove(o, "temp")
}
let data = { keep: 1, temp: 2 }
strip(data)
print(data)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["{keep: 1}"]);
    }

    // ---- Job 010: print / print.err ----

    #[test]
    fn test_job010_print_list_and_object() {
        let output = run_source("print([1, 2])\nprint({ a: 1 })").unwrap();
        assert_eq!(output, vec!["[1, 2]", "{a: 1}"]);
    }

    #[test]
    fn test_job010_print_function() {
        let output = run_source("function f() {}\nprint(f)").unwrap();
        assert_eq!(output.len(), 1);
        assert!(output[0].contains("function"), "{}", output[0]);
    }

    #[test]
    fn test_job010_print_null_boolean_number_string() {
        let output =
            run_source("print(null)\nprint(true)\nprint(10)\nprint(\"hi\")").unwrap();
        assert_eq!(output, vec!["null", "true", "10", "hi"]);
    }

    #[test]
    fn test_job010_print_err_outside_catch() {
        // §44: print.err only writes to error output, does not throw
        let vm = run_vm(r#"print.err("Something went wrong")
print("ok")"#)
            .unwrap();
        assert_eq!(vm.get_output(), vec!["ok"]);
        assert_eq!(vm.get_error_output(), vec!["Something went wrong"]);
    }

    #[test]
    fn test_job010_print_err_list_value() {
        let vm = run_vm(r#"print.err([1, 2])"#).unwrap();
        assert_eq!(vm.get_error_output(), vec!["[1, 2]"]);
    }

    #[test]
    fn test_job010_print_err_does_not_throw() {
        let output = run_source(
            r#"print.err("warn")
print("after")"#,
        )
        .unwrap();
        assert_eq!(output, vec!["after"]);
    }

    // ---- Job 010: regression / integration ----

    #[test]
    fn test_job010_string_split_result_is_list_length() {
        let output = run_source(r#"print(list.length(string.split("a,b,c", ",")))"#).unwrap();
        assert_eq!(output, vec!["3"]);
    }

    #[test]
    fn test_job010_redeclare_builtin_error() {
        let err = run_source("let string = 10").unwrap_err();
        assert!(err.to_string().contains("built-in"), "{}", err);
    }

    #[test]
    fn test_job010_unknown_method_error() {
        let err = run_source(r#"print(string.nope("x"))"#).unwrap_err();
        assert!(err.to_string().contains("not found"), "{}", err);
    }

    #[test]
    fn test_job010_list_add_string_element() {
        let output = run_source(r#"let A = []
list.add(A, "x")
print(A)"#)
            .unwrap();
        assert_eq!(output, vec!["[x]"]);
    }

    #[test]
    fn test_job010_spec_section42_examples() {
        let output = run_source(
            r#"print(string.length("Hello"))
print(string.contains("Hello World", "World"))
print(string.trim("  Hello  "))
print(string.split("a,b,c", ","))
print(string.replace("Hello World", "World", "ZeroScript"))"#,
        )
        .unwrap();
        assert_eq!(
            output,
            vec![
                "5",
                "true",
                "Hello",
                "[a, b, c]",
                "Hello ZeroScript"
            ]
        );
    }

    #[test]
    fn test_job010_spec_section58_examples() {
        let output = run_source(
            r#"let A = "Hello World"
print(string.startsWith(A, "Hello"))
print(string.endsWith(A, "World"))
print(string.lower(A))
print(string.upper(A))
print(string.indexOf(A, "World"))
print(A)"#,
        )
        .unwrap();
        assert_eq!(
            output,
            vec![
                "true",
                "true",
                "hello world",
                "HELLO WORLD",
                "6",
                "Hello World"
            ]
        );
    }

    #[test]
    fn test_job010_spec_section34_list_examples() {
        let output = run_source(
            r#"let A = [10, 20]
list.add(A, 30)
list.remove(A, 1)
print(A)
print(list.length(A))"#,
        )
        .unwrap();
        assert_eq!(output, vec!["[10, 30]", "2"]);
    }

    #[test]
    fn test_job010_spec_section39_object_remove_example() {
        let output = run_source(
            r#"let user = { nama: "Kael", umur: 18 }
object.remove(user, "umur")
print(user.nama)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["Kael"]);
    }

    #[test]
    fn test_job010_spec_section44_console() {
        let vm = run_vm(
            r#"print("Hello")
print(10)
print(true)
print(null)
print.err("Something went wrong")"#,
        )
        .unwrap();
        assert_eq!(vm.get_output(), vec!["Hello", "10", "true", "null"]);
        assert_eq!(vm.get_error_output(), vec!["Something went wrong"]);
    }

    // ---- Job 011: color.* (§45) ----

    #[test]
    fn test_job011_color_red() {
        let output = run_source(r#"print(color.red("Error"))"#).unwrap();
        assert_eq!(output, vec!["\x1b[31mError\x1b[39m"]);
    }

    #[test]
    fn test_job011_color_green() {
        let output = run_source(r#"print(color.green("Success"))"#).unwrap();
        assert_eq!(output, vec!["\x1b[32mSuccess\x1b[39m"]);
    }

    #[test]
    fn test_job011_color_yellow() {
        let output = run_source(r#"print(color.yellow("Warning"))"#).unwrap();
        assert_eq!(output, vec!["\x1b[33mWarning\x1b[39m"]);
    }

    #[test]
    fn test_job011_color_blue() {
        let output = run_source(r#"print(color.blue("Info"))"#).unwrap();
        assert_eq!(output, vec!["\x1b[34mInfo\x1b[39m"]);
    }

    #[test]
    fn test_job011_color_bold_alone() {
        let output = run_source(r#"print(color.bold("Hi"))"#).unwrap();
        assert_eq!(output, vec!["\x1b[1mHi\x1b[22m"]);
    }

    #[test]
    fn test_job011_color_bold_combines_with_red() {
        // §45: color.bold(color.red("Error")) must nest correctly
        let output = run_source(r#"print(color.bold(color.red("Error")))"#).unwrap();
        assert_eq!(output, vec!["\x1b[1m\x1b[31mError\x1b[39m\x1b[22m"]);
    }

    #[test]
    fn test_job011_color_wrong_arity_error() {
        let err = run_source("print(color.red())").unwrap_err();
        assert!(err.to_string().contains("exactly 1"), "{}", err);
    }

    #[test]
    fn test_job011_color_catchable() {
        let output = run_source(
            r#"try {
    print(color.red())
} catch e {
    print("caught")
}"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught"]);
    }

    // ---- Job 011: file.* (§48) ----

    fn temp_path(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("zeroscript_job011_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name).to_string_lossy().into_owned()
    }

    fn cleanup(path: &str) {
        let p = std::path::Path::new(path);
        if p.is_dir() {
            let _ = std::fs::remove_dir_all(p);
        } else {
            let _ = std::fs::remove_file(p);
        }
    }

    fn run_source_with_args(source: &str, args: Vec<String>) -> Result<Vec<String>, VmError> {
        let bc = compile_source(source);
        let mut vm = ZeroVM::new(bc);
        vm.set_program_args(args);
        vm.run()?;
        Ok(vm.get_output().to_vec())
    }

    #[test]
    fn test_job011_file_write_read_roundtrip() {
        let path = temp_path("roundtrip.txt");
        let src = format!(
            r#"file.write("{path}", "Hello ZeroScript")
let content = file.read("{path}")
print(content)"#,
            path = path.replace('\\', "\\\\")
        );
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["Hello ZeroScript"]);
        cleanup(&path);
    }

    #[test]
    fn test_job011_file_exists_true_false() {
        let path = temp_path("exists.txt");
        let src = format!(
            r#"print(file.exists("{path}"))
file.write("{path}", "x")
print(file.exists("{path}"))"#,
            path = path.replace('\\', "\\\\")
        );
        let _ = std::fs::remove_file(&path);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["false", "true"]);
        cleanup(&path);
    }

    #[test]
    fn test_job011_file_remove_then_not_exists() {
        let path = temp_path("remove.txt");
        let src = format!(
            r#"file.write("{path}", "x")
file.remove("{path}")
print(file.exists("{path}"))"#,
            path = path.replace('\\', "\\\\")
        );
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["false"]);
        cleanup(&path);
    }

    #[test]
    fn test_job011_file_mkdir() {
        let path = temp_path("makedir");
        let _ = std::fs::remove_dir_all(&path);
        let src = format!(
            r#"file.mkdir("{path}")
print(file.exists("{path}"))"#,
            path = path.replace('\\', "\\\\")
        );
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["true"]);
        cleanup(&path);
    }

    #[test]
    fn test_job011_file_read_missing_catchable() {
        let path = temp_path("does_not_exist.txt");
        let _ = std::fs::remove_file(&path);
        let src = format!(
            r#"try {{
    file.read("{path}")
    print("no-error")
}} catch e {{
    print("caught")
}}"#,
            path = path.replace('\\', "\\\\")
        );
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["caught"]);
    }

    #[test]
    fn test_job011_file_read_missing_uncatchable_surfaces_error() {
        let path = temp_path("also_missing.txt");
        let _ = std::fs::remove_file(&path);
        let src = format!(r#"file.read("{path}")"#, path = path.replace('\\', "\\\\"));
        let err = run_source(&src).unwrap_err();
        assert!(err.to_string().contains("file.read"), "{}", err);
    }

    #[test]
    fn test_job011_file_write_wrong_type_error() {
        let err = run_source(r#"file.write(10, "x")"#).unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    // ---- Job 011: system.* (§52) ----

    #[test]
    fn test_job011_system_args_excludes_program_name() {
        // §52: zerovm app.zs hello world → ["hello", "world"]
        let output =
            run_source_with_args("print(system.args())", vec!["hello".into(), "world".into()])
                .unwrap();
        assert_eq!(output, vec!["[hello, world]"]);
    }

    #[test]
    fn test_job011_system_args_default_empty() {
        let output = run_source("print(system.args())").unwrap();
        assert_eq!(output, vec!["[]"]);
    }

    #[test]
    fn test_job011_system_env_existing() {
        // HOME is set in the test environment
        let output = run_source(r#"let v = system.env("HOME")
print(v == null)"#)
            .unwrap();
        assert_eq!(output, vec!["false"]);
    }

    #[test]
    fn test_job011_system_env_missing_returns_null() {
        let output =
            run_source(r#"print(system.env("ZEROSCRIPT_NO_SUCH_VAR_XYZ_12345") === null)"#)
                .unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_job011_system_os_returns_string() {
        let output = run_source(r#"print(type(system.os()))"#).unwrap();
        assert_eq!(output, vec!["string"]);
    }

    #[test]
    fn test_job011_system_arch_returns_string() {
        let output = run_source(r#"print(type(system.arch()))"#).unwrap();
        assert_eq!(output, vec!["string"]);
    }

    #[test]
    fn test_job011_system_os_arch_nonempty() {
        let output = run_source(r#"print(system.os() != "")
print(system.arch() != "")"#)
            .unwrap();
        assert_eq!(output, vec!["true", "true"]);
    }

    // ---- Job 011: json.* (§51) ----

    #[test]
    fn test_job011_json_stringify_parse_roundtrip_object() {
        let output = run_source(
            r#"let obj = { nama: "Kael", umur: 18, aktif: true }
let s = json.stringify(obj)
print(s)
let back = json.parse(s)
print(back.nama)
print(back.umur)
print(back.aktif)"#,
        )
        .unwrap();
        assert_eq!(
            output,
            vec![
                r#"{"nama":"Kael","umur":18,"aktif":true}"#,
                "Kael",
                "18",
                "true"
            ]
        );
    }

    #[test]
    fn test_job011_json_stringify_parse_roundtrip_list() {
        let output = run_source(
            r#"let xs = [1, "two", false, null]
let s = json.stringify(xs)
print(s)
let back = json.parse(s)
print(back)
print(list.length(back))"#,
        )
        .unwrap();
        assert_eq!(
            output,
            vec![r#"[1,"two",false,null]"#, r#"[1, two, false, null]"#, "4"]
        );
    }

    #[test]
    fn test_job011_json_parse_spec_example() {
        // §51: explicit parse only — no automatic JSON detection
        let output = run_source(
            r#"let body = "{\"nama\":\"Kael\"}"
let data = json.parse(body)
print(data.nama)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["Kael"]);
    }

    #[test]
    fn test_job011_json_stringify_spec_example() {
        let output = run_source(
            r#"let body = json.stringify({ nama: "Kael" })
print(body)"#,
        )
        .unwrap();
        assert_eq!(output, vec![r#"{"nama":"Kael"}"#]);
    }

    #[test]
    fn test_job011_json_parse_invalid_catchable() {
        let output = run_source(
            r#"try {
    json.parse("not valid json")
    print("no-error")
} catch e {
    print("caught")
}
print("after")"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught", "after"]);
    }

    #[test]
    fn test_job011_json_parse_invalid_uncatchable_surfaces() {
        let err = run_source(r#"json.parse("{bad")"#).unwrap_err();
        assert!(err.to_string().contains("Invalid JSON"), "{}", err);
    }

    #[test]
    fn test_job011_json_parse_wrong_type_error() {
        let err = run_source("json.parse(10)").unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job011_json_stringify_nested_object_list() {
        let output = run_source(
            r#"let data = { user: { name: "Kael" }, tags: ["a", "b"], n: 3.5 }
print(json.stringify(data))"#,
        )
        .unwrap();
        assert_eq!(
            output,
            vec![r#"{"user":{"name":"Kael"},"tags":["a","b"],"n":3.5}"#]
        );
    }

    #[test]
    fn test_job011_json_parse_returns_null_for_json_null() {
        let output = run_source(r#"print(json.parse("null") === null)"#).unwrap();
        assert_eq!(output, vec!["true"]);
    }

    // ---- Job 011: namespace integration ----

    #[test]
    fn test_job011_redeclare_namespace_builtin_error() {
        let err = run_source("let color = 10").unwrap_err();
        assert!(err.to_string().contains("built-in"), "{}", err);
    }

    #[test]
    fn test_job011_unknown_method_error() {
        let err = run_source(r#"print(file.nope("x"))"#).unwrap_err();
        assert!(err.to_string().contains("not found"), "{}", err);
    }

    #[test]
    fn test_job011_spec_section45_examples() {
        let output = run_source(
            r#"print(color.red("Error"))
print(color.green("Success"))
print(color.yellow("Warning"))
print(color.blue("Info"))
print(color.bold(color.red("Error")))"#,
        )
        .unwrap();
        assert_eq!(output.len(), 5);
        assert!(output[0].contains("Error"));
        assert!(output[1].contains("Success"));
        assert!(output[2].contains("Warning"));
        assert!(output[3].contains("Info"));
        assert!(output[4].starts_with("\x1b[1m"));
        assert!(output[4].contains("\x1b[31m"));
        assert!(output[4].ends_with("\x1b[22m"));
    }

    #[test]
    fn test_job011_spec_section52_examples() {
        let output = run_source_with_args(
            r#"print(type(system.args()))
print(type(system.os()))
print(type(system.arch()))"#,
            vec!["hello".into(), "world".into()],
        )
        .unwrap();
        assert_eq!(output, vec!["list", "string", "string"]);
    }

    // ---- Job 014: async/await core (§49, §101, §106-§108, §113) ----

    fn compile_err_msg(source: &str) -> String {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        match parser.parse() {
            Ok(program) => match compile(&program) {
                Ok(_) => panic!("Expected compile error for: {}", source),
                Err(e) => e.to_string(),
            },
            Err(e) => e.to_string(),
        }
    }

    fn run_vm_source(source: &str) -> ZeroVM {
        let bc = compile_source(source);
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        vm
    }

    /// Req 1 + 3: async function call creates a Task with initial state
    /// "pending"; the task value is a distinct runtime value (§101, §106).
    #[test]
    fn test_job014_async_call_creates_pending_task() {
        let vm = run_vm_source(
            r#"async function f() { return 1 }
let t = f()
print(type(t))
print(task.state(t))"#,
        );
        assert_eq!(vm.get_output(), &["task".to_string(), "pending".to_string()]);
        assert_eq!(vm.tasks.len(), 1);
        // Scheduler drained the un-awaited task after the top level (§112).
        assert_eq!(vm.tasks[0].state, TaskState::Completed);
        assert_eq!(vm.tasks[0].result, Some(Value::Number(1.0)));
    }

    /// Req 2: `await` outside an async function → compile-time ERROR (§49, §101).
    #[test]
    fn test_job014_await_outside_async_is_compile_error() {
        // Top level.
        let msg = compile_err_msg("async function f() { return 1 }\nlet t = f()\nlet x = await t");
        assert!(msg.contains("await"), "{}", msg);

        // Inside a plain (non-async) function.
        let msg = compile_err_msg("function syncFn() { await other() }");
        assert!(msg.contains("await"), "{}", msg);

        // Nested sync function inside an async function resets the context.
        let msg =
            compile_err_msg("async function outer() { function inner() { await nope() } }");
        assert!(msg.contains("await"), "{}", msg);
    }

    /// Req 4: `await task` returns the result once the task is completed
    /// (§107 example: tambah(10, 20) → 30).
    #[test]
    fn test_job014_await_returns_result() {
        let output = run_source(
            r#"async function tambah(A, B) { return A + B }
async function main() {
    let taskA = tambah(10, 20)
    let result = await taskA
    print(result)
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["30"]);
    }

    /// Req 5: await of a failed task forwards the stored error; try/catch
    /// handles it (§49, §108).
    #[test]
    fn test_job014_await_failed_task_catchable() {
        let bc = compile_source(
            r#"async function gagal() { return nosuchvar }
async function main() {
    let taskA = gagal()
    try {
        let result = await taskA
        print(result)
    } catch e {
        print.err(e)
    }
}
main()"#,
        );
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert!(vm.get_output().is_empty());
        assert_eq!(vm.get_error_output().len(), 1);
        assert!(
            vm.get_error_output()[0].contains("Undefined variable 'nosuchvar'"),
            "{}",
            vm.get_error_output()[0]
        );
        // tasks[0] = main (completed after catching), tasks[1] = gagal (failed).
        assert_eq!(vm.tasks[0].state, TaskState::Completed);
        assert_eq!(vm.tasks[1].state, TaskState::Failed);
    }

    /// Req 7: task.state() reports the correct string at every observable
    /// phase (§106). "cancelled" is unreachable until Job 015's task.cancel.
    #[test]
    fn test_job014_task_state_transitions() {
        let vm = run_vm_source(
            r#"async function f() { print(task.state(t))
    return 1 }
async function bad() { return nosuchvar }
async function check() { print(task.state(t))
    print(task.state(b)) }
let t = f()
let b = bad()
print(task.state(t))
let c = check()"#,
        );
        assert_eq!(
            vm.get_output(),
            &[
                "pending".to_string(),
                "running".to_string(),
                "completed".to_string(),
                "failed".to_string()
            ]
        );
        assert_eq!(vm.tasks[0].state, TaskState::Completed);
        assert_eq!(vm.tasks[1].state, TaskState::Failed);
        assert_eq!(vm.tasks[2].state, TaskState::Completed);
    }

    /// Req 8: awaiting a completed task again returns the stored result
    /// without re-running the body (§101, §107, §113).
    #[test]
    fn test_job014_await_twice_does_not_rerun() {
        let output = run_source(
            r#"async function f() { print("once")
    return 1 }
async function main() {
    let t = f()
    let a = await t
    let b = await t
    print(a)
    print(b)
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["once", "1", "1"]);
    }

    /// Req 9: nested async calls (outer awaits inner).
    #[test]
    fn test_job014_nested_async_calls() {
        let output = run_source(
            r#"async function inner() { return 2 }
async function outer() {
    let t = inner()
    let v = await t
    return v + 1
}
async function main() {
    let t = outer()
    let r = await t
    print(r)
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["3"]);
    }

    /// Req 10: recursion inside an async function.
    #[test]
    fn test_job014_async_recursion() {
        let output = run_source(
            r#"async function f(n) {
    if (n <= 1) { return 1 }
    let t = f(n - 1)
    let sub = await t
    return n * sub
}
async function main() {
    let t = f(5)
    let r = await t
    print(r)
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["120"]);
    }

    /// §106: task.state(A) with a non-task argument → ERROR.
    #[test]
    fn test_job014_task_state_non_task_error() {
        let err = run_source("print(task.state(5))").unwrap_err();
        assert!(err.to_string().contains("task"), "{}", err);
    }

    /// Part D decision (spec silent on awaiting non-tasks): runtime ERROR at
    /// the await point, catchable like any other runtime error. See
    /// ISSUES.md §63c.
    #[test]
    fn test_job014_await_non_task_is_error() {
        let bc = compile_source(
            r#"async function main() {
    try {
        let x = await 5
        print(x)
    } catch e {
        print.err(e)
    }
}
main()"#,
        );
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert!(vm.get_output().is_empty());
        assert_eq!(vm.get_error_output().len(), 1);
        assert!(
            vm.get_error_output()[0].contains("await expects a task"),
            "{}",
            vm.get_error_output()[0]
        );
    }

    /// §108: a task that fails without ever being awaited stores its error and
    /// does not stop the program or other tasks.
    #[test]
    fn test_job014_unawaited_failure_does_not_stop_program() {
        let vm = run_vm_source(
            r#"async function bad() { return nosuchvar }
async function good() { print("good") }
let b = bad()
let g = good()
print("done")"#,
        );
        assert_eq!(vm.get_output(), &["done".to_string(), "good".to_string()]);
        assert_eq!(vm.tasks[0].state, TaskState::Failed);
        assert_eq!(vm.tasks[1].state, TaskState::Completed);
    }

    /// §49: an error inside a task body can be handled by try/catch INSIDE the
    /// task — the task itself then completes normally.
    #[test]
    fn test_job014_try_catch_inside_task_body() {
        let bc = compile_source(
            r#"async function f() {
    try {
        let x = nosuchvar
        print(x)
    } catch e {
        print.err(e)
    }
    return 1
}
async function main() {
    let t = f()
    let r = await t
    print(r)
}
main()"#,
        );
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_output(), vec!["1"]);
        assert_eq!(vm.get_error_output().len(), 1);
        assert!(
            vm.get_error_output()[0].contains("Undefined variable 'nosuchvar'"),
            "{}",
            vm.get_error_output()[0]
        );
        // The task did not fail: its own try/catch handled the error.
        assert_eq!(vm.tasks[1].state, TaskState::Completed);
    }

    /// §113: awaiting a failed task again re-raises the stored error (the task
    /// stays terminal and is not re-run).
    #[test]
    fn test_job014_second_await_of_failed_task_reraises() {
        let bc = compile_source(
            r#"async function bad() { return nosuchvar }
async function main() {
    let t = bad()
    try {
        let a = await t
        print(a)
    } catch e {
        print.err("first")
    }
    try {
        let b = await t
        print(b)
    } catch e {
        print.err("second")
    }
}
main()"#,
        );
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert!(vm.get_output().is_empty());
        assert_eq!(vm.get_error_output(), &["first".to_string(), "second".to_string()]);
        assert_eq!(vm.tasks[1].state, TaskState::Failed);
    }

    /// Async closure call path: an async function that captured variables is
    /// called as a closure; the task keeps the env for its body (§101).
    #[test]
    fn test_job014_async_closure_capture() {
        let output = run_source(
            r#"function outer() {
    let base = 10
    async function inner(n) { print(base + n) }
    let t = inner(5)
}
outer()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["15"]);
    }

    // ==================== Job 015: Async Concurrency ====================

    /// Req 1 (§102): several tasks are created before any await; both
    /// progress concurrently — the short task reaches a terminal state while
    /// the long task is still being awaited, and each result stays separate.
    #[test]
    fn test_job015_concurrent_creation_before_await() {
        let output = run_source(
            r#"async function ambilUser() { await task.sleep(60)
    return "user" }
async function ambilData() { await task.sleep(10)
    return "data" }
async function main() {
    let userTask = ambilUser()
    let dataTask = ambilData()
    let user = await userTask
    print(user)
    print(task.state(dataTask))
    let data = await dataTask
    print(data)
}
main()"#,
        )
        .unwrap();
        // dataTask (10 ms) completed while main was still awaiting
        // userTask (60 ms): creating task A first does not gate task B.
        assert_eq!(output, vec!["user", "completed", "data"]);
    }

    /// Req 2 (§103): task.all() waits for every task and returns results in
    /// input order, not completion order.
    #[test]
    fn test_job015_all_results_input_order() {
        let output = run_source(
            r#"async function slow() { await task.sleep(60)
    return "slow" }
async function fast() { await task.sleep(10)
    return "fast" }
async function main() {
    let results = await task.all([slow(), fast()])
    print(results[0])
    print(results[1])
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["slow", "fast"]);
    }

    /// Req 3 (§103): task.all([]) → empty list.
    #[test]
    fn test_job015_all_empty_list() {
        let output = run_source(
            r#"async function main() {
    let results = await task.all([])
    print(type(results))
    print(list.length(results))
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["list", "0"]);
    }

    /// Req 4 (§103): one failing task fails task.all() (fail-fast, error
    /// forwarded) and does not cancel or corrupt the other tasks.
    #[test]
    fn test_job015_all_fail_fast() {
        let output = run_source(
            r#"async function bad() { return nosuchvar }
async function slow() { await task.sleep(200)
    return "ok" }
async function main() {
    let s = slow()
    try {
        await task.all([bad(), s])
        print("no error")
    } catch e {
        print("caught")
        print(task.state(s))
    }
}
main()"#,
        )
        .unwrap();
        // The slow task is untouched by the sibling failure: still running.
        assert_eq!(output, vec!["caught", "running"]);
    }

    /// Req 5 (§103): a non-task element in the list → ERROR.
    #[test]
    fn test_job015_all_non_task_error() {
        let output = run_source(
            r#"async function main() {
    try {
        await task.all([1, 2])
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught"]);
    }

    /// Req 6 (§104): task.race() yields the first finisher's result; losing
    /// tasks are not cancelled (they keep running).
    #[test]
    fn test_job015_race_first_finisher_wins() {
        let output = run_source(
            r#"async function slow() { await task.sleep(60)
    return "slow" }
async function fast() { await task.sleep(10)
    return "fast" }
async function main() {
    let s = slow()
    let f = fast()
    let winner = await task.race([s, f])
    print(winner)
    print(task.state(s))
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["fast", "running"]);
    }

    /// Req 7 (§104): if the first finisher fails, task.race() forwards the
    /// error to the awaiter.
    #[test]
    fn test_job015_race_first_finisher_error() {
        let output = run_source(
            r#"async function bad() { return nosuchvar }
async function slow() { await task.sleep(200)
    return "slow" }
async function main() {
    try {
        await task.race([bad(), slow()])
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught"]);
    }

    /// Req 8 (§104): task.race([]) → ERROR (nothing to wait for).
    #[test]
    fn test_job015_race_empty_error() {
        let output = run_source(
            r#"async function main() {
    try {
        await task.race([])
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught"]);
    }

    /// Req 9 (§105): task.cancel() on a running task → state becomes
    /// "cancelled".
    #[test]
    fn test_job015_cancel_running_task() {
        let output = run_source(
            r#"async function sleepy() { await task.sleep(1000)
    print("never") }
async function main() {
    let t = sleepy()
    await task.sleep(0)
    print(task.state(t))
    task.cancel(t)
    print(task.state(t))
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["running", "cancelled"]);
    }

    /// Req 10 (§105): task.cancel() on a completed task is a no-op — no
    /// error, state unchanged.
    #[test]
    fn test_job015_cancel_completed_noop() {
        let output = run_source(
            r#"async function quick() { return 1 }
async function main() {
    let t = quick()
    let r = await t
    task.cancel(t)
    print(task.state(t))
    print(r)
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["completed", "1"]);
    }

    /// Req 11 (§105): awaiting a cancelled task raises a cancellation error
    /// that try/catch handles.
    #[test]
    fn test_job015_await_cancelled_is_error() {
        let output = run_source(
            r#"async function sleepy() { await task.sleep(1000)
    return "never" }
async function main() {
    let t = sleepy()
    await task.sleep(0)
    task.cancel(t)
    try {
        await t
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught"]);
    }

    /// Req 12 (§109): task.wait() on completed → Null (result not taken);
    /// on failed → the task error; on cancelled → the cancellation error.
    #[test]
    fn test_job015_wait_terminal_states() {
        let output = run_source(
            r#"async function ok() { return 1 }
async function bad() { return nosuchvar }
async function sleepy() { await task.sleep(1000)
    return "never" }
async function main() {
    let a = ok()
    let b = bad()
    let c = sleepy()
    let w = await task.wait(a)
    print(type(w))
    try {
        await task.wait(b)
        print("no error")
    } catch e {
        print("wait failed caught")
    }
    task.cancel(c)
    try {
        await task.wait(c)
        print("no error")
    } catch e {
        print("wait cancelled caught")
    }
}
main()"#,
        )
        .unwrap();
        assert_eq!(
            output,
            vec!["null", "wait failed caught", "wait cancelled caught"]
        );
    }

    /// Req 13 (§110): task finishes before the limit → normal result.
    #[test]
    fn test_job015_timeout_success() {
        let output = run_source(
            r#"async function quick() { await task.sleep(10)
    return "ok" }
async function main() {
    let t = quick()
    let r = await task.timeout(t, 5000)
    print(r)
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["ok"]);
    }

    /// Req 14 (§110): timeout → ERROR, and the timed-out task is NOT
    /// cancelled — it keeps running and can still finish normally.
    #[test]
    fn test_job015_timeout_error_task_keeps_running() {
        let output = run_source(
            r#"async function slow() { await task.sleep(500)
    print("slow done")
    return "slow" }
async function main() {
    let t = slow()
    try {
        await task.timeout(t, 10)
        print("no timeout")
    } catch e {
        print("timeout caught")
    }
    print(task.state(t))
    await task.wait(t)
    print("finished")
}
main()"#,
        )
        .unwrap();
        assert_eq!(
            output,
            vec!["timeout caught", "running", "slow done", "finished"]
        );
    }

    /// Req 15 (§111, §112): completion order follows sleep duration, not
    /// creation order — mirrors the §112 example (1000 ms vs 500 ms).
    #[test]
    fn test_job015_sleep_ordering_not_creation_order() {
        let output = run_source(
            r#"async function A() { await task.sleep(1000)
    print("A") }
async function B() { await task.sleep(500)
    print("B") }
async function main() {
    let taskA = A()
    let taskB = B()
    await task.all([taskA, taskB])
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["B", "A"]);
    }

    /// Req 16 (§111): task.sleep(0) does not crash or hang; it yields to
    /// other runnable tasks before resuming.
    #[test]
    fn test_job015_sleep_zero_yields() {
        let output = run_source(
            r#"async function other() { print("other") }
async function main() {
    let o = other()
    await task.sleep(0)
    print("main")
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["other", "main"]);
    }

    /// Extra: task.cancel() on a non-task argument → ERROR (§105 requires
    /// an asynchronous task).
    #[test]
    fn test_job015_cancel_non_task_error() {
        let output = run_source(
            r#"async function main() {
    try {
        task.cancel(42)
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught"]);
    }

    /// Extra (ISSUES §64b): cancelling the currently running task from
    /// inside its own body → runtime ERROR (spec silent; a single-threaded
    /// scheduler cannot unwind itself).
    #[test]
    fn test_job015_self_cancel_is_error() {
        let output = run_source(
            r#"async function selfy() { task.cancel(t)
    print("after") }
let t = selfy()
async function main() {
    try {
        await t
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught"]);
    }

    /// Extra (ISSUES §64c): a cycle of tasks awaiting each other makes no
    /// progress and has no timers → runtime ERROR instead of a hang
    /// (spec silent on await cycles).
    #[test]
    fn test_job015_await_cycle_deadlock_error() {
        let err = run_source(
            r#"async function a() { await tb }
async function b() { await ta }
let ta = a()
let tb = b()
async function main() { await ta }
main()"#,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("deadlock"),
            "unexpected error: {}",
            err
        );
    }

    /// Extra: negative sleep duration → ERROR (§111: non-negative integer).
    #[test]
    fn test_job015_sleep_negative_error() {
        let output = run_source(
            r#"async function main() {
    try {
        await task.sleep(-1)
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught"]);
    }

    // ==================== Job 016: Standard Library HTTP ====================

    /// One request recorded by the deterministic local mock server.
    struct RecordedRequest {
        method: String,
        target: String,
        headers: Vec<(String, String)>,
        body: String,
    }

    struct MockReply {
        status: u16,
        headers: Vec<(String, String)>,
        body: String,
        /// Accept the request but never respond (timeout tests).
        hang: bool,
    }

    /// Start a localhost HTTP/1.1 server that records every request and
    /// answers with `handler`'s reply. Returns the base URL and the shared
    /// recording log. Handlers run inline in the accept loop (tests are
    /// sequential), `hang` sleeps bounded before dropping the connection.
    fn spawn_mock_server<F>(handler: F) -> (String, Arc<Mutex<Vec<RecordedRequest>>>)
    where
        F: Fn(&RecordedRequest) -> MockReply + Send + 'static,
    {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let records: Arc<Mutex<Vec<RecordedRequest>>> = Arc::new(Mutex::new(Vec::new()));
        let rec2 = records.clone();
        thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(mut stream) = conn else { continue };
                let mut buf: Vec<u8> = Vec::new();
                let mut chunk = [0u8; 1024];
                let header_end = loop {
                    if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break Some(p);
                    }
                    match stream.read(&mut chunk) {
                        Ok(0) => break None,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        Err(_) => break None,
                    }
                    if buf.len() > 64 * 1024 {
                        break None;
                    }
                };
                let Some(header_end) = header_end else { continue };
                let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
                let mut lines = head.split("\r\n");
                let request_line = lines.next().unwrap_or("");
                let mut parts = request_line.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let target = parts.next().unwrap_or("").to_string();
                let mut headers = Vec::new();
                for line in lines {
                    if let Some((k, v)) = line.split_once(':') {
                        headers.push((k.trim().to_string(), v.trim().to_string()));
                    }
                }
                let content_length = headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, v)| v.parse::<usize>().ok())
                    .unwrap_or(0);
                let mut body = buf[header_end + 4..].to_vec();
                while body.len() < content_length {
                    match stream.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => body.extend_from_slice(&chunk[..n]),
                        Err(_) => break,
                    }
                }
                let rec = RecordedRequest {
                    method,
                    target,
                    headers,
                    body: String::from_utf8_lossy(&body).to_string(),
                };
                let reply = handler(&rec);
                rec2.lock().unwrap().push(rec);
                if reply.hang {
                    thread::sleep(Duration::from_secs(3));
                    continue;
                }
                let reason = match reply.status {
                    200 => "OK",
                    404 => "Not Found",
                    500 => "Internal Server Error",
                    _ => "Status",
                };
                let mut resp = format!("HTTP/1.1 {} {}\r\n", reply.status, reason);
                for (k, v) in &reply.headers {
                    resp.push_str(&format!("{}: {}\r\n", k, v));
                }
                resp.push_str(&format!(
                    "Content-Length: {}\r\nConnection: close\r\n\r\n",
                    reply.body.len()
                ));
                resp.push_str(&reply.body);
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
        });
        (format!("http://127.0.0.1:{}", port), records)
    }

    fn plain_reply(body: &str) -> MockReply {
        MockReply {
            status: 200,
            headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
            body: body.to_string(),
            hang: false,
        }
    }

    /// Req 1 (§50): http.get returns res.status (number), res.headers
    /// (object), res.body (string).
    #[test]
    fn test_job016_http_get_basic_response() {
        let (base, _recs) = spawn_mock_server(|_| plain_reply("hello world"));
        let src = r#"async function main() {
    let res = await http.get("__URL__/data")
    print(res.status)
    print(res.body)
    print(type(res.status))
    print(type(res.headers))
    print(type(res.body))
    print(res.headers)
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(
            &output[..5],
            ["200", "hello world", "number", "object", "string"]
        );
        // Header key casing is preserved exactly as the server sent it.
        let h = output[5].to_lowercase();
        assert!(h.contains("content-type"), "headers: {}", output[5]);
        assert!(h.contains("text/plain"), "headers: {}", output[5]);
    }

    /// Req 2 (§50, §50.1): post/put/patch/delete carry body + headers.
    #[test]
    fn test_job016_http_methods_with_body_and_headers() {
        let (base, recs) = spawn_mock_server(|r| MockReply {
            status: 200,
            headers: vec![],
            body: format!("got {}", r.method),
            hang: false,
        });
        let src = r#"async function main() {
    let opts = {
        headers: { "Content-Type": "application/json", "X-Token": "abc" },
        body: "{\"a\":1}"
    }
    print((await http.post("__URL__/a", opts)).body)
    print((await http.put("__URL__/b", opts)).body)
    print((await http.patch("__URL__/c", opts)).body)
    print((await http.delete("__URL__/d", opts)).body)
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["got POST", "got PUT", "got PATCH", "got DELETE"]);
        let recs = recs.lock().unwrap();
        assert_eq!(recs.len(), 4);
        let methods: Vec<&str> = recs.iter().map(|r| r.method.as_str()).collect();
        assert_eq!(methods, ["POST", "PUT", "PATCH", "DELETE"]);
        let targets: Vec<&str> = recs.iter().map(|r| r.target.as_str()).collect();
        assert_eq!(targets, ["/a", "/b", "/c", "/d"]);
        for r in recs.iter() {
            assert_eq!(r.body, r#"{"a":1}"#);
            assert!(r
                .headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("content-type") && v == "application/json"));
            assert!(r
                .headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("x-token") && v == "abc"));
        }
    }

    /// Req 3 (§50, §51): res.body is always a string — a JSON payload is
    /// never auto-parsed into an object.
    #[test]
    fn test_job016_http_body_never_auto_parsed() {
        let (base, _recs) = spawn_mock_server(|_| MockReply {
            status: 200,
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: r#"{"n":1}"#.to_string(),
            hang: false,
        });
        let src = r#"async function main() {
    let res = await http.get("__URL__/j")
    print(type(res.body))
    print(res.body)
    let data = json.parse(res.body)
    print(data.n)
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["string", r#"{"n":1}"#, "1"]);
    }

    /// Req 4 (§50.1): 4xx/5xx are NOT runtime errors — res.status reads
    /// normally.
    #[test]
    fn test_job016_http_4xx_5xx_are_normal_responses() {
        let (base, _recs) = spawn_mock_server(|r| MockReply {
            status: if r.target.contains("missing") { 404 } else { 500 },
            headers: vec![],
            body: "e".to_string(),
            hang: false,
        });
        let src = r#"async function main() {
    let a = await http.get("__URL__/missing")
    print(a.status)
    let b = await http.get("__URL__/boom")
    print(b.status)
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["404", "500"]);
    }

    /// Req 5 (§59): query values are URL-encoded (incl. null → empty),
    /// existing query in the URL is preserved (appended, not replaced).
    #[test]
    fn test_job016_http_query_encoding() {
        let (base, recs) = spawn_mock_server(|_| plain_reply("ok"));
        let src = r#"async function main() {
    let res = await http.get("__URL__/s?sort=name", {
        query: { page: 2, active: true, q: "a b&c", empty: null }
    })
    print(res.status)
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["200"]);
        let recs = recs.lock().unwrap();
        assert_eq!(
            recs[0].target,
            "/s?sort=name&page=2&active=true&q=a%20b%26c&empty="
        );
    }

    /// Req 6 (§59): query that is not an object → ERROR (request never
    /// leaves the process).
    #[test]
    fn test_job016_http_query_non_object_error() {
        let (base, recs) = spawn_mock_server(|_| plain_reply("ok"));
        let src = r#"async function main() {
    try {
        let r = await http.get("__URL__/q", { query: 5 })
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["caught"]);
        assert!(recs.lock().unwrap().is_empty());
    }

    /// Req 7 (§60): request headers are sent exactly as given.
    #[test]
    fn test_job016_http_request_headers_sent_as_given() {
        let (base, recs) = spawn_mock_server(|_| plain_reply("ok"));
        let src = r#"async function main() {
    let res = await http.get("__URL__/h", {
        headers: { "Authorization": "Bearer token", "Accept": "application/json" }
    })
    print(res.status)
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["200"]);
        let recs = recs.lock().unwrap();
        assert!(recs[0].headers.iter().any(|(k, v)| {
            k.eq_ignore_ascii_case("authorization") && v == "Bearer token"
        }));
        assert!(recs[0]
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("accept") && v == "application/json"));
    }

    /// Req 8 (§60): headers that is not an object → ERROR.
    #[test]
    fn test_job016_http_headers_non_object_error() {
        let (base, recs) = spawn_mock_server(|_| plain_reply("ok"));
        let src = r#"async function main() {
    try {
        let r = await http.get("__URL__/h", { headers: 5 })
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["caught"]);
        assert!(recs.lock().unwrap().is_empty());
    }

    /// Req 9 (§60): a non-string header value → ERROR.
    #[test]
    fn test_job016_http_header_value_non_string_error() {
        let (base, recs) = spawn_mock_server(|_| plain_reply("ok"));
        let src = r#"async function main() {
    try {
        let r = await http.get("__URL__/h", { headers: { "X-Bad": 5 } })
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["caught"]);
        assert!(recs.lock().unwrap().is_empty());
    }

    /// Req 10 (§50.1): timeout → runtime error, catchable with try/catch
    /// (server accepts but never responds).
    #[test]
    fn test_job016_http_timeout_is_catchable_error() {
        let (base, _recs) = spawn_mock_server(|_| MockReply {
            status: 200,
            headers: vec![],
            body: String::new(),
            hang: true,
        });
        let src = r#"async function main() {
    try {
        let r = await http.get("__URL__/slow", { timeout: 150 })
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#
        .replace("__URL__", &base);
        let started = std::time::Instant::now();
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["caught"]);
        // The failure came from the 150 ms timeout, not a hang.
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    /// Req 11: network failure (connection refused) → runtime error,
    /// catchable.
    #[test]
    fn test_job016_http_network_failure_catchable() {
        use std::net::TcpListener;
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l); // port closed → connection refused
        let src = r#"async function main() {
    try {
        let r = await http.get("http://127.0.0.1:9nope/")
        print("no error")
    } catch e {
        print("bad url caught")
    }
    try {
        let r = await http.get("http://127.0.0.1:__PORT__/")
        print("no error")
    } catch e {
        print("refused caught")
    }
}
main()"#
        .replace("__PORT__", &port.to_string());
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["bad url caught", "refused caught"]);
    }

    /// Req 12 (§50.1 example): json.stringify as request body, json.parse
    /// on the response — no automatic conversion at the HTTP layer.
    #[test]
    fn test_job016_http_json_stringify_body_roundtrip() {
        let (base, recs) = spawn_mock_server(|r| MockReply {
            status: 200,
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: r.body.clone(),
            hang: false,
        });
        let src = r#"async function main() {
    let payload = json.stringify({ nama: "Kael" })
    let res = await http.post("__URL__/users", {
        headers: { "Content-Type": "application/json" },
        body: payload
    })
    if res.status == 200 {
        let data = json.parse(res.body)
        print(data.nama)
    }
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["Kael"]);
        let recs = recs.lock().unwrap();
        assert_eq!(recs[0].body, r#"{"nama":"Kael"}"#);
    }

    /// Extra: options itself must be an object (mechanical rule; §59/§60
    /// only define field-level errors).
    #[test]
    fn test_job016_http_options_non_object_error() {
        let (base, recs) = spawn_mock_server(|_| plain_reply("ok"));
        let src = r#"async function main() {
    try {
        let r = await http.get("__URL__/o", 5)
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["caught"]);
        assert!(recs.lock().unwrap().is_empty());
    }
}
