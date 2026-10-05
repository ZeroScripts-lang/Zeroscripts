use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::compiler::Opcode;

use crate::heap::HeapRef as HeapRefHandle;

use super::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionState {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub(super) struct Binding {
    pub(super) value: Value,
    pub(super) is_const: bool,
    pub(super) initialized: bool,
    /// Live binding (§66/§74): (source module env, export name).
    /// When set, loads read through the heap Environment so mutations
    /// by the source module are visible; stores are rejected via is_const.
    pub(super) live: Option<(HeapRefHandle, String)>,
}

/// Collect the heap references carried by a single value (§121: Value
/// Stack entries; the Closure variant's env counts as closure environment,
/// §131).
pub(super) fn collect_value_refs(value: &Value, roots: &mut Vec<HeapRefHandle>) {
    match value {
        Value::List(r) | Value::Object(r) => roots.push(r.clone()),
        Value::Closure { env, .. } => roots.push(env.clone()),
        _ => {}
    }
}

/// A binding contributes both its value and its live-binding environment
/// (module bindings, §121).
pub(super) fn collect_binding_refs(binding: &Binding, roots: &mut Vec<HeapRefHandle>) {
    collect_value_refs(&binding.value, roots);
    if let Some((env, _)) = &binding.live {
        roots.push(env.clone());
    }
}

/// Local variables and parameters live in frame scopes; closure
/// environments hang off the frame (§121 "local variables pada Execution
/// Frame yang aktif", "parameter fungsi"; §131 "closure environment").
pub(super) fn collect_frame_refs(frame: &Frame, roots: &mut Vec<HeapRefHandle>) {
    for scope in &frame.scopes {
        for binding in scope.values() {
            collect_binding_refs(binding, roots);
        }
    }
    if let Some(env) = &frame.captured_env {
        roots.push(env.clone());
    }
    if let Some(env) = &frame.scope_env {
        roots.push(env.clone());
    }
}

impl Binding {
    pub(super) fn new(value: Value, is_const: bool, initialized: bool) -> Self {
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
pub(super) enum ExportBinding {
    Value(Value),
    Live(HeapRefHandle, String),
}

#[derive(Debug, Clone)]
pub(super) struct Frame {
    pub(super) function_index: usize,
    pub(super) scopes: Vec<HashMap<String, Binding>>,
    pub(super) return_pc: usize,
    pub(super) captured_env: Option<HeapRefHandle>,
    pub(super) scope_env: Option<HeapRefHandle>,
    pub(super) return_instructions: Option<Vec<Opcode>>,
    /// Set when this frame belongs to an async task's own context (§101).
    pub(super) task_id: Option<usize>,
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

    pub(super) fn is_terminal(&self) -> bool {
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
pub(super) enum TaskKind {
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
pub(super) type HttpSlot = Arc<Mutex<Option<Result<HttpOutcome, String>>>>;

/// A completed HTTP exchange as parsed by the client (§50 Response).
#[derive(Debug, Clone)]
pub(super) struct HttpOutcome {
    pub(super) status: u16,
    pub(super) headers: Vec<(String, String)>,
    pub(super) body: String,
}

/// A suspended task's execution context: everything the linear instruction
/// loop operates on, swapped in/out of `ZeroVM`'s active fields by the
/// cooperative scheduler (§112).
#[derive(Debug, Clone)]
pub(super) struct TaskContext {
    pub(super) pc: usize,
    pub(super) instructions: Vec<Opcode>,
    pub(super) stack: Vec<Value>,
    pub(super) call_stack: Vec<Frame>,
    pub(super) handlers: Vec<TryHandler>,
    pub(super) pending_error: Option<String>,
    pub(super) pending_return: Option<Value>,
}

/// A single asynchronous task (§101). Stored in `ZeroVM.tasks` (outside the
/// heap); heap-backed values (kind args/callee/env, stored result, saved
/// context) are roots via gc_roots (§121) while the task exists.
#[derive(Debug, Clone)]
pub(super) struct Task {
    pub(super) state: TaskState,
    pub(super) kind: TaskKind,
    /// Stored result of a completed task (§107).
    pub(super) result: Option<Value>,
    /// Stored error of a failed task (§108).
    pub(super) error: Option<String>,
    /// Saved execution context while the task is suspended; None for tasks
    /// that have not started yet or have reached a terminal state.
    pub(super) context: Option<TaskContext>,
    /// Id of the task this task is suspended on (`await ...`). Combinators
    /// are themselves tasks, so every suspension is a wait on one task id.
    pub(super) wait: Option<usize>,
    /// §108 "Un-awaited Task Failure": set when `await` targets this task.
    /// A failed task with `awaited == false` (and no awaited combinator
    /// downstream) yields one stderr warning when the program finishes.
    pub(super) awaited: bool,
    /// Id of the source member whose error made this combinator fail
    /// (§103/§104/§109/§110 propagation). Part of the §108 warning-
    /// suppression chain: source members count as awaited when the
    /// combinator that consumed their failure was itself observed.
    pub(super) failed_from: Option<usize>,
    /// Order in which this task reached a terminal state — `task.race`
    /// compares it to find the genuine first finisher (§104).
    pub(super) terminal_seq: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum TryPhase {
    TryBody,
    Catch,
}

#[derive(Debug, Clone)]
pub(super) struct TryHandler {
    pub(super) catch_pc: Option<usize>,
    pub(super) finally_pc: Option<usize>,
    pub(super) scope_depth: usize,
    pub(super) stack_depth: usize,
    pub(super) frame_depth: usize,
    pub(super) phase: TryPhase,
}

/// A continuation marker popped by `FinallyEnd` (ISSUES §59d): four value-
/// stack slots `[scope-pops, target pc, handler-pops, sentinel]`, one pushed
/// on every entry into a finally body (§43 "finally selalu dijalankan").
pub(super) struct FinallyMarker {
    pub(super) scope_pops: usize,
    pub(super) target: usize,
    pub(super) handler_pops: usize,
}
