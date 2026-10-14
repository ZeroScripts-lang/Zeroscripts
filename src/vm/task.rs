use std::thread;
use std::time::Duration;

use crate::heap::HeapRef as HeapRefHandle;

use super::helpers::type_name;
use super::{Frame, Task, TaskContext, TaskKind, TaskState, Value, VmError, ZeroVM};

impl ZeroVM {
    /// Create a pending task for an async function call (§101). The body runs
    /// when the scheduler gives the task control (round-robin ready queue, §112).
    pub(super) fn create_task(
        &mut self,
        func_idx: usize,
        args: Vec<Value>,
        callee: Value,
        env: Option<HeapRefHandle>,
    ) -> Result<(), VmError> {
        // §150 task count: only non-terminal tasks count (ISSUES §71e).
        if let Some(max) = self.limits.task_count {
            let active = self.tasks.len() - self.count_terminal();
            if active + 1 > max {
                return Err(VmError::RuntimeError(format!(
                    "Execution limit exceeded: task count {}",
                    max
                )));
            }
        }
        let id = self.tasks.len();
        // args/callee/env live in the task table — scanned by gc_roots
        // (§121 "task yang masih berjalan").
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
            awaited: false,
            failed_from: None,
            terminal_seq: None,
        });
        self.ready_queue.push_back(id);
        self.stack_mut().push(Value::Task(id));
        Ok(())
    }

    /// Create a synthetic task driven by the scheduler instead of a function
    /// body (§103, §104, §109-§111). Combinator inputs may already be
    /// terminal, so they are evaluated immediately; sleeps only complete when
    /// a timer fires (so `task.sleep(0)` still yields, §111).
    pub(super) fn create_synthetic_task(&mut self, kind: TaskKind) -> Result<usize, VmError> {
        // §150 task count: synthetic (combinator/sleep) tasks are tasks too
        // — same non-terminal policy as `create_task` (ISSUES §71e).
        if let Some(max) = self.limits.task_count {
            let active = self.tasks.len() - self.count_terminal();
            if active + 1 > max {
                return Err(VmError::RuntimeError(format!(
                    "Execution limit exceeded: task count {}",
                    max
                )));
            }
        }
        let id = self.tasks.len();
        let is_sleep = matches!(kind, TaskKind::Sleep { .. });
        self.tasks.push(Task {
            state: TaskState::Pending,
            kind,
            result: None,
            error: None,
            context: None,
            wait: None,
            awaited: false,
            failed_from: None,
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
    pub(super) fn stamp_terminal(&mut self, id: usize) {
        if self.tasks[id].terminal_seq.is_none() {
            let seq = self.next_terminal_seq;
            self.next_terminal_seq += 1;
            self.tasks[id].terminal_seq = Some(seq);
        }
    }

    /// Build the initial context for a pending normal task and mark it
    /// running (§106). The scheduler loads the returned context to execute it.
    pub(super) fn prepare_task_start(&mut self, task_id: usize) -> Result<TaskContext, VmError> {
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
        // Job 022 (§30/§101): defaults are evaluated when the task starts
        // (prepare), before the body runs — with the task's closure env.
        let Some(root_scope) =
            self.build_function_scope(&func_info, &args, &callee, env.clone())?
        else {
            return Err(VmError::InvalidExecutionState(format!(
                "Task {} default parameter evaluation was interrupted",
                task_id
            )));
        };
        let instructions = if func_idx < self.program.function_bodies.len() {
            self.program.function_bodies[func_idx].clone()
        } else {
            Vec::new()
        };
        self.tasks[task_id].state = TaskState::Running;
        Ok(TaskContext {
            pc: 0,
            instructions,
            debug: func_info.debug.clone(),
            source_file: func_info.source_file.clone(),
            call_stack: vec![Frame {
                function_index: func_idx,
                scopes: vec![root_scope],
                stack: Vec::new(),
                return_pc: 0,
                captured_env: env,
                scope_env: None,
                return_instructions: None,
                return_debug: None,
                return_source: None,
                task_id: Some(task_id),
            }],
            handlers: Vec::new(),
            pending_error: None,
            pending_return: None,
        })
    }

    /// Save the active instruction-loop state into `task_id`'s context.
    /// Called when a task suspends on `await` (§112).
    pub(super) fn save_active_context(&mut self, task_id: usize) {
        let ctx = TaskContext {
            pc: self.pc,
            instructions: std::mem::take(&mut self.program.instructions),
            debug: std::mem::take(&mut self.active_debug),
            source_file: std::mem::take(&mut self.active_source),
            call_stack: std::mem::take(&mut self.call_stack),
            handlers: std::mem::take(&mut self.handlers),
            pending_error: self.pending_error.take(),
            pending_return: self.pending_return.take(),
        };
        self.tasks[task_id].context = Some(ctx);
        self.bump_cache_gen();
    }

    /// Load a saved context into the instruction-loop fields.
    pub(super) fn load_context(&mut self, ctx: TaskContext) {
        self.pc = ctx.pc;
        self.swap_instructions(ctx.instructions, ctx.debug, ctx.source_file);
        self.call_stack = ctx.call_stack;
        self.handlers = ctx.handlers;
        self.pending_error = ctx.pending_error;
        self.pending_return = ctx.pending_return;
    }

    /// Record a task reaching `completed` with its result (§106, §107) and
    /// wake everything waiting on it (§103, §104, §109, §110).
    pub(super) fn finish_task(&mut self, task_id: usize, result: &Value) -> Result<(), VmError> {
        if !self.tasks[task_id].state.is_terminal() {
            self.tasks[task_id].state = TaskState::Completed;
            self.tasks[task_id].result = Some(result.clone());
            self.tasks[task_id].context = None;
            self.stamp_terminal(task_id);
            self.notify_dependents(task_id)?;
        }
        Ok(())
    }

    /// Mark a task `failed`, discard its context, and wake dependents (§106,
    /// §108). The error stays stored; it is thrown only where the task is
    /// awaited — an un-awaited failure must not stop the program (§108).
    pub(super) fn fail_task(&mut self, task_id: usize, msg: String) -> Result<(), VmError> {
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
                self.stack_mut().clear();
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
    pub(super) fn cancel_task(&mut self, task_id: usize) -> Result<(), VmError> {
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
    pub(super) fn notify_dependents(&mut self, tid: usize) -> Result<(), VmError> {
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
    pub(super) fn reevaluate_synthetic(&mut self, id: usize) -> Result<(), VmError> {
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
                            // §108: record which member's error entered this combinator.
                            self.tasks[id].failed_from = Some(x);
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
                        // §108: record which member's error entered this combinator.
                        self.tasks[id].failed_from = Some(w);
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
                    // §108: record which member's error entered this combinator.
                    self.tasks[id].failed_from = Some(target);
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
                    // §108: record which member's error entered this combinator.
                    self.tasks[id].failed_from = Some(target);
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
    pub(super) fn wake_due_timers(&mut self) -> Result<bool, VmError> {
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
    pub(super) fn earliest_deadline(&self) -> Option<u128> {
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

    pub(super) fn count_terminal(&self) -> usize {
        self.tasks.iter().filter(|t| t.state.is_terminal()).count()
    }

    /// Pop the next runnable normal task from the FIFO ready queue (§112).
    /// Stale entries (terminal or re-suspended tasks) are skipped.
    pub(super) fn pop_runnable(&mut self) -> Option<usize> {
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
    /// §108 "Un-awaited Task Failure": did this failed task's error reach an
    /// observation point? A task counts as observed when `await` targeted it
    /// directly, or when the failure entered a combinator (`task.all`,
    /// `task.race`, `task.wait`, `task.timeout`) that is itself observed —
    /// the chain follows `failed_from` (member ids are always smaller than
    /// their combinator's, so recursion terminates).
    pub(super) fn failure_was_observed(&self, tid: usize) -> bool {
        if self.tasks[tid].awaited {
            return true;
        }
        for (i, t) in self.tasks.iter().enumerate() {
            if t.failed_from == Some(tid) && self.failure_was_observed(i) {
                return true;
            }
        }
        false
    }

    /// §108 "Un-awaited Task Failure": when the program finishes, push one
    /// stderr warning per failed task that was never observed. Warnings go
    /// through `err_output` (the stderr channel `print.err` uses); wording is
    /// an implementation choice — the spec requires one warning per task but
    /// does not prescribe its text.
    pub(super) fn emit_unawaited_failure_warnings(&mut self) {
        let mut lines = Vec::new();
        for (tid, task) in self.tasks.iter().enumerate() {
            if task.state != TaskState::Failed || self.failure_was_observed(tid) {
                continue;
            }
            let err = task
                .error
                .clone()
                .unwrap_or_else(|| "Task failed".to_string());
            let subject = match &task.kind {
                TaskKind::Normal { func_idx, .. } => self
                    .program
                    .functions
                    .get(*func_idx)
                    .map(|f| format!("task \"{}\"", f.name))
                    .unwrap_or_else(|| "task".to_string()),
                TaskKind::All(_) => "task.all()".to_string(),
                TaskKind::Race(_) => "task.race()".to_string(),
                TaskKind::Wait(_) => "task.wait()".to_string(),
                TaskKind::Sleep { .. } => "task.sleep()".to_string(),
                TaskKind::Timeout { .. } => "task.timeout()".to_string(),
                TaskKind::Http { .. } => "http request".to_string(),
            };
            lines.push(format!(
                "ZeroScript warning: {} failed but was never awaited: {}",
                subject, err
            ));
        }
        self.err_output.extend(lines);
    }

    pub(super) fn run_scheduler(&mut self) -> Result<(), VmError> {
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
                // §150: wall time keeps passing while parked (ISSUES §71b);
                // no context is loaded, so this surfaces uncaught (§71a).
                self.check_idle_time_limit()?;
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
            // Program finished normally → §108 un-awaited failure warnings.
            self.emit_unawaited_failure_warnings();
            return Ok(());
        }
    }

    /// Run one task until it suspends on `await` or reaches a terminal state.
    /// Its context is saved back to the task table on suspension (§112).
    pub(super) fn run_task_step(&mut self, tid: usize) -> Result<(), VmError> {
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
                            self.stack_mut().push(result);
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
            // GC boundary (§121): the active task's context is in the
            // instruction-loop fields; suspended tasks stay in the table —
            // all covered by gc_roots.
            self.maybe_gc();
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
            let instr_pc = self.pc;
            self.pc += 1;
            if let Err(err) = self.execute_instruction(instr_pc, &instruction) {
                if let VmError::RuntimeError(msg) = err {
                    self.note_error_pc(instr_pc);
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
    pub(super) fn execute_await(&mut self) -> Result<(), VmError> {
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

        // §108 "Un-awaited Task Failure": reaching await counts as observation —
        // this task's failure will not produce a warning.
        self.tasks[target].awaited = true;

        match self.tasks[target].state {
            TaskState::Completed => {
                let result = self.tasks[target].result.clone().unwrap_or(Value::Null);
                self.stack_mut().push(result);
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

}
