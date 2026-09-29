# ZeroScript — Open Issues / Not Yet Finalized

57. Planned / Not Yet Finalized

Area yang masih belum dikunci sepenuhnya:

- List functional API seperti "map", "filter", "forEach".
- String API tambahan.
- Object API tambahan.
- HTTP/networking lanjutan.
- Aturan module lebih lanjut.
- Package/dependency system.
- Concurrency di luar "async/await".
- Detail bytecode ZeroVM.
- Memory management / garbage collection.
- Formal grammar.
- Formal error specification.
- Closure/resource limits.
- Explicit conversion lanjutan.
- Immediate invocation syntax untuk anonymous function.

Spesifikasi ini dapat berubah selama pengembangan ZeroScript.

---

## 58. Job 008 Ambiguity Analyses (2026-09-23)

### 58a. §3 vs §4 vs Closure Capture (Function Scope Assignment)

**Readings considered:**

1. **§3-only reading**: name lookup walks current → parent → global; assignment writes to whatever binding the lookup finds. Consequence: `A = 20` inside `function test()` would mutate the global `A`, producing output `20` then `20`. Directly contradicts §4's stated output `20` then `10`.

2. **§4-only reading**: assignment inside a function always creates a function-local, never touching outer bindings; but reads still follow §3's chain. Produces the required output.

3. **Closure-capture reading (Job 007)**: a function that lexically captures `A` shares the same binding via `LoadCaptured`/`StoreCaptured`; assignment mutates the shared env. This is *explicit* capture, distinct from Job 008's *incidental* name match.

**Decision (reading 2):** §4's example with exact expected output is normative for ordinary functions (no explicit capture). §3's chain governs *reads*. Job 008 implements:

- `resolve_for_load` for a bare function: Global/Local chain (no capture).
- `resolve_for_store` for a bare function: if outer binding exists but we are inside a function frame, emit `StoreLocal` with `create_local=true`, then `register_implicit_local` so subsequent loads in the same function use the new local.
- Explicit closures (Job 007) continue to use `Captured`/`ConstCaptured` and share bindings.

**Why not read 1:** contradicts §4's explicit output. **Why not read 3:** §4 says "tidak mengubah A milik outer scope" for the plain `function test()` case; capture is a separate mechanism reserved for functions that close over the name at compile time.

**No ambiguity remains** between §4 and closure capture once the two paths are distinguished (incidental name match vs. compile-time capture detection). The prior "ambiguity" was a failure to separate those two cases, not a genuine spec conflict.

### 58b. One-Way Const Collection Marking (§3, §34, §39)

Spec requires `const` list/object element/property mutation to error, but is silent on:

- `let A = [1]; const C = A;` — does marking `C` freeze `A`'s object?
- Returning a const collection and binding with `let`.

**Current behavior:** `mark_const_if_collection` sets `HeapObject.is_const=true` on the referenced object when *any* const binding stores it. This is one-way and sticky: `let A=[1]; const C=A` freezes A too; subsequent `A[0]=2` errors.

**Ambiguity:** Spec never defines const-flow through aliasing. Two readings:

1. Const is a property of the *binding* only; aliasing via `let` should not freeze the object.
2. Const is a property of the *heap object* once frozen (current behavior).

**Decision (reading 2, provisional):** Simpler to enforce, matches "const object mutation → ERROR" without tracking binding graphs. Recorded here because the spec does not state which reading is correct; if Job 010+ clarifies const aliasing, revisit `mark_const_if_collection`.

### 58c. Identity Equality for Lists/Objects under `===`

§14: `===` compares raw value ignoring type; `==` compares value+type. For lists/objects, shared reference semantics (§35/§40) mean "same value" under `===` is identity (same HeapRef), not structural equality.

**Decision:** `raw_eq` for `Value::List`/`Value::Object` compares HeapRef handles (`x == y`), i.e., identity. Structural deep-equality under `===` is not implemented and is not required by any spec example. Test: `test_job008_shared_reference_via_raw_eq`.

### 58d. `&&` / `||` / `not` vs truthiness

§16 forbids truthy/falsy; only booleans accepted. Lexer maps `&&`→`And`, `||`→`Or` (same tokens as keywords). Parser maps `not`→`UnaryOp::Not` (same as `!`). No ambiguity; verified by tests rejecting `10 and true` and `"hello" || false`.

---

## 59. Job 009 Ambiguity Analyses (2026-09-23)

### 59a. Runtime representation of caught error `e` (§43)

Spec says only: `"e" merupakan local variable yang berisi error` and shows `print.err(e)`. Runtime.md §154 says runtime errors have information usable by catch, which *may* include error type, message, source location, call stack — but does not mandate a structure or field names.

**Readings considered:**

1. **`e` is a plain string** containing the error message (or a Display form of the error).
2. **`e` is an object** with fields such as `message`, `type` — would invent field names not stated in Spec.
3. **`e` is a dedicated Error value type** — Spec has no Error type in §6 Types.

**Decision (reading 1):** `e` is bound as `Value::Str(error_message)`. Grounded in "berisi error" + `print.err(e)` without inventing fields (AGENTS.md §12). `print.err` writes the formatted string to a separate error output stream (StandardLibrary §42). If Spec later defines an Error structure, bind `e` to that without changing catch mechanics.

**Not blocked by Job 008 point 13 limitations** — none of those (default params, closure-in-data, GC roots, const aliasing) affect try/catch, type(), or conversions.

### 59b. Which errors are catchable

§43: catch captures runtime errors; syntax/parse errors not caught (happen before execution). Runtime.md §154: VM Error is distinguished from ZeroScript runtime error.

**Decision:** Only `VmError::RuntimeError(_)` is routed through `raise_runtime_error` (handler stack). `InvalidBytecode`, `InvalidOpcode`, `StackUnderflow` from internal VM misuse, etc., abort execution uncaught. Compile-time errors never reach the VM.

### 59c. Conversion rules beyond shown examples (§54)

Spec shows: `number("123")`, `number("3.14")`, `string(123)`, `string(true)`, `boolean(true)`, errors for `number("hello")`, `boolean(1)`, `boolean("true")`. Silent on: `number(123)` (identity), `string(null)`, `string("x")` (identity), `string([..])`, `boolean(false)` (shown via identity of true only).

**Decisions (documented, minimal invention):**
- `number`: `Number` identity; `String` strict f64 parse; else ERROR.
- `string`: `Str` identity; `Number` (integer form without `.0` when whole); `Boolean`; `Null` → `"null"` (matches Display); `List`/`Object`/`Function` → ERROR (invalid conversion).
- `boolean`: only `Boolean` identity; all else ERROR (no truthy/falsy, §54).

### 59d. `finally` on `break`/`continue` early exit

