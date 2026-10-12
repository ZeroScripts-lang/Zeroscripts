use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::time::Instant;

use crate::compiler::{BytecodeProgram, Constant, FunctionInfo, Opcode, FINALLY_MARKER};
use crate::gc::Gc;
use crate::heap::{EnvBinding, Heap, HeapObject, HeapRef as HeapRefHandle};
use crate::native;

mod error;
mod helpers;
mod http;
mod task;
mod ops;
mod types;
mod validate;
mod value;

pub use error::VmError;
pub use types::{ExecutionState, Limits, TaskState};
pub use value::Value;
use error::classify_runtime_error;
use helpers::{
    duration_ms, expect_list_arg, expect_object_arg, expect_string_arg,
    expect_task_arg, raw_eq, type_name,
};

use types::{
    collect_binding_refs, collect_frame_refs, collect_value_refs, Binding, ExportBinding,
    FinallyMarker, Frame, HttpOutcome, HttpSlot, Task, TaskContext, TaskKind, TryHandler,
    TryPhase,
};


/// Cached name-resolution result for one instruction pc (Job 030).
/// Keyed by `cache_gen` — invalid whenever the active instruction stream
/// changes (function-body swap/restore, default body, task context load).
#[derive(Clone, Copy)]
enum ResCache {
    Global(usize),
    Local { dist: u32, scope_len: u32 },
}

#[derive(Clone, Copy)]
struct ResEntry {
    generation: u64,
    slot: Option<ResCache>,
}

pub struct ZeroVM {
    program: BytecodeProgram,
    pc: usize,
    /// §116: value-stack values not owned by any frame. Normally empty —
    /// every frame keeps its operands in `Frame.stack`. Non-empty only in
    /// the degenerate top-level-return window after the root frame was
    /// popped, preserving the pre-refactor paths (ISSUES §75).
    orphan_stack: Vec<Value>,
    call_stack: Vec<Frame>,
    state: ExecutionState,
    output: Vec<String>,
    err_output: Vec<String>,
    heap: Heap,
    gc: Gc,
    /// §121 "reference yang sedang digunakan oleh runtime": pins for heap
    /// refs held only on the Rust side (currently the run_modules export
    /// cache). Everything else is scanned structurally by gc_roots.
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
    /// loop fields (pc/instructions/call_stack/handlers; value stacks ride
    /// inside their frames).
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
    /// Append-only after `clear()` (Job 030) so binding indices stay stable
    /// and cacheable.
    global_scope: Vec<(String, Binding)>,
    /// Per-pc name-resolution cache (Job 030): O(1) lookup after the first
    /// resolve of an instruction instead of hashing/walking every execution.
    resolve_cache: Vec<ResEntry>,
    /// Generation counter for `resolve_cache`; bumped on every
    /// `program.instructions` swap and on global-scope reset.
    cache_gen: u64,
    /// §150 execution limits + §133 heap limit — host configuration only
    /// (§149, ISSUES §71g); `None` fields = unlimited (ISSUES §71f default).
    limits: Limits,
    /// Dispatched-instruction counter for the §150 instruction-count limit
    /// (ISSUES §71c) and the amortized wall-time sample (ISSUES §71b).
    instructions_executed: u64,
    /// Wall-clock origin stamped at run/run_modules entry — §150 execution
    /// time is real elapsed resource time, not the virtual `now_ms` (§71b).
    execution_started: Option<Instant>,
}

impl ZeroVM {
    pub fn new(program: BytecodeProgram) -> Self {
        Self {
            program,
            pc: 0,
            orphan_stack: Vec::new(),
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
            global_scope: Vec::new(),
            resolve_cache: Vec::new(),
            cache_gen: 0,
            limits: Limits::default(),
            instructions_executed: 0,
            execution_started: None,
        }
    }

    /// Set program arguments for system.args(). Per §52 the host is
    /// responsible for excluding the interpreter and script file names
    /// (e.g. `zerovm app.zs hello world` → `["hello", "world"]`).
    pub fn set_program_args(&mut self, args: Vec<String>) {
        self.program_args = args;
    }

    /// Configure the §150 execution limits / §133 heap limit. Host-side only
    /// (§149; the program cannot change limits, ISSUES §71g); `None` fields
    /// stay unlimited. Invalid values (any limit = 0) are rejected here and
    /// again at run/run_modules entry before the first instruction (§149
    /// startup error, ISSUES §71f).
    pub fn set_limits(&mut self, limits: Limits) -> Result<(), VmError> {
        limits.validate()?;
        self.limits = limits;
        Ok(())
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
        let roots = self.gc_roots();
        self.gc.collect(&mut self.heap, &roots);

        self.live_refs.retain(|r| self.heap.is_valid(r));
    }

    /// Root set for the mark phase — §121 "Root Reference" and §131
    /// "Reference Sources": the value stack (local variables, parameters as
    /// pushed/stored values), active execution frames (scope bindings,
    /// captured/scope environments, live bindings), global bindings,
    /// module bindings (via live bindings + the pinned export cache),
    /// pending return values, running/suspended tasks (kind args/callee/
    /// env, stored results, saved contexts) and `live_refs` — references
    /// held only on the Rust side (§121 "reference yang sedang digunakan
    /// oleh runtime"), e.g. the `run_modules` export cache.
    ///
    /// Roots are scanned only at instruction boundaries (the execution
    /// loops call `maybe_gc` between instructions): during one instruction
    /// no collection can fire, so values temporarily held in Rust locals
    /// (popped operands, recursion inside natives) are safe without pins —
    /// §120/§121 leave GC timing to the implementation.
    fn gc_roots(&self) -> Vec<HeapRefHandle> {
        let mut roots = Vec::new();
        for value in &self.orphan_stack {
            collect_value_refs(value, &mut roots);
        }
        for frame in &self.call_stack {
            collect_frame_refs(frame, &mut roots);
        }
        for (_, binding) in &self.global_scope {
            collect_binding_refs(binding, &mut roots);
        }
        if let Some(value) = &self.pending_return {
            collect_value_refs(value, &mut roots);
        }
        for task in &self.tasks {
            if let TaskKind::Normal {
                args,
                callee,
                env,
                ..
            } = &task.kind
            {
                for value in args {
                    collect_value_refs(value, &mut roots);
                }
                collect_value_refs(callee, &mut roots);
                if let Some(e) = env {
                    roots.push(e.clone());
                }
            }
            if let Some(value) = &task.result {
                collect_value_refs(value, &mut roots);
            }
            if let Some(ctx) = &task.context {
                for frame in &ctx.call_stack {
                    collect_frame_refs(frame, &mut roots);
                }
                if let Some(value) = &ctx.pending_return {
                    collect_value_refs(value, &mut roots);
                }
            }
        }
        roots.extend(self.live_refs.iter().cloned());
        roots
    }

