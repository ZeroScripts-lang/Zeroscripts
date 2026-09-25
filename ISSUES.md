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