§43: `"finally" selalu dijalankan`. Job 009 text: also on early exit like `return`. `return`-through-finally is implemented (`pending_return` + `FinallyEnd`). `break`/`continue` that jump out of a try region do **not** currently run finally (compiler does not emit finally hooks on those jumps). Recorded as known limitation / candidate gap in CHECK.md — not silently ignored; may be fixed without changing §43.

---

## 60. Job 010 Ambiguity Analyses (2026-09-24)

### 60a. `object.remove` on missing key (§39)

§39 shows `object.remove(user, "umur")` (successful case) but does not state behavior when the key does not exist. §38 (Object *Access*) explicitly says `Missing key → ERROR`.

**Readings considered:**

1. **ERROR** (analogous to §38 access): removing a non-existent key is an invalid operation on the object, consistent with access semantics.
2. **No-op / return `null`**: removal of something that isn't there is harmless; some languages return a status.
3. **Return boolean** indicating whether something was removed.

**Decision (reading 1):** `object.remove` on a missing key → `RuntimeError` (`"Key '<k>' not found"`). Grounded by §38's explicit missing-key rule applied to the same object namespace; spec is silent on remove-specific behavior, so no field/return shape is invented (AGENTS.md §12). Catchable via try/catch like other runtime errors. If Spec later defines remove-missing as no-op, revisit the `props.len() == before` branch in `call_builtin`.

**No Job 008 point-13 limitation blocked this job** — none of (default params, closure-in-data, GC roots, const aliasing) affect string/list/object builtins or print.

### 60b. Soft CALL_NATIVE vs formal `CALL_NATIVE` opcode (§145-§146)

Job 010 required explaining how builtins connect to the VM. Spec §145 allows "mekanisme internal lain yang setara" to `CALL_NATIVE list.length`. Formal `CALL_NATIVE` opcode is Job 019.

**Decision:** Job 010 uses a **soft native boundary**: namespace objects (`string`, `list`, `object`, `print`) are `Value::BuiltIn(ns)` in root scope; `GetProperty` resolves known method names to `Value::BuiltIn("ns.method")`; `ExecuteCall` dispatches all BuiltIn values through `call_builtin`. No new opcodes. Observable behavior is identical to a future `CALL_NATIVE` migration (Job 019 can swap the dispatch without changing call sites or results).

---

## 61. Job 011 Ambiguity Analyses (2026-09-24)

### 61a. Color nesting resets (§45)

§45 requires `color.bold(color.red("Error"))` to combine, but does not specify ANSI codes or reset strategy.

**Readings considered:**