    pub fn heap_alloc(&mut self, object: HeapObject) -> HeapRefHandle {
        let reference = self.heap.allocate(object);
        self.gc.on_allocation();
        reference
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


    /// Install built-in bindings into the shared global scope (§42-§53).
    fn install_builtins(&mut self) {
        for name in native::BUILTIN_ROOTS {
            let binding = Binding {
                value: Value::BuiltIn(name.to_string()),
                is_const: true,
                initialized: true,
                live: None,
            };
            match self.global_index(name) {
                Some(idx) => self.global_scope[idx].1 = binding,
                None => self.global_scope.push((name.to_string(), binding)),
            }
        }
    }

    // ------------------------------------------------------------------
    // Name-resolution cache (Job 030)
    // ------------------------------------------------------------------

    fn global_index(&self, name: &str) -> Option<usize> {
        self.global_scope.iter().position(|(k, _)| k == name)
    }

    fn res_get(&self, pc: usize) -> Option<ResCache> {
        self.resolve_cache
            .get(pc)
            .filter(|e| e.generation == self.cache_gen)
            .and_then(|e| e.slot)
    }

    fn res_put(&mut self, pc: usize, slot: ResCache) {
        let entry = ResEntry {
            generation: self.cache_gen,
            slot: Some(slot),
        };
        if pc >= self.resolve_cache.len() {
            self.resolve_cache
                .resize(pc + 1, ResEntry { generation: 0, slot: None });
        }
        self.resolve_cache[pc] = entry;
    }

    /// Called whenever `program.instructions` is replaced — the pc space of
    /// the previous stream must never be read against the new one (Job 030).
    fn bump_cache_gen(&mut self) {
        self.cache_gen = self.cache_gen.wrapping_add(1);
    }

    /// Reset cache + global scope between runs: `clear()` drops bindings the
    /// cached indices point at, and a new run re-executes every pc anyway.
    fn reset_run_state(&mut self) {
        self.global_scope.clear();
        self.install_builtins();
        self.resolve_cache.clear();
        self.resolve_cache.resize(
            self.program.instructions.len(),
            ResEntry { generation: 0, slot: None },
        );
        self.bump_cache_gen();
    }

    /// Swap in a new instruction stream and invalidate per-pc cache entries.
    fn swap_instructions(&mut self, instructions: Vec<Opcode>) {
        self.program.instructions = instructions;
        self.bump_cache_gen();
    }

    /// §150/§133 enforcement at the instruction boundary: instruction
    /// count, amortized wall time, and heap occupancy with GC-first (§133,
    /// ISSUES §71d). Call depth and task count are checked at their push
    /// sites. Runtime-limit failures are ordinary `RuntimeError`s so the
    /// existing catch machinery applies (ISSUES §71a); the memory limit
    /// aborts with a VM Error.
    fn check_execution_limits(&mut self) -> Result<(), VmError> {
        self.instructions_executed = self.instructions_executed.wrapping_add(1);

        if let Some(max) = self.limits.instruction_count {
            if self.instructions_executed > max {
                return Err(VmError::RuntimeError(format!(
                    "Execution limit exceeded: instruction count {}",
                    max
                )));
            }
        }

        if let Some(max_ms) = self.limits.execution_time_ms {
            // Amortized sample (ISSUES §71b): every 1024 instructions the
            // real clock is read; idle time is covered by the scheduler's
            // park-loop check instead.
            if self.instructions_executed % 1024 == 0 {
                if let Some(start) = self.execution_started {
                    if start.elapsed().as_millis() > max_ms as u128 {
                        return Err(VmError::RuntimeError(format!(
                            "Execution limit exceeded: wall clock time {}ms",
                            max_ms
                        )));
                    }
                }
            }
        }

        if let Some(max_slots) = self.limits.heap_slots {
            if self.heap.occupied_slots() > max_slots {
                // §133: "Garbage collection dapat dijalankan sebelum runtime
                // menyatakan allocation gagal" — GC first, then re-measure.
                self.run_gc();
                let occupied = self.heap.occupied_slots();
                if occupied > max_slots {
                    return Err(VmError::MemoryLimit(format!(
                        "{} heap slots occupied, limit {}",
                        occupied, max_slots
                    )));
                }
            }
        }

        Ok(())
    }

    /// §150 call stack depth: the call that would push frame `max + 1`
    /// (root frame included) is rejected before it happens (ISSUES §71a).
    fn check_call_depth(&self) -> Result<(), VmError> {
        if let Some(max) = self.limits.call_stack_depth {
            if self.call_stack.len() + 1 > max {
                return Err(VmError::RuntimeError(format!(
                    "Execution limit exceeded: call stack depth {}",
                    max
                )));
            }
        }
        Ok(())
    }

    /// §150 wall-time check for scheduler idle paths where no instruction
    /// boundary runs (HTTP park, ISSUES §71b). No execution context is
    /// loaded there, so the failure reaches the host uncaught (ISSUES §71a,
    /// §153 termination).
    fn check_idle_time_limit(&self) -> Result<(), VmError> {
        if let Some(max_ms) = self.limits.execution_time_ms {
            if let Some(start) = self.execution_started {
                if start.elapsed().as_millis() > max_ms as u128 {
                    return Err(VmError::RuntimeError(format!(
                        "Execution limit exceeded: wall clock time {}ms",
                        max_ms
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn run(&mut self) -> Result<(), VmError> {
        self.validate()?;
        self.limits.validate()?;
        self.instructions_executed = 0;
        self.execution_started = Some(Instant::now());
        self.state = ExecutionState::Running;
        self.pc = self.program.entry_point;

        // Globals live in `global_scope` (shared across task contexts); the
        // root frame only hosts the top-level lexical scope stack.
        self.reset_run_state();
        self.call_stack.push(Frame {
            function_index: 0,
            scopes: vec![HashMap::new()],
            stack: Vec::new(),
            return_pc: 0,
            captured_env: None,
            scope_env: None,
            return_instructions: None,
            task_id: None,
        });

        while self.state == ExecutionState::Running {
            // GC boundary (§121): all live values sit in scanned roots
            // (stack/frames/globals/tasks) between instructions.
            self.maybe_gc();
            if self.pc >= self.program.instructions.len() {
                self.state = ExecutionState::Completed;
                break;
            }

            let instruction = self.program.instructions[self.pc].clone();
            let instr_pc = self.pc;
            self.pc += 1;

            if let Err(err) = self.execute_instruction(instr_pc, &instruction) {
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
        self.limits.validate()?;
        self.instructions_executed = 0;
        self.execution_started = Some(Instant::now());
        self.state = ExecutionState::Running;

        self.reset_run_state();
        self.call_stack.push(Frame {
            function_index: 0,
            scopes: vec![HashMap::new()],
            stack: Vec::new(),
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
                        // Module bindings stay active until run_modules
                        // returns — pin the exported value for the cache
                        // (§121 "module bindings yang masih aktif").
                        self.track_value_heap_refs(&b.value);
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
        loop {
            // GC boundary (§121) — same safe point as the main loop.
            self.maybe_gc();
            if self.pc >= self.program.instructions.len() {
                break;
            }
            // Job 029: while a function/eval frame is active, Call has swapped
            // `program.instructions` to that body and `pc` lives in the body's
            // address space — the module's top-level `end` no longer bounds it
            // (body-relative jumps could exceed `end` and silently kill the
            // loop before Return restored the caller). The root frame still
            // ends at the module's `end`.
            let in_body_frame = self
                .call_stack
                .last()
                .map(|f| f.return_instructions.is_some())
                .unwrap_or(false);
            if !in_body_frame && self.pc >= end {
                break;
            }
            let instruction = self.program.instructions[self.pc].clone();
            let instr_pc = self.pc;
            self.pc += 1;
            if matches!(instruction, Opcode::EndModule) {
                break;
            }
            if let Err(err) = self.execute_instruction(instr_pc, &instruction) {
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

    /// §116 view of the active frame's value stack (or the orphan slot when
    /// no frame exists). At program end the root frame holds exactly what
    /// the old shared stack held, so embedder/tests see the same slice.
    pub fn get_stack(&self) -> &[Value] {
        self.stack()
    }

    /// §116: value stack of the active execution frame. Falls back to the
    /// orphan slot only while no frame exists (top-level return window).
    fn stack(&self) -> &Vec<Value> {
        match self.call_stack.last() {
            Some(frame) => &frame.stack,
            None => &self.orphan_stack,
        }
    }

    fn stack_mut(&mut self) -> &mut Vec<Value> {
        if let Some(frame) = self.call_stack.last_mut() {
            &mut frame.stack
        } else {
            &mut self.orphan_stack
        }
    }

    fn execute_instruction(&mut self, pc: usize, instruction: &Opcode) -> Result<(), VmError> {
        // §150/§133: enforce configured limits before dispatching (ISSUES §71).
        self.check_execution_limits()?;
        match instruction {
            Opcode::LoadConst(idx) => self.execute_load_const(*idx),
            Opcode::DeclareLocal(name, is_const) => self.execute_declare_local(name, *is_const),
            Opcode::StoreLocal(name) => self.execute_store_local(pc, name),
            Opcode::LoadLocal(name) => self.execute_load_local(pc, name),
            Opcode::DeclareGlobal(name, is_const) => self.execute_declare_global(name, *is_const),
            Opcode::LoadGlobal(name) => self.execute_load_global(pc, name),
            Opcode::StoreGlobal(name) => self.execute_store_global(pc, name),
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
            Opcode::CallNative(operation, argc) => self.execute_call_native(operation, *argc),
            Opcode::Await => self.execute_await(),
            Opcode::Return => self.execute_return(),
            Opcode::LoadFunction(idx) => self.execute_load_function(*idx),
            Opcode::MakeClosure(func_idx, captured) => {
                self.execute_make_closure(*func_idx, captured.as_slice())
            }
            Opcode::LoadCaptured(name) => self.execute_load_captured(name),
            Opcode::StoreCaptured(name) => self.execute_store_captured(name),
            Opcode::MakeList(size) => self.execute_make_list(*size),
            Opcode::MakeObject(keys) => self.execute_make_object(keys),
            Opcode::ListAdd(argc) => self.execute_list_add(*argc),
            Opcode::ListRemove(argc) => self.execute_list_remove(*argc),
            Opcode::ObjectRemove(argc) => self.execute_object_remove(*argc),
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
            // Unreachable after validation (§124); kept so a hand-built or
            // mutated program fails instead of silently doing nothing.
            Opcode::Unknown(tag) => Err(VmError::InvalidOpcode(format!(
                "Unknown opcode {}",
                tag
            ))),
        }
    }

    fn execute_load_const(&mut self, idx: usize) -> Result<(), VmError> {
        if idx >= self.program.constants.len() {
            return Err(VmError::InvalidConstantIndex(idx));
        }
        let value = self.constant_to_value(&self.program.constants[idx]);
        self.stack_mut().push(value);
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

    fn execute_store_local(&mut self, pc: usize, name: &str) -> Result<(), VmError> {
        let value = self.pop_stack("store local")?;
        let cached = self.res_get(pc);

        if let Some(frame) = self.call_stack.last_mut() {
            let scope_len = frame.scopes.len();

            // Cached distance (Job 030): one get_mut and no walk, valid only
            // while the scope depth matches the depth at resolve time.
            if let Some(ResCache::Local { dist, scope_len: l }) = cached {
                if l as usize == scope_len && (scope_len as u32) > dist {
                    let idx = scope_len - 1 - dist as usize;
                    if let Some(binding) = frame.scopes[idx].get_mut(name) {
                        if binding.is_const && binding.initialized {
                            return Err(VmError::RuntimeError(format!(
                                "Cannot assign to const '{}'",
                                name
                            )));
                        }
                        binding.value = value.clone();
                        binding.initialized = true;
                        let write_env = frame.scope_env.clone();
                        self.res_put(
                            pc,
                            ResCache::Local { dist, scope_len: scope_len as u32 },
                        );
                        // Keep shared closure env in sync when this frame owns it (§66).
                        if let Some(env) = write_env {
                            if let Ok(Some(_)) = self.heap.get_env_binding(&env, name) {
                                self.store_in_env(&env, name, value.clone())?;
                            }
                        }
                        return Ok(());
                    }
                }
            }

            // Resolve innermost-first in the same pass as the write.
            let mut resolved: Option<u32> = None;
            for (i, scope) in frame.scopes.iter_mut().rev().enumerate() {
                if let Some(binding) = scope.get_mut(name) {
                    if binding.is_const && binding.initialized {
                        return Err(VmError::RuntimeError(format!(
                            "Cannot assign to const '{}'",
                            name
                        )));
                    }
                    binding.value = value.clone();
                    binding.initialized = true;
                    resolved = Some(i as u32);
                    break;
                }
            }
            if let Some(dist) = resolved {
                let write_env = frame.scope_env.clone();
                self.res_put(
                    pc,
                    ResCache::Local { dist, scope_len: scope_len as u32 },
                );
                if let Some(env) = write_env {
                    if let Ok(Some(_)) = self.heap.get_env_binding(&env, name) {
                        self.store_in_env(&env, name, value.clone())?;
                    }
                }
                return Ok(());
            }

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
                self.res_put(
                    pc,
                    ResCache::Local { dist: 0, scope_len: scope_len as u32 },
                );
            }
        }

        Ok(())
    }

    /// Read a binding's value, following a live import source when present
    /// (§66/§74). Falls back to the stored value if the source env is gone.
    fn read_binding_value(heap: &Heap, binding: &Binding, name: &str) -> Result<Value, VmError> {
        if let Some((env, env_name)) = &binding.live {
            if let Ok(Some(heap_ref)) = heap.get_env_binding(env, env_name) {
                return Value::from_heap_ref(&heap_ref, heap).map_err(|_| {
                    VmError::RuntimeError(format!("Failed to convert heap value for '{}'", name))
                });
            }
        }
        Ok(binding.value.clone())
    }

    fn execute_load_local(&mut self, pc: usize, name: &str) -> Result<(), VmError> {
        let cached = self.res_get(pc);
        let mut resolved: Option<(u32, usize, Value)> = None;

        if let Some(frame) = self.call_stack.last() {
            let scope_len = frame.scopes.len();

            if let Some(ResCache::Local { dist, scope_len: l }) = cached {
                if l as usize == scope_len && (scope_len as u32) > dist {
                    let idx = scope_len - 1 - dist as usize;
                    if let Some(binding) = frame.scopes[idx].get(name) {
                        let value = Self::read_binding_value(&self.heap, binding, name)?;
                        resolved = Some((dist, scope_len, value));
                    }
                }
            }
            if resolved.is_none() {
                for (i, scope) in frame.scopes.iter().rev().enumerate() {
                    if let Some(binding) = scope.get(name) {
                        let value = Self::read_binding_value(&self.heap, binding, name)?;
                        resolved = Some((i as u32, scope_len, value));
                        break;
                    }
                }
            }
        }

        let (dist, scope_len, value) = resolved.ok_or_else(|| {
            VmError::RuntimeError(format!("Undefined variable '{}'", name))
        })?;
        self.res_put(pc, ResCache::Local { dist, scope_len: scope_len as u32 });
        self.stack_mut().push(value);
        Ok(())
    }

    fn execute_declare_global(&mut self, name: &str, is_const: bool) -> Result<(), VmError> {
        if self.global_index(name).is_some() {
            if native::is_builtin_root(name) {
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
        self.global_scope.push((
            name.to_string(),
            Binding {
                value: Value::Null,
                is_const,
                initialized: false,
                live: None,
            },
        ));
        Ok(())
    }

    fn execute_load_global(&mut self, pc: usize, name: &str) -> Result<(), VmError> {
        let idx = match self.res_get(pc) {
            Some(ResCache::Global(i))
                if i < self.global_scope.len() && self.global_scope[i].0 == name =>
            {
                i
            }
            _ => {
                let i = self.global_index(name).ok_or_else(|| {
                    VmError::RuntimeError(format!("Undefined variable '{}'", name))
                })?;
                self.res_put(pc, ResCache::Global(i));
                i
            }
        };
        let value = self.global_scope[idx].1.value.clone();
        self.stack_mut().push(value);
        Ok(())
    }

    fn execute_store_global(&mut self, pc: usize, name: &str) -> Result<(), VmError> {
        let value = self.pop_stack("store global")?;

        let idx = match self.res_get(pc) {
            Some(ResCache::Global(i))
                if i < self.global_scope.len() && self.global_scope[i].0 == name =>
            {
                Some(i)
            }
            _ => self.global_index(name),
        };

        if let Some(i) = idx {
            let binding = &mut self.global_scope[i].1;
            if binding.is_const && binding.initialized {
                return Err(VmError::RuntimeError(format!(
                    "Cannot assign to const '{}'",
                    name
                )));
            }
            binding.value = value.clone();
            binding.initialized = true;
            self.res_put(pc, ResCache::Global(i));
        } else {
            // Bare assignment creates a mutable global
            self.global_scope.push((
                name.to_string(),
                Binding {
                    value: value.clone(),
                    is_const: false,
                    initialized: true,
                    live: None,
                },
            ));
            self.res_put(pc, ResCache::Global(self.global_scope.len() - 1));
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
        self.stack_mut().push(result);
        Ok(())
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
                self.stack_mut().push(Value::Number(result));
                Ok(())
            }
            Value::Null => {
                self.stack_mut().push(Value::Number(0.0));
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
                self.stack_mut().push(value);
                Ok(())
            }
            Value::Null => {
                self.stack_mut().push(Value::Number(0.0));
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
                self.stack_mut().push(Value::Boolean(!b));
                Ok(())
            }
            _ => Err(VmError::RuntimeError(format!(
                "Cannot apply ! to {:?}",
                value
            ))),
        }
    }


    fn execute_pop(&mut self) -> Result<(), VmError> {
        self.pop_stack("pop")?;
        Ok(())
    }

    fn execute_dup(&mut self) -> Result<(), VmError> {
        let value = self.peek_stack("dup")?;
        self.stack_mut().push(value);
        Ok(())
    }

    fn execute_swap(&mut self) -> Result<(), VmError> {
        if self.stack().len() < 2 {
            return Err(VmError::StackUnderflow("swap needs 2 values".to_string()));
        }
        let len = self.stack().len();
        self.stack_mut().swap(len - 1, len - 2);
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
        if self.stack().len() < argc + 1 {
            return Err(VmError::StackUnderflow(format!(
                "Call needs {} args + function, stack has {}",
                argc,
                self.stack().len()
            )));
        }

        let func_value = {
            let stack = self.stack();
            stack[stack.len() - argc - 1].clone()
        };

        match &func_value {
            Value::BuiltIn(name) => {
                let result = self.execute_builtin(name, argc)?;
                self.stack_mut().push(result);
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

                // §150 call stack depth — reject before defaults run.
                self.check_call_depth()?;

                // Job 022 (§30): defaults are evaluated at call time.
                let Some(root_scope) =
                    self.build_function_scope(&func_info, &args, &func_value, None)?
                else {
                    // Default evaluation unwound to an outer handler or failed
                    // a task: the call never happens.
                    return Ok(());
                };

                let return_pc = self.pc;
                let return_instructions = self.program.instructions.clone();

                if func_idx < self.program.function_bodies.len() {
                    let body = self.program.function_bodies[func_idx].clone();
                    self.swap_instructions(body);
                }

                self.call_stack.push(Frame {
                    function_index: func_idx,
                    scopes: vec![root_scope],
                    stack: Vec::new(),
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

                // §150 call stack depth — reject before defaults run.
                self.check_call_depth()?;

                // Job 022 (§30): defaults are evaluated at call time, with
                // captured names resolvable through the closure environment.
                let Some(root_scope) = self.build_function_scope(
                    &func_info,
                    &args,
                    &func_value,
                    Some(env.clone()),
                )?
                else {
                    return Ok(());
                };

                let return_pc = self.pc;
                let return_instructions = self.program.instructions.clone();

                if func_idx < self.program.function_bodies.len() {
                    let body = self.program.function_bodies[func_idx].clone();
                    self.swap_instructions(body);
                }

                self.call_stack.push(Frame {
                    function_index: func_idx,
                    scopes: vec![root_scope],
                    stack: Vec::new(),
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

    /// Pop `argc` values pushed as call arguments (no callee on the stack).
    fn pop_native_args(&mut self, argc: usize, context: &str) -> Result<Vec<Value>, VmError> {
        if self.stack().len() < argc {
            return Err(VmError::StackUnderflow(format!(
                "{} needs {} argument(s), stack has {}",
                context,
                argc,
                self.stack().len()
            )));
        }
        let mut args = Vec::with_capacity(argc);
        for _ in 0..argc {
            args.push(self.pop_stack(context)?);
        }
        args.reverse();
        Ok(args)
    }

    /// Push a function value for `CALL` — `LOAD_FUNCTION` (§141).
    fn execute_load_function(&mut self, index: usize) -> Result<(), VmError> {
        if index >= self.program.functions.len() {
            return Err(VmError::InvalidOperand(format!(
                "Function index {} out of range",
                index
            )));
        }
        self.stack_mut().push(Value::Function(index));
        Ok(())
    }

    /// `CALL_NATIVE <operation>` (§145, §146): arguments only, no callee value,
    /// dispatched through the single native-operation boundary.
    fn execute_call_native(&mut self, operation: &str, argc: usize) -> Result<(), VmError> {
        let args = self.pop_native_args(argc, operation)?;
        let result = self.call_native(operation, &args)?;
        self.stack_mut().push(result);
        Ok(())
    }

    /// `LIST_ADD` (§144) — shares its implementation with `list.add`.
    fn execute_list_add(&mut self, argc: usize) -> Result<(), VmError> {
        let args = self.pop_native_args(argc, "list.add")?;
        let result = self.native_list_add(&args)?;
        self.stack_mut().push(result);
        Ok(())
    }

    /// `LIST_REMOVE` (§144) — shares its implementation with `list.remove`.
    fn execute_list_remove(&mut self, argc: usize) -> Result<(), VmError> {
        let args = self.pop_native_args(argc, "list.remove")?;
        let result = self.native_list_remove(&args)?;
        self.stack_mut().push(result);
        Ok(())
    }

    /// `OBJECT_REMOVE` (§144) — shares its implementation with `object.remove`.
    fn execute_object_remove(&mut self, argc: usize) -> Result<(), VmError> {
        let args = self.pop_native_args(argc, "object.remove")?;
        let result = self.native_object_remove(&args)?;
        self.stack_mut().push(result);
        Ok(())
    }

    /// Build a function frame's root scope: parameters (with defaults
    /// evaluated at call time, §30), then the function's own name bound to
    /// `callee` for self-reference. `captured_env` is the closure/task
    /// environment a default expression may read captured names through
    /// (§130).
    ///
    /// Returns `None` when default evaluation unwound to an outer handler or
    /// failed a task instead of completing — the call never happens and
    /// control already continues at that handler.
    fn build_function_scope(
        &mut self,
        func_info: &FunctionInfo,
        args: &[Value],
        callee: &Value,
        captured_env: Option<HeapRefHandle>,
    ) -> Result<Option<HashMap<String, Binding>>, VmError> {
        let mut root_scope = HashMap::new();

        // Bind every explicit argument first: a default then sees earlier
        // parameters regardless of which of them were passed explicitly, and
        // all argument values are reachable from each default's evaluation
        // scope (GC roots, §121 — only instruction boundaries collect).
        for (i, param_name) in func_info.params.iter().enumerate() {
            if i < args.len() {
                root_scope.insert(
                    param_name.clone(),
                    Binding {
                        value: args[i].clone(),
                        is_const: false,
                        initialized: true,
                        live: None,
                    },
                );
            }
        }

        // Left-to-right defaults (§30): each default sees the parameters
        // bound before it; a missing argument without a default is null.
        for (i, param_name) in func_info.params.iter().enumerate() {
            if i < args.len() {
                continue;
            }
            let value = match func_info.param_defaults.get(i).copied().flatten() {
                Some(default_idx) => {
                    match self.evaluate_default_body(default_idx, &root_scope, func_info, callee, captured_env.clone())? {
                        Some(value) => value,
                        // Unwound to a handler / failed task: abort the call.
                        None => return Ok(None),
                    }
                }
                None => Value::Null,
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

        Ok(Some(root_scope))
    }

    /// Job 022 (§30): evaluate one default-parameter expression at call time.
    ///
    /// The body runs in a dedicated frame on the current call stack, driven
    /// exactly like the outer execution loops (GC at instruction boundaries,
    /// `RuntimeError` → `raise_runtime_error`), so `Return` restores the
    /// caller's instruction stream and program counter through the ordinary
    /// frame machinery and an inner `try`/`catch` inside the default's
    /// callees is handled without leaving the evaluation.
    ///
    /// The frame's single scope is a snapshot of the parameters bound so far
    /// (cloned onto the stack, so §121 roots cover it); `captured_env` gives
    /// §130 closure access and `global_scope` stays visible for globals.
    ///
    /// Returns `Ok(None)` when evaluation did not complete normally — the
    /// error unwound to an outer handler or failed a task, so the call must
    /// be aborted (control is already positioned there).
    fn evaluate_default_body(
        &mut self,
        default_idx: usize,
        bound_params: &HashMap<String, Binding>,
        func_info: &FunctionInfo,
        callee: &Value,
        captured_env: Option<HeapRefHandle>,
    ) -> Result<Option<Value>, VmError> {
        if default_idx >= self.program.default_bodies.len() {
            return Err(VmError::InvalidOperand(format!(
                "Default body index {} out of range",
                default_idx
            )));
        }

        // Evaluation scope: parameters bound so far, plus the function's own
        // name so a default may recurse like the body can.
        let mut scope = bound_params.clone();
        if !func_info.name.is_empty() {
            scope.insert(
                func_info.name.clone(),
                Binding {
                    value: callee.clone(),
                    is_const: false,
                    initialized: true,
                    live: None,
                },
            );
        }

        let saved_pc = self.pc;
        let stack_at_start = self.stack().len();
        let default_body = self.program.default_bodies[default_idx].clone();
        let saved_instructions = std::mem::replace(&mut self.program.instructions, default_body);
        self.bump_cache_gen();
        self.call_stack.push(Frame {
            // Default bodies are not functions; nothing reads this index.
            function_index: usize::MAX,
            scopes: vec![scope],
            stack: Vec::new(),
            return_pc: saved_pc,
            captured_env,
            scope_env: None,
            return_instructions: Some(saved_instructions),
            task_id: None,
        });
        self.pc = 0;
        let depth = self.call_stack.len();

        loop {
            self.maybe_gc();
            if self.pc >= self.program.instructions.len() {
                return Err(VmError::InvalidExecutionState(
                    "Default parameter expression ran past its body end".to_string(),
                ));
            }

            let instruction = self.program.instructions[self.pc].clone();
            let instr_pc = self.pc;
            self.pc += 1;

            if let Err(err) = self.execute_instruction(instr_pc, &instruction) {
                // Mirror the outer loops: only ZeroScript runtime errors are
                // catchable (§43, §154); anything else aborts evaluation.
                if let VmError::RuntimeError(msg) = err {
                    self.raise_runtime_error(msg)?;
                } else {
                    return Err(err);
                }
            }

            // The frame's own Return (or an unwind past it) ends evaluation;
            // nested calls made from the body push frames above it.
            if self.call_stack.len() < depth {
                break;
            }
        }

        // Normal completion: the frame was popped by Return, the caller's
        // pc/instructions were restored, and exactly the result was pushed.
        // Anything else (unwind to an outer handler, task failure) is an
        // aborted evaluation — the call must not proceed.
        if self.call_stack.len() == depth - 1
            && self.pc == saved_pc
            && self.stack().len() == stack_at_start + 1
        {
            Ok(Some(self.stack_mut().pop().unwrap_or(Value::Null)))
        } else {
            Ok(None)
        }
    }


    /// Dynamic native call: a `BuiltIn` value reached through `CALL` (for
    /// example a builtin stored in a variable). Static call sites compile to
    /// `CALL_NATIVE` instead; both land in `call_native` (§145, §146).
    fn execute_builtin(&mut self, name: &str, argc: usize) -> Result<Value, VmError> {
        // All BuiltIn values (root functions and namespace methods) pop args
        // and dispatch through call_native — the native-call boundary.
        let mut args = Vec::new();
        for _ in 0..argc {
            args.push(self.pop_stack("builtin arg")?);
        }
        args.reverse();
        self.pop_stack("builtin callee")?;
        self.call_native(name, &args)
    }

    /// The single native-operation boundary (§145, §146): every standard
    /// library and builtin call — compiled statically as `CALL_NATIVE`, or
    /// reached dynamically through a `BuiltIn` value — dispatches here.
    fn call_native(&mut self, name: &str, args: &[Value]) -> Result<Value, VmError> {
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
                // ISSUES §59a: a caught error object prints its message so the
                // output stays a human-readable error line.
                let output = match args.first() {
                    Some(value) => value
                        .error_message(&self.heap)
                        .unwrap_or_else(|| value.format_value(&self.heap)),
                    None => String::new(),
                };
                self.err_output.push(output);
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
                    value => {
                        // ISSUES §59a: string(caught error) is its message;
                        // other objects stay a conversion error (§54/§59c).
                        match value.error_message(&self.heap) {
                            Some(text) => Ok(Value::Str(text)),
                            None => Err(VmError::RuntimeError(format!(
                                "Invalid conversion to string from {:?}",
                                value
                            ))),
                        }
                    }
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
            "list.add" => self.native_list_add(args),
            "list.remove" => self.native_list_remove(args),
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
            "object.remove" => self.native_object_remove(args),
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

    /// `list.add` / `LIST_ADD` (§144, §34): append to a mutable list. Shared by
    /// the core opcode and by the native operation so both behave identically.
    fn native_list_add(&mut self, args: &[Value]) -> Result<Value, VmError> {
        let r = expect_list_arg(&args, 0, "list.add")?;
        if args.len() != 2 {
            return Err(VmError::RuntimeError(
                "list.add expects exactly 2 arguments".to_string(),
            ));
        }
        let elem = args[1].heap_object_for_element(&mut self.heap);
        match self.heap.get_mut(&r) {
            Ok(HeapObject::List { items, .. }) => {
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

    /// `list.remove` / `LIST_REMOVE` (§144, §34): remove a list element by index.
    fn native_list_remove(&mut self, args: &[Value]) -> Result<Value, VmError> {
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
            Ok(HeapObject::List { items, .. }) => {
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

    /// `object.remove` / `OBJECT_REMOVE` (§144, §39): remove a property by key.
    fn native_object_remove(&mut self, args: &[Value]) -> Result<Value, VmError> {
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
            Ok(HeapObject::Object { props, .. }) => {
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
            stack_depth: self.stack().len(),
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
        // Every entry into a finally body pushes exactly one continuation
        // marker (ISSUES §59d); FinallyEnd consumes the top marker on every
        // path before deciding where control goes.
        let marker = self.pop_finally_marker()?;
        if let Some(msg) = self.pending_error.take() {
            // Marker discarded: the error continues outward unchanged.
            return self.raise_runtime_error(msg);
        }
        if let Some(val) = self.pending_return.take() {
            // Marker discarded: return completes as if no marker existed.
            return self.complete_pending_return(val);
        }
        // Marker continuation: pop the crossed handlers the compiler recorded,
        // unwind scopes to the destination depth, jump to the destination.
        for _ in 0..marker.handler_pops {
            if self.handlers.pop().is_none() {
                return Err(VmError::InvalidExecutionState(
                    "finally continuation marker: handler pops exceeded handler stack"
                        .to_string(),
                ));
            }
        }
        if let Some(frame) = self.call_stack.last_mut() {
            let mut remaining = marker.scope_pops;
            while remaining > 0 && frame.scopes.len() > 1 {
                frame.scopes.pop();
                remaining -= 1;
            }
        }
        self.pc = marker.target;
        Ok(())
    }

    /// Marker for a finally entered with `pending_error`/`pending_return`:
    /// the four slots exist so FinallyEnd consumes exactly one marker on every
    /// path; the pending branch discards it without reading the fields.
    fn push_pending_finally_marker(&mut self) {
        self.stack_mut().push(Value::Number(0.0));
        self.stack_mut().push(Value::Number(0.0));
        self.stack_mut().push(Value::Number(0.0));
        self.stack_mut().push(Value::Str(FINALLY_MARKER.to_string()));
    }

    fn pop_finally_marker(&mut self) -> Result<FinallyMarker, VmError> {
        let invalid = |reason: &str| {
            VmError::InvalidExecutionState(format!("finally continuation marker: {}", reason))
        };
        if self.stack().len() < 4 {
            return Err(invalid("stack underflow"));
        }
        let sentinel = self.stack_mut().pop().unwrap();
        let handler_pops = self.stack_mut().pop().unwrap();
        let target = self.stack_mut().pop().unwrap();
        let scope_pops = self.stack_mut().pop().unwrap();
        match sentinel {
            Value::Str(ref s) if s == FINALLY_MARKER => {}
            _ => return Err(invalid("sentinel missing at stack top")),
        }
        let as_count = |v: &Value| match v {
            Value::Number(n)
                if n.is_finite() && *n >= 0.0 && n.fract() == 0.0 && *n <= usize::MAX as f64 =>
            {
                Some(*n as usize)
            }
            _ => None,
        };
        let scope_pops = as_count(&scope_pops).ok_or_else(|| invalid("scope-pops field"))?;
        let target = as_count(&target).ok_or_else(|| invalid("target field"))?;
        let handler_pops = as_count(&handler_pops).ok_or_else(|| invalid("handler-pops field"))?;
        if target >= self.program.instructions.len() {
            return Err(VmError::InvalidJumpTarget(target));
        }
        Ok(FinallyMarker {
            scope_pops,
            target,
            handler_pops,
        })
    }

    /// Build the object bound to `catch e` (ISSUES §59a, §43, §154).
    /// Fields: `type` (error category), `message` (the runtime error text),
    /// `location` (`{file, line, column}` or `null` — `null` here because the
    /// VM carries no source position for runtime errors, §154 "jika informasi
    /// tersebut tersedia").
    fn make_error_object(&mut self, msg: &str) -> Value {
        let type_ref = self
            .heap
            .allocate(HeapObject::Str(classify_runtime_error(msg).to_string()));
        let message_ref = self.heap.allocate(HeapObject::Str(msg.to_string()));
        let location_ref = self.heap.allocate(HeapObject::Null);
        let object_ref = self.heap.allocate(HeapObject::error_object(vec![
            ("type".to_string(), type_ref),
            ("message".to_string(), message_ref),
            ("location".to_string(), location_ref),
        ]));
        Value::Object(object_ref)
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
                        let error_value = self.make_error_object(&msg);
                        self.stack_mut().push(error_value);
                        self.pc = catch_pc;
                        return Ok(());
                    } else if let Some(finally_pc) = handler.finally_pc {
                        self.handlers.pop();
                        self.unwind_to_handler(&handler);
                        self.pending_error = Some(msg);
                        // Marker after truncation: FinallyEnd consumes it, then
                        // re-raises pending_error (ISSUES §59d).
                        self.push_pending_finally_marker();
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
                        self.push_pending_finally_marker();
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
                    self.swap_instructions(instrs);
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
        if self.stack().len() > handler.stack_depth {
            self.stack_mut().truncate(handler.stack_depth);
        }
    }

    fn complete_pending_return(&mut self, return_value: Value) -> Result<(), VmError> {
        // Discard handlers belonging to the frame being returned from
        let frame_depth = self.call_stack.len();
        self.handlers
            .retain(|h| h.frame_depth < frame_depth);

        if self.call_stack.is_empty() {
            self.stack_mut().push(return_value);
            self.state = ExecutionState::Completed;
            return Ok(());
        }

        let frame = self.call_stack.pop().unwrap();
        self.pc = frame.return_pc;

        if let Some(instructions) = frame.return_instructions {
            self.swap_instructions(instructions);
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
            self.stack_mut().push(return_value);
        }
        Ok(())
    }

    fn execute_return(&mut self) -> Result<(), VmError> {
        let return_value = if self.stack().is_empty() {
            Value::Null
        } else {
            self.stack_mut().pop().unwrap()
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
            if self.stack().len() > handler.stack_depth {
                self.stack_mut().truncate(handler.stack_depth);
            }
            self.pending_return = Some(return_value);
            // Marker after truncation: FinallyEnd consumes it, then completes
            // pending_return (ISSUES §59d).
            self.push_pending_finally_marker();
            self.pc = handler.finally_pc.unwrap();
            return Ok(());
        }

        // Discard any remaining handlers for this frame (no finally)
        self.handlers.retain(|h| h.frame_depth < frame_depth);

        if self.call_stack.is_empty() {
            self.stack_mut().push(return_value);
            self.state = ExecutionState::Completed;
            return Ok(());
        }

        let frame = self.call_stack.pop().unwrap();
        self.pc = frame.return_pc;

        if let Some(instructions) = frame.return_instructions {
            self.swap_instructions(instructions);
        }

        if let Some(tid) = frame.task_id {
            // Base frame of a task context: the body finished (§106, §107).
            // The context is discarded, dependents are woken, and any awaiter
            // resolves its await when the scheduler resumes it (§107).
            self.finish_task(tid, &return_value)?;
        } else {
            self.stack_mut().push(return_value);
        }

        Ok(())
    }

    fn execute_make_closure(
        &mut self,
        func_idx: usize,
        captured: &[(Rc<str>, bool)],
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
                    self.heap
                        .set_env_binding(&shared, name.to_string(), heap_val, *is_const)
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
                env_bindings.push(EnvBinding {
                    name: name.to_string(),
                    value: heap_val,
                    is_const: *is_const,
                });
            }
            let new_env = self.heap.allocate(HeapObject::Environment(env_bindings));
            self.gc.on_allocation();
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

        self.stack_mut().push(closure);

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
        if let Some(idx) = self.global_index(name) {
            return Ok(self.global_scope[idx].1.value.clone());
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
        if let Some(idx) = self.global_index(name) {
            return Ok(self.global_scope[idx].1.value.clone());
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
            self.stack_mut().push(value);
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
        if self.stack().len() < size {
            return Err(VmError::StackUnderflow(format!(
                "MakeList needs {} elements, stack has {}",
                size,
                self.stack().len()
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

        self.stack_mut().push(Value::List(list_ref));
        Ok(())
    }

    fn execute_make_object(&mut self, keys: &[Rc<str>]) -> Result<(), VmError> {
        if self.stack().len() < keys.len() {
            return Err(VmError::StackUnderflow(format!(
                "MakeObject needs {} values, stack has {}",
                keys.len(),
                self.stack().len()
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
            props.push((key.to_string(), r));
        }

        let obj_ref = self.heap.allocate(HeapObject::object(props));
        self.gc.on_allocation();

        self.stack_mut().push(Value::Object(obj_ref));
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
                        self.stack_mut().push(value);
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
                self.stack_mut().push(Value::Str(ch.to_string()));
                Ok(())
            }
            (Value::Object(obj_ref), Value::Str(key)) => match self.heap.get(obj_ref) {
                Ok(HeapObject::Object { props, .. }) => {
                    for (k, vr) in props {
                        if k == key {
                            let value = Value::from_heap_ref(vr, &self.heap).map_err(|_| {
                                VmError::RuntimeError("Failed to convert property".to_string())
                            })?;
                            self.stack_mut().push(value);
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
                    Ok(HeapObject::List { items, .. }) => {
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
                self.stack_mut().push(value);
                Ok(())
            }
            (Value::Object(obj_ref), Value::Str(key)) => {
                let heap_val = value.heap_object_for_element(&mut self.heap);
                match self.heap.get_mut(obj_ref) {
                    Ok(HeapObject::Object { props, .. }) => {
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
                self.stack_mut().push(value);
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
                            self.stack_mut().push(value);
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
            // Dynamic CALL_NATIVE path (Bytecode.md §145): BuiltIn + GetProperty
            // yields a method BuiltIn that Call dispatches through call_native.
            Value::BuiltIn(base) => {
                let known = native::native_methods(base);
                if known.contains(&name) {
                    self.stack_mut()
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
                    Ok(HeapObject::Object { props, .. }) => {
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
                self.stack_mut().push(value);
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
        self.stack_mut().pop().ok_or_else(|| {
            VmError::StackUnderflow(format!(
                "{}: stack is empty",
                context
            ))
        })
    }

    fn peek_stack(&self, context: &str) -> Result<Value, VmError> {
        self.stack().last().cloned().ok_or_else(|| {
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






#[cfg(test)]
mod tests;