1. **Full reset (`\x1b[0m`)** after each color: simple, but outer `bold` is cleared when inner `red` closes — nesting order `bold(red(x))` works only if `red` closes before `bold` re-applies (it doesn't; closes are inner-first). Specifically `bold_start + red_start + x + reset + reset` leaves no bold after the first reset.
2. **Selective resets** (`\x1b[39m` default-fg for colors, `\x1b[22m` normal-intensity for bold): inner close does not clear the outer attribute.

**Decision (reading 2):** `red/green/yellow/blue` → `\x1b[3Xm` … `\x1b[39m`; `bold` → `\x1b[1m` … `\x1b[22m`. No external dependency (§45, AGENTS.md no-unnecessary-deps). Spec is silent on exact codes — codes are the standard ANSI SGR set, not a language rule.

### 61b. `file.write` / `file.remove` / `file.mkdir` return value (§48)

§48 shows call forms only; no return values stated.

**Readings considered:**

1. Return `null` (side-effecting ops; matches `list.add`/`object.remove` → null).
2. Return `true`/status boolean.
3. Return path/content echo.

**Decision (reading 1):** mutators return `null`; `file.read` returns string; `file.exists` returns boolean (stated in job/scope). No fields or shapes invented (AGENTS.md §12).

### 61c. `file.mkdir` recursive vs single-level (§48)

Spec shows `file.mkdir("data")` only.

**Readings:** (1) `create_dir` — parent must exist, missing parent → catchable error; (2) `create_dir_all` — creates parents silently.

**Decision (reading 1):** single-level `create_dir`. Simpler, errors surface through try/catch as §48 requires for filesystem errors; recursive creation is not specified.

### 61d. `system.arch()` value format (§52)

Spec example: `"arm64"`. Host `std::env::consts::ARCH` on this machine is `"aarch64"`.

**Readings:** (1) return Rust `ARCH` as-is (`aarch64`/`x86_64`/…); (2) map to marketing names (`aarch64`→`arm64`).

**Decision (reading 1):** return `std::env::consts::ARCH` unmodified. Spec says "Contoh" (example), not a normative enumeration; inventing a mapping table would be an unstated transform (AGENTS.md §12). Tests assert non-empty string type only. Same for `system.os()` → `std::env::consts::OS` (`"linux"` matches example).

### 61e. `system.env` non-UTF-8 values (§52)

Spec: string or `null` if missing. `std::env::var` also fails with `NotUnicode` for values that exist but are not valid UTF-8.

**Decision:** both `NotPresent` and `NotUnicode` → `null`. Spec has no type for "bytes that aren't a string"; returning null preserves the stated string-or-null contract without inventing a binary type.

### 61f. `json.parse` error representation (§51, Job 011 scope note)

Spec does not define the error message for invalid JSON. Job instructs: use existing runtime-error mechanism, catchable via try/catch, report assumptions.

**Decision:** `VmError::RuntimeError(format!("Invalid JSON: {}", detail))` where `detail` is the hand-rolled parser's position/message (e.g. `Unterminated string`, `Unexpected character 'b' at position 1`). Catchable as plain message string per §59a (e is `Value::Str`). Exact wording is mechanical (spec-silent), not spec-derived.

### 61g. `json.stringify` accepted top-level types (§51)

Job text: "A adalah object/list". §51 examples show only objects/lists. `json.parse` can return any JSON value (RFC 8259 allows top-level scalars).

**Readings:**

1. **Strict:** only object/list accepted at top level; `json.stringify(json.parse("42"))` errors.
2. **Closed set of JSON types:** null, boolean, number, string, list, object accepted; functions/closures error (no JSON form).

**Decision (reading 2):** stringify accepts exactly the JSON data types so parse↔stringify roundtrips any valid JSON document; rejects function/built-in/closure with catchable error. Non-finite numbers (NaN/±Infinity) also error — not representable in JSON. Job's "object/list" phrasing describes §51's examples, not an exclusive type restriction; excluding scalars would make roundtrip asymmetric with parse.

### 61h. Duplicate keys on `json.parse` (§51)

JSON (RFC 8259) allows duplicate names; parsers commonly keep last-wins.

**Decision:** last-wins (replace value for existing key, preserve first-seen key order). Spec silent; matches common parser behavior; no language rule invented.

### 61i. Color argument types (§45)

§45 examples pass strings only; no type rule stated (unlike §58 string API "Argumen dengan tipe salah → ERROR").

**Decision:** any single argument accepted; rendered via `format_value` (same as `print`). Arity must be exactly 1. Rationale: color is a formatting wrapper; requiring string would forbid e.g. `color.red(10)` without spec basis. Non-collection types format identically to `print`; collections format via heap contents.

### 61j. `system.args()` host contract (§52)

§52: program file name excluded. `ZeroVM` receives `program_args` via `set_program_args`; default empty.

**Decision:** stripping interpreter/script argv is the **host's** responsibility when populating `program_args` — matches "zerovm app.zs hello world → [hello, world]" where the launcher knows argv layout. `main.rs` does not yet execute `.zs` files, so no CLI wiring in this job (out of scope; noted as known limitation in CHECK.md). Tests inject args explicitly via `set_program_args`.

---

## 62. Job 012 Ambiguity Analyses (2026-09-24)

### 62a. §73 vs §78: Module Evaluation Failure and try/catch

**Readings considered:**

1. **§78-strict (implemented)**: `import` is a static module declaration processed before any module body runs (loader builds graph, topo-sorts, then `run_modules` executes ranges in order). A dependency's runtime error aborts `run_modules` before the importer body starts. Consequence: the error is **not** catchable by try/catch inside the importer (the importer never executes). §73's "error can be caught if import occurs in a runtime-executed context" is unreachable under §78.

2. **§73-runtime reading**: treat import as a runtime operation so try/catch around the import site can catch eval failure. Consequence: contradicts §78's "import is static; position does not determine when dependency is loaded" and "all dependencies must be completed before module code starts."

3. **Hybrid**: run dependencies first; if a dep fails, raise a deferred error into the importer's try handlers when the importer body starts. Not specified by either section; invents a new error-injection rule.

**Decision (reading 1):** §78 is explicit and normative for ordering (`test_job012_import_mid_file_static`). §73's catch clause is only consistent with §78 if "runtime operation" means host-level `run_modules` failure (returned as `Err`), not in-language try/catch. Failed modules are not cached (scope popped, no export snapshot) so a subsequent `load` would retry — but within one program run, `run_modules` stops. Job file G asked to report rather than guess: **this is the reported contradiction**. No in-language catch of static import failure is implemented.

### 62b. Export value timing (Job 012 snapshot vs §66 live binding)

§66/§74 require live binding (importer sees later mutations of `export let`). Job 013 owns live binding; Job 012 snapshots export values into a cache after each module's `EndModule`.

**Decision:** snapshot is an intentional Job 012 limitation (CHECK.md "Live binding → Not Implemented / Job 013"). Functions still work for mutation tests because the closure captures module state; only direct `import { count }` reads of a mutated `export let` would be stale until Job 013.

### 62c. Import bindings declared const

Job 012 declares import names with `is_const: true` at compile time. §74 forbids assignment to imported bindings; §69 alias/conflict detail is Job 013. Const declaration implements the assignment ban early without inventing a separate "imported" binding kind.

**Decision:** imports are const locals. Assignment to an import errors as `Cannot assign to const` (same mechanism as ordinary const).

### 62d. Package/bare import paths

Job 012 Scope: `import` without `./` or `../` is Job 016-017.

**Decision:** bare paths return `ModuleError::NotFound` at load time (no silent resolution against importer dir). Absolute `/...` paths are accepted by `resolve_path` (spec §64 allows runtime support) but are not tested as primary form.

---

## 63. Job 014 Ambiguity Analyses (2026-09-25)

### 63a. Top-level await: rules vs examples (§49/§101 rules vs §49/§112 examples)

**Readings considered:**

1. **Strict rules (implemented)**: §49 states "await digunakan di dalam async function" and §101 states "'await' hanya dapat digunakan di dalam 'async function'". Job 014 Scope A mandates exactly this as a compile-time ERROR. Under this reading, top-level `await` is rejected at compile time.

2. **Examples reading**: §49's own invocation example (`let data = await ambilData()` shown outside any function body) and §112's scheduler example (`await task.all([taskA, taskB])` at top level, after top-level `let taskA = A()`) both require the script itself to run in an implicit async context where top-level await is legal.

**Decision (reading 1):** the rules are stated normatively; the examples show invocation snippets whose surrounding context is unspecified. Top-level await → compile error `'await' is only allowed inside an async function`. Consequence: §112's example cannot run as written. Job 015 (owner of `task.all`/scheduler) must resolve how that example executes (implicit top-level async context, mandatory `async` main wrapper, or a corrected example). Reported per Job 014 Scope "spec silent/contradictory → stop and report", not guessed.

### 63b. `await` on a non-task value (spec silent)

§107 defines await semantics only for tasks (completed → stored result, running → wait, failed → runtime error, cancelled → cancellation error). Behavior of `await 5` is unstated.

**Readings considered:**

1. **Runtime ERROR at the await point (implemented)** — message `await expects a task`.
2. **Passthrough** — return the value unchanged; makes await a no-op on non-tasks and silently accepts mistakes.

**Decision (reading 1):** consistent with §106's requirement that `task.state(A)` with a non-task argument be an ERROR, and with the general rule that unspecified behavior becomes an explicit error rather than invented semantics. The error is a normal runtime error and is catchable by try/catch (`test_job014_await_non_task_is_error`).

### 63c. Mechanical details the spec does not state

- `type(task)` → `"task"`: §53's type-introspection table has no task entry; `"task"` chosen for symmetry with the value kind. (Reported, not spec-derived.)
- Display of a task value → `<task N>` (N = task id). No spec-mandated rendering.
- `task.state` argument errors and error message wording (e.g. `task.state() expects a task`) are implementation wording, not spec text.

### 63d. `await` of a "running" task: wait vs error (§107 vs deadlock)

§107: "await terhadap task yang masih berjalan akan menunggu sampai task mencapai terminal state."

Under Job 014's lazy-start single-task model, `execute_await` only observes state `running` when the awaited task is already running *because of the awaiting context itself* (self-await or an await cycle) — the normal flow transitions pending→running inside `execute_await` itself. Literal "wait" in the cycle case means deadlock with no progress.

**Decision:** runtime error `Task is already running` when awaiting a running task (spec silent on cycles). **Job 015 note:** once the scheduler starts tasks independently (`task.sleep`, `task.all`), a *different* task awaiting a running task must wait per §107 — the `Running` branch must be revisited then, or §113/§107 behavior breaks.

### 63e. Scheduler placement under Job 014's single-task scope (§112 vs job scope)

§112 gives the scheduler responsibility for running `pending`/`running` tasks; Job 014 Scope C is single-task lifecycle (create → await) and explicitly leaves concurrency to Job 015.

**Decision:** async calls create tasks in `pending` (lazy start); a task body runs only when awaited (await-start) or when `drain_pending_tasks()` drains the FIFO queue at the end of `run()`/`run_modules()`. This satisfies §108 (an un-awaited task that fails stores its error without stopping the program or other tasks — `test_job014_unawaited_failure_does_not_stop_program`) and lets every program end with all tasks in a terminal state, without implementing interleaved/concurrent execution. Interleaving, `task.all`/`race`, and independent task starts remain Job 015.

### 63f. Unreachable `cancelled` state (§105 vs Job 014 scope)

§105 (`task.cancel`) and §106's `cancelled` state are not in Job 014's scope; without `task.cancel` no code path can produce a cancelled task. `TaskState::Cancelled` exists in the type but is unreachable in Job 014 tests. Cancellation tests deferred to Job 015 (job file permits deferring, must be reported).

## 64. Job 015 Ambiguity Analyses (2026-09-25)

### 64a. §112 example still requires a wrapper (follow-up to §63a)

§63a (reading 1, implemented) makes top-level `await` a compile error because §49/§101 state the rule normatively. §112's scheduler example writes `await task.all([taskA, taskB])` at top level after top-level `let taskA = A()`.

Job 015 owned the resolution (per §63a) and keeps the compile error: the example executes in tests only when wrapped in `async function main() { ... }` plus a top-level `main()` call (`test_job015_sleep_ordering_not_creation_order` runs exactly §112's logic). Readings considered: (1) implicit top-level async context — rejected, it would contradict the §49/§101 rules; (2) mandatory `async` main wrapper (implemented); (3) editing the spec example — not permitted without a spec job. The literal example text remains non-runnable as written; this is left as a spec-level documentation ambiguity, not silently changed.

### 64b. Self-cancellation (§105 silent)

§105 defines `task.cancel(A)` for tasks that are still running but says nothing about cancelling the task whose body is executing the call (constructible when the task handle is stored in a global the body can read, e.g. `let t = selfy()` with `selfy`'s body calling `task.cancel(t)`).

**Decision:** runtime ERROR `Cannot cancel the currently running task` — a single-threaded cooperative scheduler cannot unwind its own running frame, and inventing silent re-scheduling semantics is not spec-supported. The error is a normal catchable runtime error (`test_job015_self_cancel_is_error`). Note: the spec provides no way to obtain the *current* task's handle (`task.current()` does not exist), so self-cancel is only reachable via an externally stored handle.

### 64c. Await cycles / deadlock (§107, §112 silent)

§107 says awaiting a running task waits for the terminal state; §112 says a suspended task must not block runnable ones. Neither covers a cycle (task A awaits B while B awaits A): waiting per §107 means neither ever reaches a terminal state, and there is no timer to advance.

**Decision:** when the scheduler has no runnable task and no timer, but non-terminal tasks remain, it raises runtime error `Task deadlock: remaining tasks are waiting on each other` instead of hanging (`test_job015_await_cycle_deadlock_error`). Message wording is implementation text, not spec-derived. §107's literal "wait" for a running task is honored for all non-cyclic cases; the Running branch added in Job 015 awaits the target's terminal state (`test_job014` semantics unchanged).

### 64d. Mechanical details the spec does not state

- **Virtual clock.** `task.sleep`/`task.timeout` are measured on a monotonic *virtual* clock advanced by the scheduler to the earliest deadline only when nothing is runnable; the spec exposes no wall-clock API, so no observable behavior derives from real time. Tests are therefore instant and deterministic; real-time pacing is not implemented (known limitation, reported).
- **Timeout/empty-race/duration/deadlock error wording** (e.g. `task.timeout(): task did not complete before the timeout`, `task.race() requires at least one task`, `... must be a non-negative integer number of milliseconds`) is implementation wording; the spec only says ERROR.
- **`task.sleep` without `await`.** §111 says sleep "harus digunakan dengan await", but the spec defines no mechanism for detecting an un-awaited sleep. An un-awaited `task.sleep(ms)` creates a background timer task that completes silently; it is not rejected (reported as a limitation rather than an invented compile/runtime rule).
- **`task.all`/`task.race`/`task.wait`/`task.timeout` return values** are task values awaited by the surrounding `await` (the combinator is itself a synthetic task); this composition is an implementation choice — the spec describes only the awaited result of the combined call.

## 65. Job 016 Ambiguity Analyses (2026-09-25)

### 65a. http.* as Task: worker thread + scheduler poll (§50 + §101 + §112)

§50 says every `http.*` call yields a Task used with `await`, while §112 requires that a task waiting on an async operation must not block other runnable tasks. A blocking HTTP call executed inside `run_task_step` would freeze the whole scheduler for the duration of the request.

**Decision:** each call spawns a `std::thread` performing the blocking exchange (ureq); the task is created `running` (the request is already in flight — §106 wording) with a shared `Arc<Mutex<Option<Result<...>>>>` slot. The worker writes the slot exactly once; the scheduler polls slots on the VM thread and finishes/fails the task there, so every task-state transition stays single-threaded. The awaiter resumes through the normal `notify_dependents` path. Not spec-derived: no section states *how* the network work is scheduled (§112 explicitly leaves the mechanism to the runtime).

### 65b. Virtual clock vs real network time (§110-§112 vs HTTP)

Job 015 measures `task.sleep`/`task.timeout` on a virtual monotonic clock that jumps to the next deadline when idle (deterministic tests). HTTP exchanges complete on the *real* clock.

**Decision (hybrid):** when only virtual timers remain, the clock jumps as before; when no virtual timer remains but HTTP work is on the wire, the scheduler parks in real time (`park_timeout(1ms)` polls) **without** advancing `now_ms`. HTTP `options.timeout` is enforced inside the worker thread in real time (ureq `timeout_global`) — it never participates in virtual jumps, so a `timeout: 5000` request against a fast server cannot be "virtually" preempted. Mixing a virtual `task.sleep` with an in-flight HTTP deadline in the same program can interleave the two clocks in an order that has no real-time meaning; no spec section coordinates them (reported, untested combination).

### 65c. `res.headers` read-only enforcement (§60)

§60 guarantees that changing `res.headers` does not alter the response or any request. Job 016's scope note offers "independent copy" vs "heap object that is simply never written back".

**Decision:** `res.headers` is a freshly allocated independent heap object built from the parsed response at completion time. Mutation is *allowed* in the language (the object is not const/frozen) but trivially cannot affect anything network-related: nothing ever reads it back. This matches the explanatory sentence of §60 ("Mengubah res.headers tidak mengubah response atau request yang sudah terjadi") over a freeze reading. Whether the object should be const was not stated by the spec and was not guessed.

### 65d. Mechanical details the spec does not state

- **HTTP client dependency:** `ureq` 3.4.2 (blocking, pure-Rust TLS via rustls) — chosen because §50's own examples use `https://` URLs (a hand-rolled client would need TLS) and because it has no async runtime, matching the single-threaded VM. Report item 2.
- **Error wording**: `http.get(): ...` prefixes, `query must be an object`, `header values must be strings`, `body must be a string`, `options must be an object`, `request timed out after N ms` — the spec only says ERROR.
- **`options` itself non-object** → ERROR (only field-level rules exist in §59/§60); **`body` non-string** → ERROR (§50.1 only ever shows a string body from `json.stringify`); **unknown option keys** → ignored (no rule); **url non-string** → ERROR; **`timeout: 0`** → accepted (0 ms, fails unless instantaneous).
- **Client-added headers**: ureq adds `user-agent` (and friends per its defaults); §60 only governs *user-provided* headers ("Header dikirim sesuai key/value yang diberikan" — asserted for given headers; client defaults are not spec-controlled either way).
- **Redirects**: followed (ureq default, max 10, error beyond) — spec silent on redirects.
- **Body limits**: response bodies read as lossy UTF-8 up to ureq's 10 MB default (`read_to_string`); larger bodies → runtime error. Lossy decoding means invalid UTF-8 bytes become U+FFFD — `res.body` is always a string per §50.
- **Un-awaited `http.get(...)`**: creates a background request task (same shape as un-awaited sleep, ISSUES §64d); not rejected.
- **Query order**: `options.query` key order follows the object literal order (props are a `Vec`), so `?page=2&active=true` from §59's example is reproduced exactly; §59 does not mandate ordering for programmatically built objects.
- **Query keys/values**: encoded with RFC 3986 unreserved set kept (`A-Za-z0-9-._~`); `null` → `key=` (empty value with `=`); `#fragment` URLs keep the fragment last.

## 66. Job 017 Ambiguity Analyses (2026-09-26)

### 66a. ZSON grammar is never defined (§82, §84, §96)

§82 calls `project.zson` "format ZSON (ZeroScript Object Notation)" and shows only one object; §84 and §96 show two more. No section defines ZSON's grammar, whether it allows comments, trailing commas, unquoted keys, or how it differs from JSON. Job 011's `json.*` parser (Job 010/011) implements strict JSON only, and AGENTS.md forbids inventing syntax beyond what the spec illustrates.

**Decision:** ZSON = the JSON value grammar **plus bare identifier keys**, reusing `json.rs` (`parse_zson` sets a `zson` flag that makes `parse_key` try an identifier before a quoted string). Bare keys match the spec's three examples (`name`, `version`, `dependencies`, `packages`, `scripts`); a hyphen cannot be a bare key (all spec examples that contain `-` — `"http-utils"`, `"2.0.1"` — are quoted), so keys stay `[A-Za-z_][A-Za-z0-9_]*` and `http-utils:` is rejected (`test_job017_manifest_unsafe_dependency_name_rejected`, `zson_bare_key_with_hyphen_is_rejected`). No comments, no trailing commas (`zson_no_comments_or_trailing_commas`) — neither appears in any spec example, and adding them would be invented syntax. Values are exactly JSON's. Readings rejected: (2) ZSON as a full superset of ZeroScript expression syntax, (3) ZSON as strict JSON with quoted keys only (would make all three spec examples invalid).

### 66b. Version selector notation (§83)

§83 documents exactly four forms — `1.2.0`, `1.2.x`, `1.x`, `x` — states that `^`, `~`, `>=` are *not* used, and says `x` is only a selector, never part of a version.

**Decision:** `Selector::parse` accepts only those four forms; anything else (bare `1`, `1.2`, `1.x.x`, uppercase `X`, `*`, `1.2.3-beta`) is rejected at manifest-parse time (`test_job017_manifest_invalid_selector_rejected_at_parse`). Leading zeros (`01.x`) are rejected as invalid numeric components. A selector is always combined with every other requirement seen for the same package, and the highest version satisfying **all** of them is chosen (`highest_matching`) — §83 "Selector selalu memilih versi terbaru yang memenuhi pola selector". Pre-release/build metadata is not in §83 and is not supported (SemVer parsing accepts `MAJOR.MINOR.PATCH` only).

### 66c. Package layout: §81's example tree contradicts §82/§85/§87/§94/§96

§81 shows `my-app/ deps/http-utils/{package.zs, src/client.zs}` with `zeroproject.zs`, and its import `from "http-utils/client"` would then mean `src/client.zs`. Every later section instead shows `my-app/{project.zson, .zsc_m/http-utils/}` (§82, §85, §87) and a package tree of `package.zson` + `client.zs` + `parser.zs` at the package root (§94, §96), with no `src/` and no `deps/`.

**Decision:** §81's tree is a superseded draft (its `zeroproject.zs`, `deps/`, `package.zs` names are contradicted by §82/§94/§96). Installed layout is `.zsc_m/<name>/` containing `package.zson` plus flat `.zs` files, and a subpath maps **literally** from the package root: `http-utils/client` → `.zsc_m/http-utils/client.zs`, `pkg/util/text` → `.zsc_m/pkg/util/text.zs`. A `src/` fallback was considered and rejected — it would contradict §94/§96's example trees and would make resolution depend on which of two files exists.

### 66d. Bare package import (`from "pkg"`) has no specified target

§81 defines "path tanpa `./` atau `../` → package import" but every package import example uses a subpath (`"http-utils/client"`); no section states what `from "pkg"` resolves to. §81's only root-level file names are the superseded `package.zs`.

**Decision:** `pkg` → `.zsc_m/pkg/index.zs`. Spec-silent, therefore implementation convention, not a spec-derived rule. The alternative (`package.zs`) follows a section whose file naming §94/§96 replace. Both forms are tested (`test_job017_package_bare_name_uses_index`).

### 66e. Flat `.zsc_m/` vs §88's tree drawing (and multi-version conflict)

§88 draws `my-app → http-utils → url-parser` as a tree and ends by saying that when `package-a → shared-utils@1.2.x` and `package-b → shared-utils@2.x` cannot share a version, "resolver harus mempertahankan versi yang diperlukan oleh masing-masing dependency" — i.e. two versions of one package must be reachable at the same time, which a single flat copy cannot express. §82/§85/§87's directory examples are flat (`.zsc_m/http-utils/`, `.zsc_m/database/`), and §90 defines conflict resolution as a separate section (Job 018).

**Decision:** the §88 tree is the *dependency graph*, not directory nesting; storage is flat, one copy per package name, shared by everyone whose selectors accept it (§88 "menghindari instalasi duplikat ketika versi yang sama dapat digunakan bersama"). Every requirement for a package is intersected; if the intersection is empty → ERROR (`test_job017_unresolvable_version_conflict_is_error`), which satisfies §88's "jika tidak terdapat versi yang memenuhi seluruh kebutuhan → ERROR". Carrying two conflicting versions at once (the last §88 example) requires per-requirement isolation/nested `.zsc_m` and is deferred to Job 018 (§90) — reported as a known limitation, not silently simplified. *(Updated 2026-09-29: Job 018 §90 implemented the isolation — see §67a. `test_job017_unresolvable_version_conflict_is_error` was superseded by `test_job018_disjoint_selectors_resolve_to_two_versions`; the ERROR path is retained only for selectors with no matching version at all.)*

### 66f. §89 defines no dev/runtime categories

§89's rules are only: the project sees its own `dependencies`; a package sees its own `package.zson` dependencies; a transitive package is not importable by the project unless declared directly; scope does not affect module scope inside the package. There is no `devDependencies`, no `optional`, no peer concept anywhere in §81-§89.

**Decision:** implemented exactly those two visible sets — `Loader::resolve_package` resolves the *owner* of the importing file (project if outside `.zsc_m/`, otherwise the package name from the path) and checks membership of the requested package in that owner's declared dependencies (`test_job017_project_cannot_import_transitive_dependency`, `test_job017_package_imports_its_own_dependency`, `test_job017_package_cannot_import_undeclared_package`, `test_job017_installed_transitive_package_is_not_global`). No scope category was invented.

### 66g. What the lockfile must contain (§84 vs §88)

§84's example lockfile lists only the project's two direct dependencies; §88 requires "dependency tree harus dapat direproduksi menggunakan project.lock.zson"; §87 says the resolver uses the locked versions.

**Decision:** the lockfile records **every** resolved package (direct and transitive) in resolution order, still in §84's exact shape `{ packages: { "n": "v" } }`. That satisfies §88 reproducibility and is backward-compatible with §84's example (which is exactly what a project with no transitive deps produces). Keys are emitted in breadth-first resolution order (manifest declaration order, then each package's own declaration order) — §87 requires deterministic resolution but never states a traversal; breadth-first was chosen for stability (`test_job017_nested_dependency_tree` pins the order).

### 66h. Invalid lockfile handling (§84 silent)

§84 only says "Jika lockfile tersedia **dan masih valid**, package manager menggunakan versi yang tercatat" and "Jika lockfile dihapus, resolver dapat memilih versi terbaru".

**Decision:** a lockfile that fails to parse (or whose `packages` member is missing/malformed) is treated as **absent**: `install` re-resolves and rewrites it (`test_job017_invalid_lockfile_is_ignored_not_fatal`); `Project::open` stores `lock: None`. A *structurally valid* lockfile is consulted entry-by-entry: an entry whose selector no longer matches the project's selector is re-resolved and the file rewritten (`test_job017_stale_lockfile_entry_is_re_resolved`), while a matching entry pins the version (`test_job017_install_honours_lockfile`). Reading a corrupt lock as a hard error was rejected — §84 frames the lockfile as an optimization over `project.zson`, which remains authoritative.

### 66i. §87 verification when installed metadata is missing or corrupt

§87 requires the installed package to match `project.lock.zson`; §96 requires `package.zson` to be valid ZSON. If the file is absent or unparseable, §87's check cannot be performed at all.

**Decision:** while loading, if `.zsc_m/<pkg>/package.zson` exists it is always parsed (corrupt → ERROR, `test_job017_broken_installed_metadata_is_error`); if a lock entry exists but the file is missing → ERROR `Installed version of 'pkg' is unknown (package.zson missing) but project.lock.zson locks X` (`test_job017_missing_installed_metadata_with_lock_is_error`) — silently skipping would let a wrong version run unnoticed. With **no** lock entry there is nothing to compare against, so a package without metadata still loads (`test_job017_package_without_metadata_runs_when_not_locked`); §96's "package.zson harus ada" is a publishing/registry rule (§94), enforced there by Job 018. The package's *name* field is not compared against the directory name — no rule states that it must match, and the lock is keyed by directory name.

### 66j. Package source for `zpm install` (§85 "download package")

§85's flow says "download package"; §95's registry is a Git-backed index of Git URLs; §93's package source is Git. Job 017 explicitly allows a local, deterministic source.

**Decision:** local filesystem registry laid out exactly like §95's example tree, `<registry>/<package>/<version>/` each holding a valid package (`Registry::versions` / `package_dir`); "download" is a directory copy into `.zsc_m/<name>/`. Source lookup order — `--registry <dir>` flag, then `$ZPM_REGISTRY`, then `<project>/registry` — is **mechanical**: no section states where a local source lives. A package absent from the registry → ERROR (`Package 'x' not found in registry ...`), no matching version → ERROR (`No version of 'x' matches selector '...'`), per §85. Not-installed-and-same-version is not re-copied (`test_job017_installed_package_not_downloaded_again`).

### 66k. `zpm run` argument forwarding mechanism (§86)

§86 says "`zpm` tidak mengubah isi command yang ditulis programmer", that the command runs from the project root, and that "Command tambahan diteruskan kepada command script" for `zpm run start -- --debug`.

**Decision:** the command text is executed verbatim (`sh -c '<command>'`, `current_dir` = project root) and extra arguments are appended as **shell-quoted words** after it — the only reading under which they reach the script's argv (`printf '%s\n' > out.txt --debug 'x y'`, since redirections are position-independent, `test_job017_run_script_args_after_redirection_still_reach_command`). Positional parameters are *not* provided, so `$@` inside a script command expands to nothing (a command written to consume `$@` would double its arguments otherwise — that combination was tried and failed). Consequences: a command ending in `;` or `&&` would receive the arguments as a separate command; a command already ending in a pipe gets them as the last command's argv. Neither case appears in §86 and neither is rejected. Exit code of the child is `zpm`'s exit code (`test_job017_run_script_nonzero_exit_code_propagates`).

### 66l. Mechanical details the spec does not state

- **ZSON parse errors / file names in messages.** `project.zson: Expected ',' or '}' at position N, found 'x'` — §82 only says the file must be ZSON.
- **Error wording throughout**: `Package 'x' is not a dependency of the project|package 'y'`, `Package 'x' is not installed — run \`zpm install\``, `Script not found: name`, `Installed version of ... but project.lock.zson locks ... — run \`zpm install\``, `Package cannot import files outside its boundary: ...`, `No version of 'x' matches selector '...'`, `Circular package dependency detected: a -> b`. The spec only requires ERROR; all messages are prefixed `ERROR: `.
- **Exit codes.** `zpm` exits 1 on any error, 0 on success, and passes the script child's code through for `zpm run`; `zsc` exits 1 on any load/run error. No section defines exit codes.
- **`zsc` / `zpm` binary names.** §86's examples invoke `zsc` and `zerovm`, so the crate's default binary was split into `zsc` (src/main.rs) and `zpm` (src/bin/zpm.rs) with `autobins = false` (Cargo.toml). `zerovm` is not a separate binary — running a `.zs` file *is* the VM (`zsc`); no section defines binary names beyond §86's usage examples.
- **Project discovery.** §87 says "Package resolution tidak bergantung pada current working directory". `zsc` discovers `project.zson` by walking up from the *entry file*; `zpm` walks up from its *cwd* (it has no entry file). Both are implementation choices; §87 constrains resolution, not the CLI's starting point.
- **`project.zson` `version` is not SemVer-validated** (§82 requires only "string"), while `package.zson` `version` must be valid SemVer (§96 states it explicitly).
- **`scripts` keys/values validation** (§86: object, string keys, string command) is done at manifest parse time; unknown manifest fields are ignored (§96 allows future fields for packages; §82 is silent for projects).

## 67. Job 018 Ambiguity Analyses (2026-09-29)

### 67a. §90 never says how conflict groups are formed

§90's rules: every dependency gets "versi yang paling baru yang sesuai"; a single version is shared when possible; when a version cannot satisfy everything, the resolver must keep each required version "di area instalasi yang terisolasi"; and each *scope* must get one consistent version. No section defines what a "scope" is, how grouping is computed, or what the isolated area looks like on disk.

**Decision:** a group is identified by the set of requirement origins — for selector requirements that is the **importing scope** (project, or the package declaring it); an explicit `git:` requirement forms its own group and is never merged with selectors (a `git:` demand is exact, §93/§99). All selector requirements with an identical origin set share one version (highest matching every selector **and** a still-valid lock entry — no auto-upgrade, §87/§92); disjoint origin sets resolve independently (`place`, src/package.rs:1789). The isolated area is `.zsc_m/<name>/versions/<v>/` **only when a name keeps more than one version**; a single version keeps §82/§85's flat `.zsc_m/<name>/` layout (both readings fit §82/§85's flat example and §90's isolation rule; a permanent `versions/` nesting was rejected because it would contradict §82/§85's literal directory trees for the common one-version case). Lock keys become `name@version` for multi-version entries (§84's shape is a single-version shorthand — plain `"n": "v"` still emitted when only one version exists, so every Job 017 lockfile test keeps its exact bytes). Scope graph cycles are detected *after* convergence (`check_scope_cycles`) — §90 is silent on cycle detection and §88's cycle ERROR must still hold. Evidence: `job018_one_version_shared_by_several_needs`, `job018_disjoint_selectors_resolve_to_two_versions`.

### 67b. §91 removal: what "tidak digunakan lagi" can be determined from, and how the manifest is edited

§91 says `zpm remove <pkg>` stops declaring the dependency, removes the package only if no other dependency still needs it, and rewrites `project.lock.zson`. It does not say what happens when the package is not declared, nor how `project.zson` is edited textually.

**Decision:** removal is *undeclare + re-resolve + prune* (`remove_dependency`, src/package.rs:2483): the declaration is deleted, resolution is recomputed from the remaining manifest (so "still used by someone else" falls out of the resolution itself rather than a bespoke graph query — `prune`:2375 removes only packages absent from the new resolution), and the lockfile is rewritten from the new resolution. Removing an undeclared package → ERROR (`undeclare_dependency`:2427) — §91's flow presupposes a declared dependency; erroring is the conservative reading. The manifest is edited by parsing + `json::render_zson` + rewrite: comments and original formatting are **not** preserved (ZSON, §66a, defines no comments; a line-preserving editor was rejected as over-engineering for a format with no formatting rules).

### 67c. §92 update: what gets updated and with what arguments

§92: `zpm update` brings direct **and transitive** dependencies to the newest version matching their selector, without touching project code; `zpm install` must not silently upgrade a locked version. The spec shows no arguments.

**Decision:** `zpm update` updates *all* packages (§92 names no subset, and "jika versi baru sudah sesuai selector" is universal); an extra argument is rejected as `ERROR: usage: zpm update` (mechanical — no partial-update syntax exists in the spec, and inventing `update <pkg>` would be a new syntax). Implementation = the normal resolution pipeline with the lockfile pinning disabled (`InstallPool::for_update`), so transitive packages move too (§92 "bahkan dependency transitive"). A selector that no longer has any matching version → ERROR (§83's no-match rule applies during update as well). `zpm install` never bumps a locked version (`job018_update_bumps_locked_version_install_does_not`).

### 67d. §93/§97 Git packages: double clone, probe, and lock identity

§93 requires Git packages to be installed from a Git source and §97's lock example records `source`, `commit`, `hash`. §92/§87 require a locked commit to be honoured on reinstall. Nothing says how the commit of an unconstrained (`#main`) dependency is learned before installation, or how identity is obtained for a Git package that is already locked.

**Decision:** resolution needs the package *version* before content exists (the version selects which requirement applies), so `probe_git` (src/package.rs:2645) does a shallow clone into a staging dir, reads `package.zson`, and discards it; `acquire` then clones again for real (a shared clone-cache would avoid the second clone but §93 defines no cache for probes — the double clone is the simple architecture). A locked commit short-circuits: `git::checkout_commit` (src/git.rs:153) pins the exact commit first, so reinstall is reproducible without re-probing the ref. Checkout uses `--end-of-options` before the ref (src/git.rs:95) — both option-injection safety and refs starting with `-`. Git packages record `source` + `commit` + a computed `hash` (§97 example shape); no test relies on the clone itself being deduplicated.

### 67e. §94 publish: registry, Git tag, and "distribution channel"

§94 requires the version to be uploaded to the registry and to appear in Git's release history, forbids overwriting an existing version, and requires `package.zson`. It does not say where the registry comes from when `--registry` is absent, nor what the Git release marker is.

**Decision:** the registry is **explicit** — `--registry` flags, then `$ZPM_REGISTRY`, then ERROR `Publish` (mechanical: §94 presupposes a known registry and Job 017 established these two configuration sources; silently defaulting to `<project>/registry` would publish into the consuming project's own source tree, which §94's read-side model does not describe). The Git marker is the annotated-style lightweight tag `v<version>` (`git::create_tag`) — `v`-prefix is the conventional reading of "muncul di riwayat rilis Git" and no tag format is specified; a tag already present → ERROR (release history is final, mirroring §94's version-finality rule). Ordering: registry copy **then** tag, so a repo already tagged at that version errors on first publish (the tag is treated as evidence of a prior release — spec-silent, conservative direction). Copy destination is a **local directory registry** (§95's minimum local implementation — see §66j/§67f); §94 does not mandate an HTTP transport and no test performs network I/O. Second publish of the same version → ERROR `already published` (`job018_publish_git_repo_marks_release_tag_and_is_final`).

### 67f. §95 index format, and where candidates come from

§95's index example maps `version → source` (Git URL) and shows an optional integrity hash, but never defines: whether `source` may be a plain path, what the hash syntax is, what happens on a malformed index, or whether the *cache* can satisfy resolution when a registry is unreachable.

**Decision:** `index.zson` is `{ name?, versions: { "<version>": "<source>" | { source, hash? } } }`; a non-`git:`/non-URL source is resolved **relative to `<registry>/<name>/`** (mechanical — §95's example stores sources inside the registry tree); `hash` must be `sha256:<64 hex>` (§97's hash shape) or the index is rejected. A malformed index (bad JSON, missing `versions`, bad version key, bad hash) → ERROR with the file path in the message — §95 gives no partial-read rule and silently skipping a broken index would silently change resolution. **Candidates come only from registry listings** (§95 "Registry menyediakan daftar kandidat"): the cache stores *content* (§98), never candidate versions, so a deleted registry breaks resolution even when every package is cached — tested deliberately (`job018_valid_cache_is_used_when_registry_copy_is_gone` keeps the registry present for resolution and only guts the package copy; `job018_corrupt_cache_is_not_used` verifies a digest-mismatched cache entry is re-fetched, not used). Multiple registries are consulted in configuration order (flags → `$ZPM_REGISTRY` → `<project>/registry`); "official registry" (§95) has no specified identification mechanism and is not named specially.

### 67g. §96/§94 metadata consumption: which name/version must match what

§96 requires `package.zson` to exist with valid `name` + SemVer `version` and allows future fields; §94 requires publishing to validate it. Job 017's loader check (§66i) deliberately did *not* compare the stored `name` against the directory name because no rule stated it.

**Decision:** with §94/§95 flows in place, the comparison happens where a *requirement* exists: before using any registry copy or Git checkout, `check_meta` (src/package.rs:1225) verifies that the stored `package.zson` declares exactly the `(name, version)` being installed — mismatch → ERROR `Invalid package metadata for '<name>': package.zson declares ...` (a registry that stores package X under Y's directory, or a Git repo whose manifest disagrees with the dependency name, must not be trusted — mechanical enforcement of §96's validity rule at the point §94/§95 make it observable). Optional `description`/`author`/`license`/`repository`, when present, must be strings (§96 allows "field tambahan untuk masa depan" — the four appear in no spec example, so they are only type-checked, never read). Unknown fields are ignored (Job 017's §66l rule, unchanged).

### 67h. §97 when a hash may be recorded (and when one must be checked)

§97: a recorded hash that differs from the computed one → ERROR, and the lockfile example carries `hash`. It does not say whether a hash must be *created* on first install, nor what happens when the source never provided one.

**Decision:** a hash is recorded **only when the source provided one** — `lock_identity` (src/package.rs:2196) takes the lockfile's existing hash, else the registry index's hash, else (Git only) the digest computed at clone time (§97's example is a Git package). A plain registry install whose index has no hash keeps §84's plain `"name": "version"` form; **no fresh digest is computed for it**, because (a) §97 only defines *comparison* against a provided hash, (b) computing one on first install and comparing later would make an unchanged local registry "fail" the moment any file's mtime-like content differs in a way the spec never discusses, and (c) nothing in §84-§96 requires local copies to be hashed. Verification always runs when a hash exists (lock first — "Hash yang berbeda → ERROR" — else index), before anything reaches `.zsc_m/` (`job018_index_hash_is_verified_and_recorded`, `job018_index_hash_mismatch_is_error`). A hand-written lockfile with a `commit` but no `hash` for a Git package re-clones and re-pins the commit (the commit, not a digest, is §92's reproducibility anchor).

### 67i. §98 cache: validity, roots, and what "verified" means

§98: verified content is stored in `.zsc_cache/`, invalid cache content must never be used, and clearing the cache must not affect the project. It defines no cache root for a project without one, no sidecar format, and no staging rules.

**Decision:** cache root precedence — `$ZPM_CACHE`, else `<project>/.zsc_cache`, else `$HOME/.zsc_cache` (mechanical; per-project first keeps tests and concurrent projects isolated). Layout `<name>/<version>/` + a `<version>.sha256` sidecar recording `hash::hash_dir` of the stored copy; `Cache::entry` re-hashes the stored tree and refuses a mismatch (§98's "cache tidak valid tidak boleh digunakan"), and a stored copy whose `package.zson` disagrees with the requested `(name, version)` is likewise refused (§97/§96 identity — a corrupted cache must not resurrect a wrong package). Staging directories (probe clones, install staging) live outside the cache and are removed — §98 defines no partial-entry rule. Store happens **after** verification, for both registry copies and Git checkouts (previously Git-only; registry copies now cache too, src/package.rs:2149). Clearing the cache changes no manifest/lock/`.zsc_m` (§98's "tidak menghapus instalasi") — `job018_clearing_cache_leaves_project_untouched`.

### 67j. §99 source priority: order, and whether priority beats the selector

§99: use a valid `git:` source first, fall back to cache, then registry; the source must not change the selected version. It does not say where the lockfile sits in that order.

**Decision:** the effective order is **lockfile → explicit `git:` → cache → registries in configuration order**: the lockfile is consulted first as *version* selection (§87/§92 — it is not a "source"), then within the selected version the source choice follows §99 literally. An explicit `git:` dependency never consults a registry (§99's first clause, tested by `job018_explicit_git_source_wins_over_registry` where registry 2.5.0 loses to `git:` 1.0.0). Priority never overrides the selector: the version is chosen from candidates first, the source only supplies content for that version. Unreachable git source with no valid alternative → ERROR (§99's fallback applies to *invalid/absent* states it can detect; a declared-but-unreachable source is the dependency's identity, not a cache miss — `job018_git_source_unreachable_is_error`).

### 67k. Mechanical details the spec does not state

- **Report/CLI output.** `+ <name>@<version>` per installed package and `Installed N package(s)` — §85 defines no output; Job 017's format kept (`+ name@version` also works with multi-version keys' plain name portion).
- **`zpm install <name>` / `git:<url>` declare-then-install.** §100 requires "setiap dependency yang dipakai harus dideklarasikan di `project.zson`", so the command writes the declaration *first* (`add_dependency`, src/package.rs:2497) and then installs. Consequence (spec-silent): if installation subsequently fails, the declaration **stays** in the manifest while the lockfile is untouched — the user can fix the registry and re-run `zpm install`. Writing manifest+install atomically was rejected: there is no transaction rule in §82/§85, and undoing a manifest edit on failure would be invented behavior.
- **Error wording** for new paths (`No version of 'x' matches selector 'y'`, `Package 'x' not found in any registry`, `Invalid package metadata for ...`, `git clone failed: ...`, `Integrity check failed ...`, `Package 'x' is not a direct dependency of the project`, `already published`, `No package.zson found`, `ERROR: usage: zpm update`): §90-§100 only require ERROR; all are `ERROR: `-prefixed, consistent with §66l.
- **Exit codes**: unchanged Job 017 convention (§66l) — 1 on error, 0 on success, `zpm run` passes the child's code through.
- **`update` arg check ordering**: the usage check runs *before* project discovery, so `zpm update x` reports usage even outside a project (mechanical; §92 shows `zpm update` bare).
- **No network in tests**: every Git test uses a local `file://`-free path clone (git clone of a local path), every registry is a temp directory; `ureq` is never invoked (Job 016's rule, still in force).
- **Supersession**: `test_job017_unresolvable_version_conflict_is_error` → `test_job018_disjoint_selectors_resolve_to_two_versions` (Job 017's expectation that disjoint selectors are an error was a §89-era reading; §90 supersedes it — see the §66e update). All other Job 017 tests pass unchanged.
