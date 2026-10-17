# ZeroScript — Open Issues / Not Yet Finalized

## 58. Job 008 Ambiguity Analyses (2026-09-23)

### 58a. §3 vs §4 vs Closure Capture (Function Scope Assignment) — **Resolved (2026-10-01)**
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

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 834 passed, 0 failed (2026-10-01). Evidence: test_job008_function_scope_assignment_does_not_change_outer (the §4 example: function assignment does not change outer A), test_job008_shadowing_different_scopes_valid; resolve_for_load / resolve_for_store paths present as described.

### 58b. One-Way Const Collection Marking (§3, §34, §39) — **Resolved (2026-09-29)**

Spec required `const` list/object element/property mutation to error (§34, §35, §39) and was silent on const-flow through aliasing:

- `let A = [1]; const C = A;` — does binding `C` freeze `A`'s object?
- Returning a const collection and storing the result in `let`.

**Readings considered:**

1. Const is a property of the *binding* only; aliasing does not freeze the object.
2. Const is a property of the *heap object* once frozen — the provisional Job 008 behavior (`mark_const_if_collection` set `HeapObject.is_const = true` for any binding that stored the value, one-way and sticky).

**Decision (reading 1, spec owner 2026-09-29):** `const` is an immutable **binding**, not an immutable **heap object**.

- `const C = A` does **not** freeze the object/list that `A` refers to.
- Rebinding `C` is still ERROR (`Cannot assign to const 'C'`).
- Mutating the object/list *through* `C` is allowed: `C[0] = …`, `C.k = …`, `list.add(C, …)`, `list.remove(C, …)`, `object.remove(C, …)`, and mutation through a function.
- A collection returned by a function that was previously referenced by a `const` binding, then stored in `let`, stays mutable.

**Still ERROR (binding level, unchanged):** `C = …`, `C++`/`C--`, rebind of a captured const, duplicate `let`/`const` declaration, assignment to an `export const`.

**Applied (2026-09-29):**

- Spec: `Language.md` §34 (`"const" list` paragraph + example), §35 (`mutation melalui function … ERROR` sentence), §39 (`"const" object` paragraph + bullets + example) rewritten to the binding-only rule; §3, §30, §31 and `Modules.md`'s `export const` rule untouched (already binding-level).
- Source: removed the six const-collection guards (`native_list_add`, `native_list_remove`, `native_object_remove`, `execute_set_index` ×2, `execute_set_property`), `mark_const_if_collection` plus its two store-site call sites, and `HeapObject::List/Object.is_const` with `is_const_collection`/`mark_const_collection`. Binding-level checks (`check_assign_const`, `Binding.is_const`, `EnvBinding.is_const`) untouched.
- Tests: 7 flipped ERROR → allowed (`test_job008_const_list_element_mutation_allowed`, `test_job008_const_object_property_mutation_{existing,new}_key_allowed`, `test_job010_list_add_const_allowed`, `test_job010_list_remove_const_allowed`, `test_job010_list_const_mutation_via_function_allowed`, `test_job010_object_remove_const_allowed`); 2 added (`test_job008_const_alias_does_not_freeze_object`, `test_job008_const_returned_collection_stored_in_let_mutable`); 2 catchability tests re-anchored to errors that still exist (`test_job010_list_add_catchable` → non-list argument, `test_job010_object_remove_catchable` → missing key).
- `CHECK.md`: rows for `const` enforcement, SetIndex/SetProperty, `list.*`, `object.*`, `const` immutable, critical bug #1, plus the §17 limitation bullet and the Job 008/009 candidate-gap entries updated in place.
- Verification: `cargo test` → 811 passed, 0 failed (767 lib + 11 + 25 + 8).

### 58c. Identity Equality for Lists/Objects under `===` — **Resolved (2026-10-01)**
§14: `===` compares raw value ignoring type; `==` compares value+type. For lists/objects, shared reference semantics (§35/§40) mean "same value" under `===` is identity (same HeapRef), not structural equality.

**Decision:** `raw_eq` for `Value::List`/`Value::Object` compares HeapRef handles (`x == y`), i.e., identity. Structural deep-equality under `===` is not implemented and is not required by any spec example. Test: `test_job008_shared_reference_via_raw_eq`.

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job008_shared_reference_via_raw_eq` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 58d. `&&` / `||` / `not` vs truthiness — **Resolved (2026-10-01)**

§16 forbids truthy/falsy; only booleans accepted. Lexer maps `&&`→`And`, `||`→`Or` (same tokens as keywords). Parser maps `not`→`UnaryOp::Not` (same as `!`). No ambiguity; verified by tests rejecting `10 and true` and `"hello" || false`.

**Decision:** there is no ambiguity — §16's boolean-only rule *is* the decision: `&&`/`||` accept only booleans (non-boolean operand → catchable runtime ERROR), `not` is exactly `!`, and `&&`/`||` are the same lexer tokens as `and`/`or` (no truthy/falsy conversion anywhere in the language). Recorded as the decided reading; behavior was already locked by Job 008 tests.

**Resolved (2026-10-01):** closed per user answer (jawaban.md, option A) — no code or spec change; evidence: `test_job008_ampersand_and_alias`, `test_job008_and_or_not_identical_to_keywords`, `test_job008_no_truthy_and_non_boolean`, `test_job008_no_truthy_or_non_boolean` all passing (full suite `cargo test` → 834 passed, 0 failed).

---

## 59. Job 009 Ambiguity Analyses (2026-09-23)

### 59a. Runtime representation of caught error `e` (§43) — **Resolved (2026-09-29)**

Spec said only: `"e" merupakan local variable yang berisi error` and showed `print.err(e)`. Runtime.md §154 says runtime errors have information usable by catch, which *may* include error type, message, source location, call stack.

**Readings considered:**

1. **`e` is a plain string** containing the error message (the provisional Job 009 decision).
2. **`e` is an object** with fields such as `type`, `message`, `location`.
3. **`e` is a dedicated Error value type** — Spec has no Error type in §6 Types.

**Decision (reading 2, spec owner 2026-09-29):** `e` is an **object**.

- Minimal fields: `e.type`, `e.message`, `e.location`.
- `e.location` is `{file, line, column}` or `null`.
- `e.type` is an error category such as "TypeError", "ReferenceError", "RuntimeError", "SyntaxError", … chosen per the failing operation (categories per §154, adjustable "selama jenis kegagalan dapat dibedakan secara konsisten").
- `type(e)` → `"object"`.
- `string(e)` → `e.message`.
- `print.err(e)` still produces a human-readable error line (the message).
- Syntax/parser errors before runtime remain uncatchable; a *runtime* parse failure such as `json.parse("{bad")` is catchable and reports as `"SyntaxError"`.

**Applied (2026-09-29):**

- `Language.md` §43: `e` documented as an object with `e.type`/`e.message`/`e.location`, the `type(e)`/`string(e)`/`print.err(e)` behaviour, the category list, and the runtime-parse → `SyntaxError` sentence. `Runtime.md` §154 already covered the underlying error info — untouched.
- `src/vm.rs`:
  - `make_error_object` builds `{type, message, location}` at the single catch-binding site in `raise_runtime_error`.
  - `HeapObject::Object { props, is_error }` carries an **internal** error identity (`HeapObject::error_object` sets it, `HeapObject::object` never does). `Value::error_message` returns a message only when that flag is set; it is not observable from ZeroScript — a plain object with `type`/`message`/`location` stays an ordinary object for `string()` and `print.err()` (regression test `test_job059a_plain_object_with_error_fields_is_not_error`). ZeroScript-level API/syntax unchanged: `e` is still an ordinary object.
  - `classify_runtime_error` maps the existing message text to a category: `Invalid JSON:` → SyntaxError; out of bounds/Index error → IndexError; Key/Property not found → KeyError; Division/Modulo by zero → DivisionError; Undefined variable → ReferenceError; cancelled → CancellationError; expects at least/exactly + "Expected N arguments" → ArgumentError; `Type error`, `bad type`, `Expected <Type>`, `Invalid conversion`, `Conditional jump requires`, `expects a <type>`, ` must be `, and the operator prefixes (Cannot add/apply/call/compare/divide/index/modulo/multiply/negate/power/set index/set property/stringify/subtract/access property) → TypeError; everything else → RuntimeError.
- `e.location` is always `null` today: the VM carries no source position for runtime errors, and §154 allows location only "jika informasi tersebut tersedia". Populating `{file, line, column}` would require position tracking in the bytecode — explicitly **not** part of this decision.
- Tests: 12 added (`test_job059a_*`): object-ness of `e`, the three fields (IndexError category, message, `location` null), `string(e)` == `e.message`, `print.err(e)` message-only output, `print(e)` object dump, json.parse → SyntaxError, TypeError (operator + conversion), DivisionError, KeyError, parse error not catchable, `string()` on a plain object still erroring (§54/§59c unchanged), and a plain object carrying all three error field names not being treated as an error (internal-identity regression). `cargo test` → 823 passed, 0 failed.

**Superseded provisional decision (reading 1):** `e` bound as `Value::Str(error_message)`; `print.err` wrote the formatted string to the error output stream (StandardLibrary §42).

**Not blocked by Job 008 point 13 limitations** — none of those (default params, closure-in-data, GC roots, const aliasing) affect try/catch, type(), or conversions.

### 59b. Which errors are catchable — **Resolved (2026-09-29)**

§43: catch captures runtime errors; syntax/parse errors not caught (happen before execution). Runtime.md §154: VM Error is distinguished from ZeroScript runtime error.

**Decision:** Only `VmError::RuntimeError(_)` is routed through `raise_runtime_error` (handler stack). `InvalidBytecode`, `InvalidOpcode`, `StackUnderflow` from internal VM misuse, etc., abort execution uncaught. Compile-time errors never reach the VM.

**Resolved (2026-09-29):** implemented as decided — `run` and `run_module_range` route only `VmError::RuntimeError(msg)` through `raise_runtime_error`; every other `VmError` variant and every compile/parse error aborts uncaught (vm.rs:799-805, 983-989). Evidence: test_job009_error_outside_try_still_fails, test_job009_finally_runs_when_no_catch_then_rethrows, test_job009_parse_error_not_caught, test_job059a_syntax_error_not_catchable; CHECK.md row "VM Error distinct from runtime error" (§154).


### 59c. Conversion rules beyond shown examples (§54) — **Resolved (2026-09-29)**

Spec shows: `number("123")`, `number("3.14")`, `string(123)`, `string(true)`, `boolean(true)`, errors for `number("hello")`, `boolean(1)`, `boolean("true")`. Silent on: `number(123)` (identity), `string(null)`, `string("x")` (identity), `string([..])`, `boolean(false)` (shown via identity of true only).

**Decisions (documented, minimal invention):**
- `number`: `Number` identity; `String` strict f64 parse; else ERROR.
- `string`: `Str` identity; `Number` (integer form without `.0` when whole); `Boolean`; `Null` → `"null"` (matches Display); `List`/`Object`/`Function` → ERROR (invalid conversion).
- `boolean`: only `Boolean` identity; all else ERROR (no truthy/falsy, §54).

**Resolved (2026-09-29):** implemented as decided in the `number`/`string`/`boolean` arms of `call_native` (vm.rs:2570-2606). One later exception added by §59a (Resolved): `string(<caught error>)` returns `e.message`; list/object/function still ERROR. Evidence: test_job009_number_from_string_success, test_job009_number_invalid_string_error, test_job009_string_from_list_error, test_job009_boolean_from_number_error, test_job009_conversion_errors_catchable, test_job059a_string_e_returns_message, test_job059a_string_plain_object_still_errors.


### 59d. `finally` on `break`/`continue` early exit

§43: `"finally" selalu dijalankan`. Job 009 text: also on early exit like `return`. `return`-through-finally is implemented (`pending_return` + `FinallyEnd`). `break`/`continue` that jump out of a try region do **not** currently run finally (compiler does not emit finally hooks on those jumps). Recorded as known limitation / candidate gap in CHECK.md — not silently ignored; may be fixed without changing §43.

**Resolved (2026-09-29):** implemented without changing §43, the opcode set, or `Bytecode.md` — via *continuation markers* on the value stack. Every entry into a `finally` body pushes exactly one four-slot marker `[scope-pops, target pc, handler-pops, sentinel]` (`FINALLY_MARKER`, compiler.rs `emit_finally_marker`), and `FinallyEnd` (vm.rs `execute_finally_end`) always consumes the top marker first: `pending_error`/`pending_return` branches discard it (behavior unchanged from the `pending_*` design); otherwise it pops the recorded crossed handlers, unwinds that many scopes, and jumps to the recorded target. Entries and their markers: try-body success and catch completion (compiler, target = after-try), error entry (`raise_runtime_error`, dummy marker after stack truncation), `return` entry (`execute_return`, dummy marker), and `break`/`continue` chains (compiler `compile_early_exit`: `TryPop`× up to and including the innermost crossed try with a finally, markers for every crossed finally innermost-first, `Jump` into its entry). Intermediate markers target the next crossed finally's entry, the last targets the loop destination — all patched at try/loop completion (patchable constants excluded from dedup so patches cannot alias ordinary constants). Crossing a try with catch but no finally now pops its handler too (previously leaked a stale handler). Break/continue inside a finally body discards that finally's live marker (`Pop`×4) before the crossing logic, so no marker is ever orphaned. Recorded limitations (spec silent, no new decisions taken): (a) if a `break` inside a finally entered with `pending_error`/`pending_return` jumps away, the pending value is consumed by the *next* `FinallyEnd` rather than at the break — pre-existing behavior, unchanged by this fix; (b) `return` inside a finally during a break chain abandons the remaining markers via stack truncation (chain superseded by return, §43). Evidence: 11 new tests `test_job059d_break_runs_finally`, `test_job059d_continue_runs_finally`, `test_job059d_break_nested_finally_innermost_first`, `test_job059d_continue_nested_finally_innermost_first`, `test_job059d_break_inside_finally_body`, `test_job059d_break_runs_finally_in_infinite_loop_stmt`, `test_job059d_break_pops_catch_only_handler`, `test_job059d_error_in_finally_during_break_chain_catches_outer`, `test_job059d_break_in_try_inside_function`, `test_job059d_return_from_finally_supersedes_break_chain`, `test_job059d_break_unwinds_try_scope_before_finally`; full suite `cargo test` → 834 passed, 0 failed (CHECK.md §17, §18 gap rows updated).

---

## 60. Job 010 Ambiguity Analyses (2026-09-24)

### 60a. `object.remove` on missing key (§39) — **Resolved (2026-09-29)**

§39 shows `object.remove(user, "umur")` (successful case) but does not state behavior when the key does not exist. §38 (Object *Access*) explicitly says `Missing key → ERROR`.

**Readings considered:**

1. **ERROR** (analogous to §38 access): removing a non-existent key is an invalid operation on the object, consistent with access semantics.
2. **No-op / return `null`**: removal of something that isn't there is harmless; some languages return a status.
3. **Return boolean** indicating whether something was removed.

**Decision (reading 1):** `object.remove` on a missing key → `RuntimeError` (`"Key '<k>' not found"`). Grounded by §38's explicit missing-key rule applied to the same object namespace; spec is silent on remove-specific behavior, so no field/return shape is invented (AGENTS.md §12). Catchable via try/catch like other runtime errors. If Spec later defines remove-missing as no-op, revisit the `props.len() == before` branch in `call_builtin`.

**No Job 008 point-13 limitation blocked this job** — none of (default params, closure-in-data, GC roots, const aliasing) affect string/list/object builtins or print.

**Resolved (2026-09-29):** reading 1 implemented — `object.remove` on a missing key raises `RuntimeError("Key '<k>' not found")` and is catchable. Evidence: `native_object_remove` (vm.rs:3012); tests test_job010_object_remove_missing_key_error, test_job010_object_remove_catchable.


### 60b. Soft CALL_NATIVE vs formal `CALL_NATIVE` opcode (§145-§146) — **Resolved (2026-09-29)**

Job 010 required explaining how builtins connect to the VM. Spec §145 allows "mekanisme internal lain yang setara" to `CALL_NATIVE list.length`. Formal `CALL_NATIVE` opcode is Job 019.

**Decision:** Job 010 uses a **soft native boundary**: namespace objects (`string`, `list`, `object`, `print`) are `Value::BuiltIn(ns)` in root scope; `GetProperty` resolves known method names to `Value::BuiltIn("ns.method")`; `ExecuteCall` dispatches all BuiltIn values through `call_builtin`. No new opcodes. Observable behavior is identical to a future `CALL_NATIVE` migration (Job 019 can swap the dispatch without changing call sites or results).

**Resolved (2026-09-29):** superseded by Job 019 — the formal `CALL_NATIVE` opcode now exists for statically resolvable call sites (`Compiler::static_call_target` → `Opcode::CallNative` → `execute_call_native` → `call_native`), while the dynamic `Value::BuiltIn` + `GetProperty` + `Call` path from this decision is retained on purpose for unresolvable callees (§145 "mekanisme internal lain yang setara"; ISSUES §68a-§68c). Both paths land in the single native boundary. Evidence: tests test_job019_call_native_for_root_builtin, test_job019_call_native_for_namespace_methods, test_job019_native_call_arity_error_is_runtime_error.


---

## 61. Job 011 Ambiguity Analyses (2026-09-24)

### 61a. Color nesting resets (§45) — **Resolved (2026-10-01)**
§45 requires `color.bold(color.red("Error"))` to combine, but does not specify ANSI codes or reset strategy.

**Readings considered:**

1. **Full reset (`\x1b[0m`)** after each color: simple, but outer `bold` is cleared when inner `red` closes — nesting order `bold(red(x))` works only if `red` closes before `bold` re-applies (it doesn't; closes are inner-first). Specifically `bold_start + red_start + x + reset + reset` leaves no bold after the first reset.
2. **Selective resets** (`\x1b[39m` default-fg for colors, `\x1b[22m` normal-intensity for bold): inner close does not clear the outer attribute.

**Decision (reading 2):** `red/green/yellow/blue` → `\x1b[3Xm` … `\x1b[39m`; `bold` → `\x1b[1m` … `\x1b[22m`. No external dependency (§45, AGENTS.md no-unnecessary-deps). Spec is silent on exact codes — codes are the standard ANSI SGR set, not a language rule.

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 834 passed, 0 failed (2026-10-01). Evidence: test_job011_color_bold_combines_with_red exercises the nested bold(red(x)) case with inner close codes; passing.

### 61b. `file.write` / `file.remove` / `file.mkdir` return value (§48) — **Resolved (2026-10-01)**
§48 shows call forms only; no return values stated.

**Readings considered:**

1. Return `null` (side-effecting ops; matches `list.add`/`object.remove` → null).
2. Return `true`/status boolean.
3. Return path/content echo.

**Decision (reading 1):** mutators return `null`; `file.read` returns string; `file.exists` returns boolean (stated in job/scope). No fields or shapes invented (AGENTS.md §12).

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 841 passed, 0 failed (2026-10-01; 7 direct tests added). Implementation verified: file.write / file.remove / file.mkdir return Ok(Value::Null), file.exists returns boolean (src/vm.rs file.* natives); test_job011_file_write_read_roundtrip, test_job011_file_remove_then_not_exists, test_job011_file_mkdir passing. Follow-up closed 2026-10-01: direct test added — test_issue61b_file_mutators_return_null asserts file.write/file.remove/file.mkdir each return null (suite 841 passed, 0 failed).

### 61c. `file.mkdir` recursive vs single-level (§48) — **Resolved (2026-10-01)**
Spec shows `file.mkdir("data")` only.

**Readings:** (1) `create_dir` — parent must exist, missing parent → catchable error; (2) `create_dir_all` — creates parents silently.

**Decision (reading 1):** single-level `create_dir`. Simpler, errors surface through try/catch as §48 requires for filesystem errors; recursive creation is not specified.

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 841 passed, 0 failed (2026-10-01; 7 direct tests added). Implementation verified: file.mkdir uses std::fs::create_dir (single-level, src/vm.rs); test_job011_file_mkdir passing. Follow-up closed 2026-10-01: direct test added — test_issue61c_file_mkdir_missing_parent_is_error asserts a missing parent raises a catchable error (suite 841 passed, 0 failed).

### 61d. `system.arch()` value format (§52) — **Resolved (2026-10-01)**
Spec example: `"arm64"`. Host `std::env::consts::ARCH` on this machine is `"aarch64"`.

**Readings:** (1) return Rust `ARCH` as-is (`aarch64`/`x86_64`/…); (2) map to marketing names (`aarch64`→`arm64`).

**Decision (reading 1):** return `std::env::consts::ARCH` unmodified. Spec says "Contoh" (example), not a normative enumeration; inventing a mapping table would be an unstated transform (AGENTS.md §12). Tests assert non-empty string type only. Same for `system.os()` → `std::env::consts::OS` (`"linux"` matches example).

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 834 passed, 0 failed (2026-10-01). Implementation verified: system.arch returns std::env::consts::ARCH unmodified (src/vm.rs); tests test_job011_system_arch_returns_string and test_job011_system_os_arch_nonempty assert type and non-empty string exactly as the decision states; passing.

### 61e. `system.env` non-UTF-8 values (§52) — **Resolved (2026-10-01)**
Spec: string or `null` if missing. `std::env::var` also fails with `NotUnicode` for values that exist but are not valid UTF-8.

**Decision:** both `NotPresent` and `NotUnicode` → `null`. Spec has no type for "bytes that aren't a string"; returning null preserves the stated string-or-null contract without inventing a binary type.

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 834 passed, 0 failed (2026-10-01). Implementation verified: system.env maps both NotPresent and NotUnicode to null (Err(_) => Ok(Value::Null), src/vm.rs); test_job011_system_env_missing_returns_null passing. The NotUnicode branch is the same arm (no portable non-UTF-8 env test exists); closed with note.

### 61f. `json.parse` error representation (§51, Job 011 scope note) — **Resolved (2026-10-01)**
Spec does not define the error message for invalid JSON. Job instructs: use existing runtime-error mechanism, catchable via try/catch, report assumptions.

**Decision:** `VmError::RuntimeError(format!("Invalid JSON: {}", detail))` where `detail` is the hand-rolled parser's position/message (e.g. `Unterminated string`, `Unexpected character 'b' at position 1`). Catchable via try/catch; since §59a (Resolved 2026-09-29) `e` is an error object whose `type` is `"SyntaxError"` and whose `message` is the `Invalid JSON: …` text. Exact wording is mechanical (spec-silent), not spec-derived.

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 834 passed, 0 failed (2026-10-01). Evidence: test_job011_json_parse_invalid_catchable (catchable, Invalid JSON message), test_job011_json_parse_invalid_uncatchable_surfaces (message surfaces when uncaught), test_job059a_json_parse_failure_is_syntax_error (e.type SyntaxError, e.message text) — all passing.

### 61g. `json.stringify` accepted top-level types (§51) — **Resolved (2026-10-01)**
Job text: "A adalah object/list". §51 examples show only objects/lists. `json.parse` can return any JSON value (RFC 8259 allows top-level scalars).

**Readings:**

1. **Strict:** only object/list accepted at top level; `json.stringify(json.parse("42"))` errors.
2. **Closed set of JSON types:** null, boolean, number, string, list, object accepted; functions/closures error (no JSON form).

**Decision (reading 2):** stringify accepts exactly the JSON data types so parse↔stringify roundtrips any valid JSON document; rejects function/built-in/closure with catchable error. Non-finite numbers (NaN/±Infinity) also error — not representable in JSON. Job's "object/list" phrasing describes §51's examples, not an exclusive type restriction; excluding scalars would make roundtrip asymmetric with parse.

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 841 passed, 0 failed (2026-10-01; 7 direct tests added). Implementation verified: json.stringify accepts the JSON data types including top-level scalars and rejects functions/closures with a catchable error (src/vm.rs json.stringify arm, roundtrip comment); roundtrip tests test_job011_json_stringify_parse_roundtrip_object/_list passing. Follow-ups closed 2026-10-01: direct tests added — test_issue61g_json_stringify_top_level_scalars (42/"hi"/true/null accepted) and test_issue61g_json_stringify_rejects_function_and_non_finite (function value and 1e400 both caught) (suite 841 passed, 0 failed).

### 61h. Duplicate keys on `json.parse` (§51) — **Resolved (2026-10-01)**
JSON (RFC 8259) allows duplicate names; parsers commonly keep last-wins.

**Decision:** last-wins (replace value for existing key, preserve first-seen key order). Spec silent; matches common parser behavior; no language rule invented.

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 841 passed, 0 failed (2026-10-01; 7 direct tests added). Implementation verified: parse_object last-wins on duplicate keys (src/json.rs:522 comment + insert-overwrite); full suite passing. Follow-up closed 2026-10-01: direct test added — test_issue61h_json_duplicate_keys_last_wins asserts last value wins with first-seen key order preserved (suite 841 passed, 0 failed).

### 61i. Color argument types (§45) — **Resolved (2026-10-01)**
§45 examples pass strings only; no type rule stated (unlike §58 string API "Argumen dengan tipe salah → ERROR").

**Decision:** any single argument accepted; rendered via `format_value` (same as `print`). Arity must be exactly 1. Rationale: color is a formatting wrapper; requiring string would forbid e.g. `color.red(10)` without spec basis. Non-collection types format identically to `print`; collections format via heap contents.

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 841 passed, 0 failed (2026-10-01; 7 direct tests added). Implementation verified: arity exactly 1 -> error (test_job011_color_wrong_arity_error passing) and the argument is rendered via format_value for any type (src/vm.rs color arm). Follow-up closed 2026-10-01: direct test added — test_issue61i_color_accepts_non_string_argument asserts number/bool/list arguments render exactly like print (suite 841 passed, 0 failed).

### 61j. `system.args()` host contract (§52) — **Resolved (2026-09-29)**

§52: program file name excluded. `ZeroVM` receives `program_args` via `set_program_args`; default empty.

**Decision:** stripping interpreter/script argv is the **host's** responsibility when populating `program_args` — matches "zerovm app.zs hello world → [hello, world]" where the launcher knows argv layout. `main.rs` does not yet execute `.zs` files, so no CLI wiring in this job (out of scope; noted as known limitation in CHECK.md). Tests inject args explicitly via `set_program_args`.

**Resolved (2026-09-29):** host wiring landed — `zsc <file.zs> [args...]` (src/main.rs:17-38) strips interpreter and script name and calls `set_program_args` before `run_modules`. Evidence: integration test cli_zsc_passes_program_arguments (`zsc index.zs hello world` → [hello, world]); unit tests inject via `set_program_args` (vm.rs:8107); CHECK.md §18 bullet "CLI runner wiring for system.args()".


---

## 62. Job 012 Ambiguity Analyses (2026-09-24)

### 62a. §73 vs §78: Module Evaluation Failure and try/catch — **Resolved (2026-10-01)**
**Readings considered:**

1. **§78-strict (implemented)**: `import` is a static module declaration processed before any module body runs (loader builds graph, topo-sorts, then `run_modules` executes ranges in order). A dependency's runtime error aborts `run_modules` before the importer body starts. Consequence: the error is **not** catchable by try/catch inside the importer (the importer never executes). §73's "error can be caught if import occurs in a runtime-executed context" is unreachable under §78.

2. **§73-runtime reading**: treat import as a runtime operation so try/catch around the import site can catch eval failure. Consequence: contradicts §78's "import is static; position does not determine when dependency is loaded" and "all dependencies must be completed before module code starts."

3. **Hybrid**: run dependencies first; if a dep fails, raise a deferred error into the importer's try handlers when the importer body starts. Not specified by either section; invents a new error-injection rule.

**Decision (reading 1):** §78 is explicit and normative for ordering (`test_job012_import_mid_file_static`). §73's catch clause is only consistent with §78 if "runtime operation" means host-level `run_modules` failure (returned as `Err`), not in-language try/catch. Failed modules are not cached (scope popped, no export snapshot) so a subsequent `load` would retry — but within one program run, `run_modules` stops. Job file G asked to report rather than guess: **this is the reported contradiction**. No in-language catch of static import failure is implemented.

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job012_import_mid_file_static` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01). User accepted reading 1 (2026-10-01): §78 static import ordering is normative — a dependency's evaluation failure aborts run_modules before the importer body runs, so it is not catchable by in-language try/catch; §73's catch clause applies only to host-level run_modules failure. Implemented exactly so (test_job012_import_mid_file_static passing); the reported §73-vs-§78 contradiction is hereby closed without changing either section.

### 62b. Export value timing (Job 012 snapshot vs §66 live binding) — **Resolved (2026-09-29)**

§66/§74 require live binding (importer sees later mutations of `export let`). Job 013 owns live binding; Job 012 snapshots export values into a cache after each module's `EndModule`.

**Decision:** snapshot is an intentional Job 012 limitation (CHECK.md "Live binding → Not Implemented / Job 013"). Functions still work for mutation tests because the closure captures module state; only direct `import { count }` reads of a mutated `export let` would be stale until Job 013.

**Resolved (2026-09-29):** Job 013 implemented live binding — `ExportBinding::Live` with `Binding.live` HeapRef into the source Environment, `StoreCaptured` sync and `StoreLocal` write-through; the Job 012 snapshot limitation no longer applies. Evidence: test_job013_live_binding_counter (§66 example: reads 0 then 1); CHECK.md row "Live binding" (§66, §74).


### 62c. Import bindings declared const — **Resolved (2026-10-01)**
Job 012 declares import names with `is_const: true` at compile time. §74 forbids assignment to imported bindings; §69 alias/conflict detail is Job 013. Const declaration implements the assignment ban early without inventing a separate "imported" binding kind.

**Decision:** imports are const locals. Assignment to an import errors as `Cannot assign to const` (same mechanism as ordinary const).

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 834 passed, 0 failed (2026-10-01). Evidence: test_job013_assign_to_imported_const_error (assigning to an imported binding errors) passing.

### 62d. Package/bare import paths — **Resolved (2026-09-29)**

Job 012 Scope: `import` without `./` or `../` is Job 016-017.

**Decision:** bare paths return `ModuleError::NotFound` at load time (no silent resolution against importer dir). Absolute `/...` paths are accepted by `resolve_path` (spec §64 allows runtime support) but are not tested as primary form.

**Resolved (2026-09-29):** superseded by Job 017 — bare `from "pkg"` now resolves to the installed package in `.zsc_m` (manifest/package rules); `ModuleError::NotFound` still applies to packages that are not installed or not declared, and relative `./`, `../` paths are unchanged. Evidence: test_job017_package_import_resolves_in_zsc_m, test_job017_package_import_without_manifest_is_error, test_job017_package_cannot_import_undeclared_package, test_job017_project_cannot_import_transitive_dependency.


---

## 63. Job 014 Ambiguity Analyses (2026-09-25)

### 63a. Top-level await: rules vs examples (§49/§101 rules vs §49/§112 examples) — **Resolved (2026-10-01)**
**Readings considered:**

1. **Strict rules (implemented)**: §49 states "await digunakan di dalam async function" and §101 states "'await' hanya dapat digunakan di dalam 'async function'". Job 014 Scope A mandates exactly this as a compile-time ERROR. Under this reading, top-level `await` is rejected at compile time.

2. **Examples reading**: §49's own invocation example (`let data = await ambilData()` shown outside any function body) and §112's scheduler example (`await task.all([taskA, taskB])` at top level, after top-level `let taskA = A()`) both require the script itself to run in an implicit async context where top-level await is legal.

**Decision (reading 1):** the rules are stated normatively; the examples show invocation snippets whose surrounding context is unspecified. Top-level await → compile error `'await' is only allowed inside an async function`. Consequence: §112's example cannot run as written. Job 015 (owner of `task.all`/scheduler) must resolve how that example executes (implicit top-level async context, mandatory `async` main wrapper, or a corrected example). Reported per Job 014 Scope "spec silent/contradictory → stop and report", not guessed.

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 834 passed, 0 failed (2026-10-01). Evidence: top-level await rejected at compile time ('await' is only allowed inside an async function, src/compiler.rs) with test_job014_await_outside_async_is_compile_error passing. How §112's example executes is tracked separately in §64a (wrapper convention, documented in CHECK.md §18).

### 63b. `await` on a non-task value (spec silent) — **Resolved (2026-10-01)**
§107 defines await semantics only for tasks (completed → stored result, running → wait, failed → runtime error, cancelled → cancellation error). Behavior of `await 5` is unstated.

**Readings considered:**

1. **Runtime ERROR at the await point (implemented)** — message `await expects a task`.
2. **Passthrough** — return the value unchanged; makes await a no-op on non-tasks and silently accepts mistakes.

**Decision (reading 1):** consistent with §106's requirement that `task.state(A)` with a non-task argument be an ERROR, and with the general rule that unspecified behavior becomes an explicit error rather than invented semantics. The error is a normal runtime error and is catchable by try/catch (`test_job014_await_non_task_is_error`).

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job014_await_non_task_is_error` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 63c. Mechanical details the spec does not state — **Resolved (2026-10-01)**

- `type(task)` → `"task"`: §53's type-introspection table has no task entry; `"task"` chosen for symmetry with the value kind. (Reported, not spec-derived.)
- Display of a task value → `<task N>` (N = task id). No spec-mandated rendering.
- `task.state` argument errors and error message wording (e.g. `task.state() expects a task`) are implementation wording, not spec text.

**Decision (mechanical, spec-silent — reported, not derived):** (1) `type(task)` → `"task"` (symmetry with the value-kind names; §53's table has no task row); (2) display of a task value → `<task N>` where N is the task id; (3) `task.state` argument errors and their message wording are implementation text — the spec only requires ERROR.

**Resolved (2026-10-01):** closed per user answer (jawaban.md, option A — all three kept as-is); evidence: `test_job014_async_call_creates_pending_task` asserts `type(t)` == `"task"`, `test_job014_task_state_non_task_error` covers argument errors; full suite → 834 passed, 0 failed.

### 63d. `await` of a "running" task: wait vs error (§107 vs deadlock) — **Resolved (2026-09-29)**

§107: "await terhadap task yang masih berjalan akan menunggu sampai task mencapai terminal state."

Under Job 014's lazy-start single-task model, `execute_await` only observes state `running` when the awaited task is already running *because of the awaiting context itself* (self-await or an await cycle) — the normal flow transitions pending→running inside `execute_await` itself. Literal "wait" in the cycle case means deadlock with no progress.

**Decision:** runtime error `Task is already running` when awaiting a running task (spec silent on cycles). **Job 015 note:** once the scheduler starts tasks independently (`task.sleep`, `task.all`), a *different* task awaiting a running task must wait per §107 — the `Running` branch must be revisited then, or §113/§107 behavior breaks.

**Resolved (2026-09-29):** Job 015 revisited the branch as planned — `execute_await` now sets `wait` for both `Pending` and `Running` targets, so a *different* task awaiting a running task waits per §107; self-await raises "Cannot await a task from within itself" and await cycles raise the deadlock error. Evidence: test_job015_concurrent_creation_before_await (awaits a running task while another completes), test_job015_await_cycle_deadlock_error, test_job015_sleep_ordering_not_creation_order.


### 63e. Scheduler placement under Job 014's single-task scope (§112 vs job scope) — **Resolved (2026-10-01)**
§112 gives the scheduler responsibility for running `pending`/`running` tasks; Job 014 Scope C is single-task lifecycle (create → await) and explicitly leaves concurrency to Job 015.

**Decision:** async calls create tasks in `pending` (lazy start); a task body runs only when awaited (await-start) or when `drain_pending_tasks()` drains the FIFO queue at the end of `run()`/`run_modules()`. This satisfies §108 (an un-awaited task that fails stores its error without stopping the program or other tasks — `test_job014_unawaited_failure_does_not_stop_program`) and lets every program end with all tasks in a terminal state, without implementing interleaved/concurrent execution. Interleaving, `task.all`/`race`, and independent task starts remain Job 015.

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job014_unawaited_failure_does_not_stop_program` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 63f. Unreachable `cancelled` state (§105 vs Job 014 scope) — **Resolved (2026-09-29)**

§105 (`task.cancel`) and §106's `cancelled` state are not in Job 014's scope; without `task.cancel` no code path can produce a cancelled task. `TaskState::Cancelled` exists in the type but is unreachable in Job 014 tests. Cancellation tests deferred to Job 015 (job file permits deferring, must be reported).

**Resolved (2026-09-29):** Job 015 implemented `task.cancel` (`cancel_task`, vm.rs:1900) — `TaskState::Cancelled` is reachable. Evidence: test_job015_cancel_running_task (running → cancelled), test_job015_cancel_completed_noop, test_job015_await_cancelled_is_error; CHECK.md row "Task states" notes §63f closed by Job 015.


## 64. Job 015 Ambiguity Analyses (2026-09-25)

### 64a. §112 example still requires a wrapper (follow-up to §63a) — **Resolved (2026-10-01)**

§63a (reading 1, implemented) makes top-level `await` a compile error because §49/§101 state the rule normatively. §112's scheduler example writes `await task.all([taskA, taskB])` at top level after top-level `let taskA = A()`.

Job 015 owned the resolution (per §63a) and keeps the compile error: the example executes in tests only when wrapped in `async function main() { ... }` plus a top-level `main()` call (`test_job015_sleep_ordering_not_creation_order` runs exactly §112's logic). Readings considered: (1) implicit top-level async context — rejected, it would contradict the §49/§101 rules; (2) mandatory `async` main wrapper (implemented); (3) editing the spec example — not permitted without a spec job. The literal example text remains non-runnable as written; this is left as a spec-level documentation ambiguity, not silently changed.

**Decision (option B, user 2026-10-01):** §49/§101 stay normative — top-level `await` remains a compile error, and no implicit top-level async context is introduced (that reading was rejected). §112's example follows the wrapper convention: its body runs inside `async function main() { ... }` with a top-level `main()` call, and the §112 example text in Async.md was edited (2026-10-01) to show exactly that wrapped form so readers can run it as written. The rule sections are unchanged; the edit is cosmetic and documentation-level.

**Resolved (2026-10-01):** closed per user answer (jawaban.md, option B) — Async.md §112's example now shows the `async function main()` wrapper (cosmetic spec edit; §49/§101 rules unchanged). Evidence: `test_job015_sleep_ordering_not_creation_order` runs §112's exact logic under the wrapper; `test_job014_await_outside_async_is_compile_error` keeps top-level await a compile error; full suite → 834 passed, 0 failed.

### 64b. Self-cancellation (§105 silent) — **Resolved (2026-10-01)**
§105 defines `task.cancel(A)` for tasks that are still running but says nothing about cancelling the task whose body is executing the call (constructible when the task handle is stored in a global the body can read, e.g. `let t = selfy()` with `selfy`'s body calling `task.cancel(t)`).

**Decision:** runtime ERROR `Cannot cancel the currently running task` — a single-threaded cooperative scheduler cannot unwind its own running frame, and inventing silent re-scheduling semantics is not spec-supported. The error is a normal catchable runtime error (`test_job015_self_cancel_is_error`). Note: the spec provides no way to obtain the *current* task's handle (`task.current()` does not exist), so self-cancel is only reachable via an externally stored handle.

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job015_self_cancel_is_error` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 64c. Await cycles / deadlock (§107, §112 silent) — **Resolved (2026-10-01)**
§107 says awaiting a running task waits for the terminal state; §112 says a suspended task must not block runnable ones. Neither covers a cycle (task A awaits B while B awaits A): waiting per §107 means neither ever reaches a terminal state, and there is no timer to advance.

**Decision:** when the scheduler has no runnable task and no timer, but non-terminal tasks remain, it raises runtime error `Task deadlock: remaining tasks are waiting on each other` instead of hanging (`test_job015_await_cycle_deadlock_error`). Message wording is implementation text, not spec-derived. §107's literal "wait" for a running task is honored for all non-cyclic cases; the Running branch added in Job 015 awaits the target's terminal state (`test_job014` semantics unchanged).

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job015_await_cycle_deadlock_error` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01). (The decision's test_job014 reference is the test_job014_* suite — semantics unchanged; the decisive test is test_job015_await_cycle_deadlock_error.)

### 64d. Mechanical details the spec does not state — **Resolved (2026-10-01)**

- **Virtual clock.** `task.sleep`/`task.timeout` are measured on a monotonic *virtual* clock advanced by the scheduler to the earliest deadline only when nothing is runnable; the spec exposes no wall-clock API, so no observable behavior derives from real time. Tests are therefore instant and deterministic; real-time pacing is not implemented (known limitation, reported).
- **Timeout/empty-race/duration/deadlock error wording** (e.g. `task.timeout(): task did not complete before the timeout`, `task.race() requires at least one task`, `... must be a non-negative integer number of milliseconds`) is implementation wording; the spec only says ERROR.
- **`task.sleep` without `await`.** §111 says sleep "harus digunakan dengan await", but the spec defines no mechanism for detecting an un-awaited sleep. An un-awaited `task.sleep(ms)` creates a background timer task that completes silently; it is not rejected (reported as a limitation rather than an invented compile/runtime rule).
- **`task.all`/`task.race`/`task.wait`/`task.timeout` return values** are task values awaited by the surrounding `await` (the combinator is itself a synthetic task); this composition is an implementation choice — the spec describes only the awaited result of the combined call.

**Decision (all four items kept as-is, user 2026-10-01):** (1) `task.sleep`/`task.timeout` are measured on a monotonic *virtual* clock that advances to the earliest deadline only when nothing is runnable — tests are instant and deterministic; real-time pacing is not implemented (known limitation, tracked by a CHECK.md candidate-gap row); (2) timeout/empty-race/duration/deadlock error wording is implementation text (the spec only says ERROR); (3) an un-awaited `task.sleep(ms)` is not rejected — §111 states the await requirement but defines no detection mechanism, so it becomes a silent background timer task (reported limitation; inventing a rule was rejected); (4) `task.all`/`task.race`/`task.wait`/`task.timeout` return task values that the surrounding `await` consumes (the combinator itself is a synthetic task) — an implementation composition choice.

**Resolved (2026-10-01):** closed per user answer (jawaban.md, option A); evidence: `test_job015_sleep_ordering_not_creation_order`, `test_job015_timeout_error_task_keeps_running`, `test_job015_all_results_input_order`, `test_job015_race_first_finisher_wins` passing; full suite → 834 passed, 0 failed. The virtual-clock limitation stays tracked by CHECK.md's candidate-gap row.

## 65. Job 016 Ambiguity Analyses (2026-09-25)

### 65a. http.* as Task: worker thread + scheduler poll (§50 + §101 + §112) — **Resolved (2026-10-01)**
§50 says every `http.*` call yields a Task used with `await`, while §112 requires that a task waiting on an async operation must not block other runnable tasks. A blocking HTTP call executed inside `run_task_step` would freeze the whole scheduler for the duration of the request.

**Decision:** each call spawns a `std::thread` performing the blocking exchange (ureq); the task is created `running` (the request is already in flight — §106 wording) with a shared `Arc<Mutex<Option<Result<...>>>>` slot. The worker writes the slot exactly once; the scheduler polls slots on the VM thread and finishes/fails the task there, so every task-state transition stays single-threaded. The awaiter resumes through the normal `notify_dependents` path. Not spec-derived: no section states *how* the network work is scheduled (§112 explicitly leaves the mechanism to the runtime).

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 834 passed, 0 failed (2026-10-01). Evidence: mechanism implemented as decided (worker thread + shared result slot polled by the scheduler, src/vm.rs http arm); the 13 Job 016 http tests (test_job016_http_*) all passing, including test_job016_http_timeout_is_catchable_error and test_job016_http_network_failure_catchable.

### 65b. Virtual clock vs real network time (§110-§112 vs HTTP) — **Resolved (2026-10-01)**
Job 015 measures `task.sleep`/`task.timeout` on a virtual monotonic clock that jumps to the next deadline when idle (deterministic tests). HTTP exchanges complete on the *real* clock.

**Decision (hybrid):** when only virtual timers remain, the clock jumps as before; when no virtual timer remains but HTTP work is on the wire, the scheduler parks in real time (`park_timeout(1ms)` polls) **without** advancing `now_ms`. HTTP `options.timeout` is enforced inside the worker thread in real time (ureq `timeout_global`) — it never participates in virtual jumps, so a `timeout: 5000` request against a fast server cannot be "virtually" preempted. Mixing a virtual `task.sleep` with an in-flight HTTP deadline in the same program can interleave the two clocks in an order that has no real-time meaning; no spec section coordinates them (reported, untested combination).

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 834 passed, 0 failed (2026-10-01). Evidence: virtual monotonic clock (now_ms) with park-poll while HTTP is on the wire, and ureq timeout_global enforced in the worker thread (src/vm.rs); Job 015 timer tests and Job 016 http tests passing. The decision's explicitly reported, untested combination (virtual sleep mixed with an in-flight HTTP deadline) remains recorded as such — not claimed tested.

### 65c. `res.headers` read-only enforcement (§60) — **Resolved (2026-10-01)**
§60 guarantees that changing `res.headers` does not alter the response or any request. Job 016's scope note offers "independent copy" vs "heap object that is simply never written back".

**Decision:** `res.headers` is a freshly allocated independent heap object built from the parsed response at completion time. Mutation is *allowed* in the language (the object is not const/frozen) but trivially cannot affect anything network-related: nothing ever reads it back. This matches the explanatory sentence of §60 ("Mengubah res.headers tidak mengubah response atau request yang sudah terjadi") over a freeze reading. Whether the object should be const was not stated by the spec and was not guessed.

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 841 passed, 0 failed (2026-10-01; 7 direct tests added). Implementation verified: res.headers is a freshly allocated heap object built at completion time and never read back (src/vm.rs http response construction); header tests test_job016_http_methods_with_body_and_headers, test_job016_http_request_headers_sent_as_given passing. Follow-up closed 2026-10-01: direct test added — test_issue65c_headers_mutation_is_local asserts the mutation is visible only on the first response's headers (status/body untouched, next response's headers contain no leaked key) (suite 841 passed, 0 failed).

### 65d. Mechanical details the spec does not state — **Resolved (2026-10-01)**

- **HTTP client dependency:** `ureq` 3.4.2 (blocking, pure-Rust TLS via rustls) — chosen because §50's own examples use `https://` URLs (a hand-rolled client would need TLS) and because it has no async runtime, matching the single-threaded VM. Report item 2.
- **Error wording**: `http.get(): ...` prefixes, `query must be an object`, `header values must be strings`, `body must be a string`, `options must be an object`, `request timed out after N ms` — the spec only says ERROR.
- **`options` itself non-object** → ERROR (only field-level rules exist in §59/§60); **`body` non-string** → ERROR (§50.1 only ever shows a string body from `json.stringify`); **unknown option keys** → ignored (no rule); **url non-string** → ERROR; **`timeout: 0`** → accepted (0 ms, fails unless instantaneous).
- **Client-added headers**: ureq adds `user-agent` (and friends per its defaults); §60 only governs *user-provided* headers ("Header dikirim sesuai key/value yang diberikan" — asserted for given headers; client defaults are not spec-controlled either way).
- **Redirects**: followed (ureq default, max 10, error beyond) — spec silent on redirects.
- **Body limits**: response bodies read as lossy UTF-8 up to ureq's 10 MB default (`read_to_string`); larger bodies → runtime error. Lossy decoding means invalid UTF-8 bytes become U+FFFD — `res.body` is always a string per §50.
- **Un-awaited `http.get(...)`**: creates a background request task (same shape as un-awaited sleep, ISSUES §64d); not rejected.
- **Query order**: `options.query` key order follows the object literal order (props are a `Vec`), so `?page=2&active=true` from §59's example is reproduced exactly; §59 does not mandate ordering for programmatically built objects.
- **Query keys/values**: encoded with RFC 3986 unreserved set kept (`A-Za-z0-9-._~`); `null` → `key=` (empty value with `=`); `#fragment` URLs keep the fragment last.

**Decision (all ten items recorded as-is, user 2026-10-01):** HTTP client = `ureq` 3.4.2 (blocking, pure-Rust TLS via rustls, no async runtime — matches §50's `https://` examples and the single-threaded VM); error wording (`query must be an object`, `request timed out after N ms`, …) is implementation text (spec only says ERROR); `options` non-object → ERROR, `body` non-string → ERROR, unknown option keys → ignored, url non-string → ERROR, `timeout: 0` accepted; client-added headers (user-agent etc.) are not spec-controlled — §60 governs user-provided headers only; redirects are followed (ureq default, max 10, error beyond); response bodies are read as lossy UTF-8 up to ureq's 10 MB default so `res.body` is always a string per §50 (larger → runtime error); an un-awaited `http.get(...)` creates a background request task (same shape as §64d item 3, not rejected); `options.query` key order follows the object-literal order (reproducing §59's example exactly); query keys/values encoded with the RFC 3986 unreserved set kept, `null` → `key=` (empty value with `=`), `#fragment` kept last.

**Resolved (2026-10-01):** closed per user answer (jawaban.md, option A); evidence: the 13 Job 016 tests `test_job016_http_*` passing; full suite → 834 passed, 0 failed. Follow-up recorded as a CHECK.md candidate gap (user note, not now): the redirect cap (max 10) is user-facing behavior for a future StandardLibrary.md §50 spec-writing session. **Follow-up closed 2026-10-02** (user-approved spec-writing session): a "Redirect" rule block was added to StandardLibrary.md §50 — `http.*` follows 3xx-with-Location responses automatically, capped at 10 redirects, exceeding the cap → ERROR (transcribed from the implementation: ureq 3.4.2 default `max_redirects: 10`, `max_redirects_will_error: true`, error text "too many redirects"). Evidence: `test_job016_http_redirect_limit_is_error` (endless self-redirect ends in a catchable error containing "redirect"); full suite → 856 passed, 0 failed.

## 66. Job 017 Ambiguity Analyses (2026-09-26)

### 66a. ZSON grammar is never defined (§82, §84, §96) — **Resolved (2026-10-01)**
§82 calls `project.zson` "format ZSON (ZeroScript Object Notation)" and shows only one object; §84 and §96 show two more. No section defines ZSON's grammar, whether it allows comments, trailing commas, unquoted keys, or how it differs from JSON. Job 011's `json.*` parser (Job 010/011) implements strict JSON only, and AGENTS.md forbids inventing syntax beyond what the spec illustrates.

**Decision:** ZSON = the JSON value grammar **plus bare identifier keys**, reusing `json.rs` (`parse_zson` sets a `zson` flag that makes `parse_key` try an identifier before a quoted string). Bare keys match the spec's three examples (`name`, `version`, `dependencies`, `packages`, `scripts`); a hyphen cannot be a bare key (all spec examples that contain `-` — `"http-utils"`, `"2.0.1"` — are quoted), so keys stay `[A-Za-z_][A-Za-z0-9_]*` and `http-utils:` is rejected (`test_job017_manifest_unsafe_dependency_name_rejected`, `zson_bare_key_with_hyphen_is_rejected`). No comments, no trailing commas (`zson_no_comments_or_trailing_commas`) — neither appears in any spec example, and adding them would be invented syntax. Values are exactly JSON's. Readings rejected: (2) ZSON as a full superset of ZeroScript expression syntax, (3) ZSON as strict JSON with quoted keys only (would make all three spec examples invalid).

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job017_manifest_unsafe_dependency_name_rejected`, `zson_bare_key_with_hyphen_is_rejected`, `zson_no_comments_or_trailing_commas` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 66b. Version selector notation (§83) — **Resolved (2026-10-01)**
§83 documents exactly four forms — `1.2.0`, `1.2.x`, `1.x`, `x` — states that `^`, `~`, `>=` are *not* used, and says `x` is only a selector, never part of a version.

**Decision:** `Selector::parse` accepts only those four forms; anything else (bare `1`, `1.2`, `1.x.x`, uppercase `X`, `*`, `1.2.3-beta`) is rejected at manifest-parse time (`test_job017_manifest_invalid_selector_rejected_at_parse`). Leading zeros (`01.x`) are rejected as invalid numeric components. A selector is always combined with every other requirement seen for the same package, and the highest version satisfying **all** of them is chosen (`highest_matching`) — §83 "Selector selalu memilih versi terbaru yang memenuhi pola selector". Pre-release/build metadata is not in §83 and is not supported (SemVer parsing accepts `MAJOR.MINOR.PATCH` only).

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job017_manifest_invalid_selector_rejected_at_parse` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 66c. Package layout: §81's example tree contradicts §82/§85/§87/§94/§96 — **Resolved (2026-10-01)**
§81 shows `my-app/ deps/http-utils/{package.zs, src/client.zs}` with `zeroproject.zs`, and its import `from "http-utils/client"` would then mean `src/client.zs`. Every later section instead shows `my-app/{project.zson, .zsc_m/http-utils/}` (§82, §85, §87) and a package tree of `package.zson` + `client.zs` + `parser.zs` at the package root (§94, §96), with no `src/` and no `deps/`.

**Decision:** §81's tree is a superseded draft (its `zeroproject.zs`, `deps/`, `package.zs` names are contradicted by §82/§94/§96). Installed layout is `.zsc_m/<name>/` containing `package.zson` plus flat `.zs` files, and a subpath maps **literally** from the package root: `http-utils/client` → `.zsc_m/http-utils/client.zs`, `pkg/util/text` → `.zsc_m/pkg/util/text.zs`. A `src/` fallback was considered and rejected — it would contradict §94/§96's example trees and would make resolution depend on which of two files exists.

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 834 passed, 0 failed (2026-10-01). Evidence: literal subpath mapping from the package root tested by test_job017_package_nested_subpath passing; installed layout matches §94/§96 trees as the decision states.

### 66d. Bare package import (`from "pkg"`) has no specified target — **Resolved (2026-10-01)**
§81 defines "path tanpa `./` atau `../` → package import" but every package import example uses a subpath (`"http-utils/client"`); no section states what `from "pkg"` resolves to. §81's only root-level file names are the superseded `package.zs`.

**Decision:** `pkg` → `.zsc_m/pkg/index.zs`. Spec-silent, therefore implementation convention, not a spec-derived rule. The alternative (`package.zs`) follows a section whose file naming §94/§96 replace. Both forms are tested (`test_job017_package_bare_name_uses_index`).

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job017_package_bare_name_uses_index` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 66e. Flat `.zsc_m/` vs §88's tree drawing (and multi-version conflict) — **Resolved (2026-10-01)**
§88 draws `my-app → http-utils → url-parser` as a tree and ends by saying that when `package-a → shared-utils@1.2.x` and `package-b → shared-utils@2.x` cannot share a version, "resolver harus mempertahankan versi yang diperlukan oleh masing-masing dependency" — i.e. two versions of one package must be reachable at the same time, which a single flat copy cannot express. §82/§85/§87's directory examples are flat (`.zsc_m/http-utils/`, `.zsc_m/database/`), and §90 defines conflict resolution as a separate section (Job 018).

**Decision:** the §88 tree is the *dependency graph*, not directory nesting; storage is flat, one copy per package name, shared by everyone whose selectors accept it (§88 "menghindari instalasi duplikat ketika versi yang sama dapat digunakan bersama"). Every requirement for a package is intersected; if the intersection is empty → ERROR (`test_job017_unresolvable_version_conflict_is_error`), which satisfies §88's "jika tidak terdapat versi yang memenuhi seluruh kebutuhan → ERROR". Carrying two conflicting versions at once (the last §88 example) requires per-requirement isolation/nested `.zsc_m` and is deferred to Job 018 (§90) — reported as a known limitation, not silently simplified. *(Updated 2026-09-29: Job 018 §90 implemented the isolation — see §67a. `test_job017_unresolvable_version_conflict_is_error` was superseded by `test_job018_disjoint_selectors_resolve_to_two_versions`; the ERROR path is retained only for selectors with no matching version at all.)*

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job018_disjoint_selectors_resolve_to_two_versions` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01). The cited test_job017_unresolvable_version_conflict_is_error was superseded by test_job018_disjoint_selectors_resolve_to_two_versions (src/package.rs:3398, passing) — Job 018 §90 isolation is implemented exactly as the decision's update note states; the ERROR path for selectors with no matching version remains (Job 017 tests all passing).

### 66f. §89 defines no dev/runtime categories — **Resolved (2026-10-01)**
§89's rules are only: the project sees its own `dependencies`; a package sees its own `package.zson` dependencies; a transitive package is not importable by the project unless declared directly; scope does not affect module scope inside the package. There is no `devDependencies`, no `optional`, no peer concept anywhere in §81-§89.

**Decision:** implemented exactly those two visible sets — `Loader::resolve_package` resolves the *owner* of the importing file (project if outside `.zsc_m/`, otherwise the package name from the path) and checks membership of the requested package in that owner's declared dependencies (`test_job017_project_cannot_import_transitive_dependency`, `test_job017_package_imports_its_own_dependency`, `test_job017_package_cannot_import_undeclared_package`, `test_job017_installed_transitive_package_is_not_global`). No scope category was invented.

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job017_installed_transitive_package_is_not_global`, `test_job017_package_cannot_import_undeclared_package`, `test_job017_package_imports_its_own_dependency`, `test_job017_project_cannot_import_transitive_dependency` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 66g. What the lockfile must contain (§84 vs §88) — **Resolved (2026-10-01)**
§84's example lockfile lists only the project's two direct dependencies; §88 requires "dependency tree harus dapat direproduksi menggunakan project.lock.zson"; §87 says the resolver uses the locked versions.

**Decision:** the lockfile records **every** resolved package (direct and transitive) in resolution order, still in §84's exact shape `{ packages: { "n": "v" } }`. That satisfies §88 reproducibility and is backward-compatible with §84's example (which is exactly what a project with no transitive deps produces). Keys are emitted in breadth-first resolution order (manifest declaration order, then each package's own declaration order) — §87 requires deterministic resolution but never states a traversal; breadth-first was chosen for stability (`test_job017_nested_dependency_tree` pins the order).

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job017_nested_dependency_tree` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 66h. Invalid lockfile handling (§84 silent) — **Resolved (2026-10-01)**
§84 only says "Jika lockfile tersedia **dan masih valid**, package manager menggunakan versi yang tercatat" and "Jika lockfile dihapus, resolver dapat memilih versi terbaru".

**Decision:** a lockfile that fails to parse (or whose `packages` member is missing/malformed) is treated as **absent**: `install` re-resolves and rewrites it (`test_job017_invalid_lockfile_is_ignored_not_fatal`); `Project::open` stores `lock: None`. A *structurally valid* lockfile is consulted entry-by-entry: an entry whose selector no longer matches the project's selector is re-resolved and the file rewritten (`test_job017_stale_lockfile_entry_is_re_resolved`), while a matching entry pins the version (`test_job017_install_honours_lockfile`). Reading a corrupt lock as a hard error was rejected — §84 frames the lockfile as an optimization over `project.zson`, which remains authoritative.

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job017_install_honours_lockfile`, `test_job017_invalid_lockfile_is_ignored_not_fatal`, `test_job017_stale_lockfile_entry_is_re_resolved` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 66i. §87 verification when installed metadata is missing or corrupt — **Resolved (2026-10-01)**
§87 requires the installed package to match `project.lock.zson`; §96 requires `package.zson` to be valid ZSON. If the file is absent or unparseable, §87's check cannot be performed at all.

**Decision:** while loading, if `.zsc_m/<pkg>/package.zson` exists it is always parsed (corrupt → ERROR, `test_job017_broken_installed_metadata_is_error`); if a lock entry exists but the file is missing → ERROR `Installed version of 'pkg' is unknown (package.zson missing) but project.lock.zson locks X` (`test_job017_missing_installed_metadata_with_lock_is_error`) — silently skipping would let a wrong version run unnoticed. With **no** lock entry there is nothing to compare against, so a package without metadata still loads (`test_job017_package_without_metadata_runs_when_not_locked`); §96's "package.zson harus ada" is a publishing/registry rule (§94), enforced there by Job 018. The package's *name* field is not compared against the directory name — no rule states that it must match, and the lock is keyed by directory name.

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job017_broken_installed_metadata_is_error`, `test_job017_missing_installed_metadata_with_lock_is_error`, `test_job017_package_without_metadata_runs_when_not_locked` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 66j. Package source for `zpm install` (§85 "download package") — **Resolved (2026-10-01)**
§85's flow says "download package"; §95's registry is a Git-backed index of Git URLs; §93's package source is Git. Job 017 explicitly allows a local, deterministic source.

**Decision:** local filesystem registry laid out exactly like §95's example tree, `<registry>/<package>/<version>/` each holding a valid package (`Registry::versions` / `package_dir`); "download" is a directory copy into `.zsc_m/<name>/`. Source lookup order — `--registry <dir>` flag, then `$ZPM_REGISTRY`, then `<project>/registry` — is **mechanical**: no section states where a local source lives. A package absent from the registry → ERROR (`Package 'x' not found in registry ...`), no matching version → ERROR (`No version of 'x' matches selector '...'`), per §85. Not-installed-and-same-version is not re-copied (`test_job017_installed_package_not_downloaded_again`).

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job017_installed_package_not_downloaded_again` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 66k. `zpm run` argument forwarding mechanism (§86) — **Resolved (2026-10-01)**
§86 says "`zpm` tidak mengubah isi command yang ditulis programmer", that the command runs from the project root, and that "Command tambahan diteruskan kepada command script" for `zpm run start -- --debug`.

**Decision:** the command text is executed verbatim (`sh -c '<command>'`, `current_dir` = project root) and extra arguments are appended as **shell-quoted words** after it — the only reading under which they reach the script's argv (`printf '%s\n' > out.txt --debug 'x y'`, since redirections are position-independent, `test_job017_run_script_args_after_redirection_still_reach_command`). Positional parameters are *not* provided, so `$@` inside a script command expands to nothing (a command written to consume `$@` would double its arguments otherwise — that combination was tried and failed). Consequences: a command ending in `;` or `&&` would receive the arguments as a separate command; a command already ending in a pipe gets them as the last command's argv. Neither case appears in §86 and neither is rejected. Exit code of the child is `zpm`'s exit code (`test_job017_run_script_nonzero_exit_code_propagates`).

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job017_run_script_args_after_redirection_still_reach_command`, `test_job017_run_script_nonzero_exit_code_propagates` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 66l. Mechanical details the spec does not state — **Resolved (2026-10-01)**

- **ZSON parse errors / file names in messages.** `project.zson: Expected ',' or '}' at position N, found 'x'` — §82 only says the file must be ZSON.
- **Error wording throughout**: `Package 'x' is not a dependency of the project|package 'y'`, `Package 'x' is not installed — run \`zpm install\``, `Script not found: name`, `Installed version of ... but project.lock.zson locks ... — run \`zpm install\``, `Package cannot import files outside its boundary: ...`, `No version of 'x' matches selector '...'`, `Circular package dependency detected: a -> b`. The spec only requires ERROR; all messages are prefixed `ERROR: `.
- **Exit codes.** `zpm` exits 1 on any error, 0 on success, and passes the script child's code through for `zpm run`; `zsc` exits 1 on any load/run error. No section defines exit codes.
- **`zsc` / `zpm` binary names.** §86's examples invoke `zsc` and `zerovm`, so the crate's default binary was split into `zsc` (src/main.rs) and `zpm` (src/bin/zpm.rs) with `autobins = false` (Cargo.toml). `zerovm` is not a separate binary — running a `.zs` file *is* the VM (`zsc`); no section defines binary names beyond §86's usage examples.
- **Project discovery.** §87 says "Package resolution tidak bergantung pada current working directory". `zsc` discovers `project.zson` by walking up from the *entry file*; `zpm` walks up from its *cwd* (it has no entry file). Both are implementation choices; §87 constrains resolution, not the CLI's starting point.
- **`project.zson` `version` is not SemVer-validated** (§82 requires only "string"), while `package.zson` `version` must be valid SemVer (§96 states it explicitly).
- **`scripts` keys/values validation** (§86: object, string keys, string command) is done at manifest parse time; unknown manifest fields are ignored (§96 allows future fields for packages; §82 is silent for projects).

**Decision (all seven items recorded as-is, user 2026-10-01):** ZSON parse errors carry `project.zson: Expected … at position N` detail (§82 only requires ZSON); all package error messages use the `ERROR: ` prefix (spec only requires ERROR); CLI exit codes are 1 on error / 0 on success / `zpm run` passes the script child's code through (no section defines exit codes); binaries are `zsc` (src/main.rs) and `zpm` (src/bin/zpm.rs) per §86's examples — `zerovm` is not a separate binary (running a `.zs` file *is* the VM); `zsc` discovers `project.zson` by walking up from the entry file while `zpm` walks up from its cwd (§87 constrains resolution, not the CLI's starting point); `project.zson` `version` stays a plain string per §82 while `package.zson` `version` must be valid SemVer per §96; `scripts` is validated at manifest parse time and unknown manifest fields are ignored.

**Resolved (2026-10-01):** closed per user answer (jawaban.md, option A); evidence: Job 017 tests (`src/package.rs` test_job017_*, `src/module.rs` test_job017_*, `tests/zero017_zpm_cli.rs`) passing; full suite → 834 passed, 0 failed. Follow-ups recorded as CHECK.md candidate gaps (user note, not now): exit codes + binary names (items 3/4) and SemVer validation for the project version (item 6) as future spec-writing candidates. **Follow-ups closed 2026-10-02** (user-approved spec-writing session): Dependencies.md gained three spec blocks transcribed from the implementation — §82 "version project is string-only, no SemVer validation" (unlike package versions, §83/§96); §86 "`zpm run` exits with the script process's exit code (signal-killed child → -1, reported as 255)"; §100 "Nama Binary dan Kode Keluar" (`zsc` runs the compiler/VM, `zpm` the package manager; 0 on success, 1 on failure; `zpm run` forwards the script's code). Evidence: `zsc_missing_file_exits_1`, `zsc_version_exits_0`, `zsc_successful_program_exits_0`, `zpm_unknown_command_exits_1` (tests/cli_exit_codes.rs), `cli_run_forwards_child_exit_code`, exact `Some(1)` assertion in `cli_install_named_failure_leaves_manifest_untouched`, plus pre-existing `test_job017_run_script_nonzero_exit_code_propagates`; full suite → 856 passed, 0 failed.

## 67. Job 018 Ambiguity Analyses (2026-09-29)

### 67a. §90 never says how conflict groups are formed — **Resolved (2026-10-01)**
§90's rules: every dependency gets "versi yang paling baru yang sesuai"; a single version is shared when possible; when a version cannot satisfy everything, the resolver must keep each required version "di area instalasi yang terisolasi"; and each *scope* must get one consistent version. No section defines what a "scope" is, how grouping is computed, or what the isolated area looks like on disk.

**Decision:** a group is identified by the set of requirement origins — for selector requirements that is the **importing scope** (project, or the package declaring it); an explicit `git:` requirement forms its own group and is never merged with selectors (a `git:` demand is exact, §93/§99). All selector requirements with an identical origin set share one version (highest matching every selector **and** a still-valid lock entry — no auto-upgrade, §87/§92); disjoint origin sets resolve independently (`place`, src/package.rs:1789). The isolated area is `.zsc_m/<name>/versions/<v>/` **only when a name keeps more than one version**; a single version keeps §82/§85's flat `.zsc_m/<name>/` layout (both readings fit §82/§85's flat example and §90's isolation rule; a permanent `versions/` nesting was rejected because it would contradict §82/§85's literal directory trees for the common one-version case). Lock keys become `name@version` for multi-version entries (§84's shape is a single-version shorthand — plain `"n": "v"` still emitted when only one version exists, so every Job 017 lockfile test keeps its exact bytes). Scope graph cycles are detected *after* convergence (`check_scope_cycles`) — §90 is silent on cycle detection and §88's cycle ERROR must still hold. Evidence: `job018_one_version_shared_by_several_needs`, `job018_disjoint_selectors_resolve_to_two_versions`.

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `job018_one_version_shared_by_several_needs` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01). Evidence: job018_one_version_shared_by_several_needs (tests/zero018_package.rs) and test_job018_disjoint_selectors_resolve_to_two_versions (src/package.rs — the decision's citation omitted the test_ prefix) passing; place (src/package.rs:1789) and check_scope_cycles (src/package.rs:2010) present as cited.

### 67b. §91 removal: what "tidak digunakan lagi" can be determined from, and how the manifest is edited — **Resolved (2026-10-01)**
§91 says `zpm remove <pkg>` stops declaring the dependency, removes the package only if no other dependency still needs it, and rewrites `project.lock.zson`. It does not say what happens when the package is not declared, nor how `project.zson` is edited textually.

**Decision:** removal is *undeclare + re-resolve + prune* (`remove_dependency`, src/package.rs:2483): the declaration is deleted, resolution is recomputed from the remaining manifest (so "still used by someone else" falls out of the resolution itself rather than a bespoke graph query — `prune`:2375 removes only packages absent from the new resolution), and the lockfile is rewritten from the new resolution. Removing an undeclared package → ERROR (`undeclare_dependency`:2427) — §91's flow presupposes a declared dependency; erroring is the conservative reading. The manifest is edited by parsing + `json::render_zson` + rewrite: comments and original formatting are **not** preserved (ZSON, §66a, defines no comments; a line-preserving editor was rejected as over-engineering for a format with no formatting rules).

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 834 passed, 0 failed (2026-10-01). Evidence: job018_remove_prunes_only_unneeded_packages and job018_remove_undeclared_package_is_error (tests/zero018_package.rs) passing.

### 67c. §92 update: what gets updated and with what arguments — **Resolved (2026-10-01)**
§92: `zpm update` brings direct **and transitive** dependencies to the newest version matching their selector, without touching project code; `zpm install` must not silently upgrade a locked version. The spec shows no arguments.

**Decision:** `zpm update` updates *all* packages (§92 names no subset, and "jika versi baru sudah sesuai selector" is universal); an extra argument is rejected as `ERROR: usage: zpm update` (mechanical — no partial-update syntax exists in the spec, and inventing `update <pkg>` would be a new syntax). Implementation = the normal resolution pipeline with the lockfile pinning disabled (`InstallPool::for_update`), so transitive packages move too (§92 "bahkan dependency transitive"). A selector that no longer has any matching version → ERROR (§83's no-match rule applies during update as well). `zpm install` never bumps a locked version (`job018_update_bumps_locked_version_install_does_not`).

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `job018_update_bumps_locked_version_install_does_not` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 67d. §93/§97 Git packages: double clone, probe, and lock identity — **Resolved (2026-10-01)**
§93 requires Git packages to be installed from a Git source and §97's lock example records `source`, `commit`, `hash`. §92/§87 require a locked commit to be honoured on reinstall. Nothing says how the commit of an unconstrained (`#main`) dependency is learned before installation, or how identity is obtained for a Git package that is already locked.

**Decision:** resolution needs the package *version* before content exists (the version selects which requirement applies), so `probe_git` (src/package.rs:2645) does a shallow clone into a staging dir, reads `package.zson`, and discards it; `acquire` then clones again for real (a shared clone-cache would avoid the second clone but §93 defines no cache for probes — the double clone is the simple architecture). A locked commit short-circuits: `git::checkout_commit` (src/git.rs:153) pins the exact commit first, so reinstall is reproducible without re-probing the ref. Checkout uses `--end-of-options` before the ref (src/git.rs:95) — both option-injection safety and refs starting with `-`. Git packages record `source` + `commit` + a computed `hash` (§97 example shape); no test relies on the clone itself being deduplicated.

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 834 passed, 0 failed (2026-10-01). Evidence: probe_git (src/package.rs:2645), git::checkout_commit (src/git.rs:153), and --end-of-options checkout (src/git.rs) present exactly as cited; job018_git_source_unreachable_is_error and job018_explicit_git_source_wins_over_registry (tests/zero018_package.rs) passing.

### 67e. §94 publish: registry, Git tag, and "distribution channel" — **Resolved (2026-10-01)**
§94 requires the version to be uploaded to the registry and to appear in Git's release history, forbids overwriting an existing version, and requires `package.zson`. It does not say where the registry comes from when `--registry` is absent, nor what the Git release marker is.

**Decision:** the registry is **explicit** — `--registry` flags, then `$ZPM_REGISTRY`, then ERROR `Publish` (mechanical: §94 presupposes a known registry and Job 017 established these two configuration sources; silently defaulting to `<project>/registry` would publish into the consuming project's own source tree, which §94's read-side model does not describe). The Git marker is the annotated-style lightweight tag `v<version>` (`git::create_tag`) — `v`-prefix is the conventional reading of "muncul di riwayat rilis Git" and no tag format is specified; a tag already present → ERROR (release history is final, mirroring §94's version-finality rule). Ordering: registry copy **then** tag, so a repo already tagged at that version errors on first publish (the tag is treated as evidence of a prior release — spec-silent, conservative direction). Copy destination is a **local directory registry** (§95's minimum local implementation — see §66j/§67f); §94 does not mandate an HTTP transport and no test performs network I/O. Second publish of the same version → ERROR `already published` (`job018_publish_git_repo_marks_release_tag_and_is_final`).

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `job018_publish_git_repo_marks_release_tag_and_is_final` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 67f. §95 index format, and where candidates come from — **Resolved (2026-10-01)**
§95's index example maps `version → source` (Git URL) and shows an optional integrity hash, but never defines: whether `source` may be a plain path, what the hash syntax is, what happens on a malformed index, or whether the *cache* can satisfy resolution when a registry is unreachable.

**Decision:** `index.zson` is `{ name?, versions: { "<version>": "<source>" | { source, hash? } } }`; a non-`git:`/non-URL source is resolved **relative to `<registry>/<name>/`** (mechanical — §95's example stores sources inside the registry tree); `hash` must be `sha256:<64 hex>` (§97's hash shape) or the index is rejected. A malformed index (bad JSON, missing `versions`, bad version key, bad hash) → ERROR with the file path in the message — §95 gives no partial-read rule and silently skipping a broken index would silently change resolution. **Candidates come only from registry listings** (§95 "Registry menyediakan daftar kandidat"): the cache stores *content* (§98), never candidate versions, so a deleted registry breaks resolution even when every package is cached — tested deliberately (`job018_valid_cache_is_used_when_registry_copy_is_gone` keeps the registry present for resolution and only guts the package copy; `job018_corrupt_cache_is_not_used` verifies a digest-mismatched cache entry is re-fetched, not used). Multiple registries are consulted in configuration order (flags → `$ZPM_REGISTRY` → `<project>/registry`); "official registry" (§95) has no specified identification mechanism and is not named specially.

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `job018_corrupt_cache_is_not_used`, `job018_valid_cache_is_used_when_registry_copy_is_gone` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 67g. §96/§94 metadata consumption: which name/version must match what — **Resolved (2026-10-01)**
§96 requires `package.zson` to exist with valid `name` + SemVer `version` and allows future fields; §94 requires publishing to validate it. Job 017's loader check (§66i) deliberately did *not* compare the stored `name` against the directory name because no rule stated it.

**Decision:** with §94/§95 flows in place, the comparison happens where a *requirement* exists: before using any registry copy or Git checkout, `check_meta` (src/package.rs:1225) verifies that the stored `package.zson` declares exactly the `(name, version)` being installed — mismatch → ERROR `Invalid package metadata for '<name>': package.zson declares ...` (a registry that stores package X under Y's directory, or a Git repo whose manifest disagrees with the dependency name, must not be trusted — mechanical enforcement of §96's validity rule at the point §94/§95 make it observable). Optional `description`/`author`/`license`/`repository`, when present, must be strings (§96 allows "field tambahan untuk masa depan" — the four appear in no spec example, so they are only type-checked, never read). Unknown fields are ignored (Job 017's §66l rule, unchanged).

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 834 passed, 0 failed (2026-10-01). Evidence: check_meta (src/package.rs:1225) verifies (name, version) before install with mismatch -> ERROR; test_job017_metadata_mismatch_is_error and job018_optional_metadata_fields_are_accepted passing.

### 67h. §97 when a hash may be recorded (and when one must be checked) — **Resolved (2026-10-01)**
§97: a recorded hash that differs from the computed one → ERROR, and the lockfile example carries `hash`. It does not say whether a hash must be *created* on first install, nor what happens when the source never provided one.

**Decision:** a hash is recorded **only when the source provided one** — `lock_identity` (src/package.rs:2196) takes the lockfile's existing hash, else the registry index's hash, else (Git only) the digest computed at clone time (§97's example is a Git package). A plain registry install whose index has no hash keeps §84's plain `"name": "version"` form; **no fresh digest is computed for it**, because (a) §97 only defines *comparison* against a provided hash, (b) computing one on first install and comparing later would make an unchanged local registry "fail" the moment any file's mtime-like content differs in a way the spec never discusses, and (c) nothing in §84-§96 requires local copies to be hashed. Verification always runs when a hash exists (lock first — "Hash yang berbeda → ERROR" — else index), before anything reaches `.zsc_m/` (`job018_index_hash_is_verified_and_recorded`, `job018_index_hash_mismatch_is_error`). A hand-written lockfile with a `commit` but no `hash` for a Git package re-clones and re-pins the commit (the commit, not a digest, is §92's reproducibility anchor).

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `job018_index_hash_is_verified_and_recorded`, `job018_index_hash_mismatch_is_error` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 67i. §98 cache: validity, roots, and what "verified" means — **Resolved (2026-10-01)**
§98: verified content is stored in `.zsc_cache/`, invalid cache content must never be used, and clearing the cache must not affect the project. It defines no cache root for a project without one, no sidecar format, and no staging rules.

**Decision:** cache root precedence — `$ZPM_CACHE`, else `<project>/.zsc_cache`, else `$HOME/.zsc_cache` (mechanical; per-project first keeps tests and concurrent projects isolated). Layout `<name>/<version>/` + a `<version>.sha256` sidecar recording `hash::hash_dir` of the stored copy; `Cache::entry` re-hashes the stored tree and refuses a mismatch (§98's "cache tidak valid tidak boleh digunakan"), and a stored copy whose `package.zson` disagrees with the requested `(name, version)` is likewise refused (§97/§96 identity — a corrupted cache must not resurrect a wrong package). Staging directories (probe clones, install staging) live outside the cache and are removed — §98 defines no partial-entry rule. Store happens **after** verification, for both registry copies and Git checkouts (previously Git-only; registry copies now cache too, src/package.rs:2149). Clearing the cache changes no manifest/lock/`.zsc_m` (§98's "tidak menghapus instalasi") — `job018_clearing_cache_leaves_project_untouched`.

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `job018_clearing_cache_leaves_project_untouched` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 67j. §99 source priority: order, and whether priority beats the selector — **Resolved (2026-10-01)**
§99: use a valid `git:` source first, fall back to cache, then registry; the source must not change the selected version. It does not say where the lockfile sits in that order.

**Decision:** the effective order is **lockfile → explicit `git:` → cache → registries in configuration order**: the lockfile is consulted first as *version* selection (§87/§92 — it is not a "source"), then within the selected version the source choice follows §99 literally. An explicit `git:` dependency never consults a registry (§99's first clause, tested by `job018_explicit_git_source_wins_over_registry` where registry 2.5.0 loses to `git:` 1.0.0). Priority never overrides the selector: the version is chosen from candidates first, the source only supplies content for that version. Unreachable git source with no valid alternative → ERROR (§99's fallback applies to *invalid/absent* states it can detect; a declared-but-unreachable source is the dependency's identity, not a cache miss — `job018_git_source_unreachable_is_error`).

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `job018_explicit_git_source_wins_over_registry`, `job018_git_source_unreachable_is_error` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 67k. Mechanical details the spec does not state — **Resolved (2026-10-01)**

- **Report/CLI output.** `+ <name>@<version>` per installed package and `Installed N package(s)` — §85 defines no output; Job 017's format kept (`+ name@version` also works with multi-version keys' plain name portion).
- **`zpm install <name>` / `git:<url>` declare-then-install.** §100 requires "setiap dependency yang dipakai harus dideklarasikan di `project.zson`", so the command writes the declaration *first* (`add_dependency`, src/package.rs:2497) and then installs. Consequence (spec-silent): if installation subsequently fails, the declaration **stays** in the manifest while the lockfile is untouched — the user can fix the registry and re-run `zpm install`. Writing manifest+install atomically was rejected: there is no transaction rule in §82/§85, and undoing a manifest edit on failure would be invented behavior.
- **Error wording** for new paths (`No version of 'x' matches selector 'y'`, `Package 'x' not found in any registry`, `Invalid package metadata for ...`, `git clone failed: ...`, `Integrity check failed ...`, `Package 'x' is not a direct dependency of the project`, `already published`, `No package.zson found`, `ERROR: usage: zpm update`): §90-§100 only require ERROR; all are `ERROR: `-prefixed, consistent with §66l.
- **Exit codes**: unchanged Job 017 convention (§66l) — 1 on error, 0 on success, `zpm run` passes the child's code through.
- **`update` arg check ordering**: the usage check runs *before* project discovery, so `zpm update x` reports usage even outside a project (mechanical; §92 shows `zpm update` bare).
- **No network in tests**: every Git test uses a local `file://`-free path clone (git clone of a local path), every registry is a temp directory; `ureq` is never invoked (Job 016's rule, still in force).
- **Supersession**: `test_job017_unresolvable_version_conflict_is_error` → `test_job018_disjoint_selectors_resolve_to_two_versions` (Job 017's expectation that disjoint selectors are an error was a §89-era reading; §90 supersedes it — see the §66e update). All other Job 017 tests pass unchanged.

**Decision (all seven items recorded as-is, user 2026-10-01):** report output `+ <name>@<version>` and `Installed N package(s)` kept (§85 defines no output); `zpm install <name>`/`git:<url>` writes the declaration *first* (`add_dependency`, src/package.rs:2497) and then installs — if installation fails, the declaration **stays** in the manifest while the lockfile is untouched (no transaction rule exists in §82/§85; a rollback would be invented behavior — the user can fix the source and re-run); error wording for the §90-§100 paths (spec only ERROR, `ERROR: ` prefix); exit codes unchanged from the §66l convention; the `update` usage check runs before project discovery; tests run with no network (local-path git clones and temp-directory registries; Job 016's rule still in force); `test_job017_unresolvable_version_conflict_is_error` is superseded by `test_job018_disjoint_selectors_resolve_to_two_versions` (the §66e update).

**Resolved (2026-10-01):** closed per user answer (jawaban.md, option A); evidence: Job 018 tests (`tests/zero018_package.rs`, `tests/zero018_zpm_cli.rs`, `src/package.rs` test_job018_*) passing; full suite → 834 passed, 0 failed. Follow-up recorded as a CHECK.md candidate gap (user note, not now): manifest rollback when `zpm install <name>` fails (item 2) — the highest-priority behavior candidate because a leftover declaration can surprise users. **Follow-up closed 2026-10-02** (user-approved patch, no Job 020): prepare-then-commit implemented — `stage_dependency` builds the post-declaration manifest text in memory (`ManifestStage::Staged`), `install_with_declared` runs resolve+materialize against a pending seed injected into `resolve_graph` and writes `project.zson` only after the full pipeline succeeds; `zpm install <name>` (registry and `git:` paths) now goes through it via `cmd_install`, and `add_dependency` was refactored onto the same stage helper (standalone API kept, no behavior change). Any failure at any stage leaves the manifest byte-identical; an already-present declaration behaves like a plain install (§100 no-rewrite rule preserved). Tests: `job018_install_declared_resolve_failure_keeps_manifest`, `job018_install_declared_materialize_failure_keeps_manifest`, `job018_install_declared_success_commits_manifest`, `cli_install_named_failure_leaves_manifest_untouched` (success paths already covered by `cli_publish_then_install_by_name` and the git install test); full suite → 850 passed, 0 failed.

## 68. Job 019 Ambiguity Analyses (2026-09-29)

### 68a. §144 vs §145: are LIST_ADD / LIST_REMOVE / OBJECT_REMOVE opcodes or native operations? — **Resolved (2026-10-01)**
§144 lists `LIST_ADD`, `LIST_REMOVE` and `OBJECT_REMOVE` among the collection instructions, while §145 says the core opcode set must stay small and that standard-library features grow through `CALL_NATIVE` instead of new opcodes. Readings: **(1)** they are core opcodes, so every `list.add(...)` compiles to one — contradicts §145's "kecil" for a function-per-opcode treadmill; **(2)** they are only native operations and §144's names are conceptual — leaves §144 with no bytecode representation at all; **(3)** both: a dedicated opcode for the statically known call, sharing one implementation with the native operation for the dynamic case.

**Decision:** (3). `Opcode::ListAdd(argc)` / `ListRemove(argc)` / `ObjectRemove(argc)` (compiler.rs:120-124) are emitted by `Compiler::static_call_target` only when the callee is an unshadowed `list`/`object` root with a registered method (compiler.rs:533, 1459-1462); every other shape (namespace kept as a value, a local/parameter named `list`, a derived callee) still compiles to `BuiltIn` + `GetProperty` + `Call` and reaches the *same* helpers through `call_native` (vm.rs:2686-2688). The three opcodes therefore name three concrete operations (§144) without one opcode per stdlib function (§145), and there is a single implementation each — `native_list_add`, `native_list_remove`, `native_object_remove` (vm.rs:3045, 1803-1827) — reached from both paths. `MAKE_LIST` / `MAKE_OBJECT` remain plain core opcodes (§144, unchanged). Tests: `test_job019_collection_instructions_emitted`, `test_job019_collection_instructions_execute`, `test_job019_make_list_and_make_object_stay_core_opcodes`.

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job019_collection_instructions_emitted`, `test_job019_collection_instructions_execute`, `test_job019_make_list_and_make_object_stay_core_opcodes` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 68b. §141 `LOAD_FUNCTION`: standalone opcode while `Constant::Function` stays in the pool — **Resolved (2026-10-01)**
Readings: **(1)** `LOAD_FUNCTION` replaces the constant entry entirely; **(2)** keep pushing `LoadConst(Function(idx))` (the pre-Job-019 status quo recorded as a CHECK.md gap); **(3)** `Opcode::LoadFunction(idx)` is the instruction that pushes a function value, and `Constant::Function(idx)` remains in the pool as §126 metadata.

**Decision:** (3). §141's example shows `LOAD_FUNCTION <index>` as an instruction, so the value push must not go through `LoadConst` any more; but §126 still lists `Function` among the constant kinds, and two index-based consumers depend on the pool entry existing: parameter-default compilation takes `default_idx = constants.len() - 1` (compiler.rs:711), and `merge_modules` remaps `Constant::Function(idx + func_offset)` (compiler.rs:1830). Emission sites: declaration (compiler.rs:827), function expression (compiler.rs:1611), named reference (compiler.rs:1731); module merge remaps the opcode too (compiler.rs:1874). Tests: `test_job019_load_function_for_function_reference`, `test_job019_load_function_inside_function_body`, `test_job019_load_function_executes`, `test_job019_load_function_recursive_still_works`, `test_job019_merge_modules_remaps_load_function_and_sets_header`.

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job019_load_function_executes`, `test_job019_load_function_for_function_reference`, `test_job019_load_function_inside_function_body`, `test_job019_load_function_recursive_still_works`, `test_job019_merge_modules_remaps_load_function_and_sets_header` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 68c. When may the compiler emit `CALL_NATIVE` instead of the dynamic path? — **Resolved (2026-10-01)**
Bytecode.md defines no name-resolution rule for native operations: it never says what happens when a user binding shadows a built-in (`function f(print) {...}`), when a namespace root itself becomes a value (`let f = string.upper`), or when a method name is not registered (`string.bogus("x")`). A naive reading — "every built-in-looking call is CALL_NATIVE" — changes all three.

**Decision (mechanical; no language rule invented):** `CALL_NATIVE` is emitted only when `Compiler::static_call_target` proves the callee is a bare identifier that (a) declares nothing in any visible scope (`is_declared` walks the whole scope stack) and (b) is registered in `src/native.rs` (`is_native_operation`). Everything else keeps the pre-Job-019 dynamic sequence. This preserves observable behavior exactly — the existing error texts `Property 'bogus' not found`, `Unknown built-in 'x'`, and `Cannot call ...` for a shadowing parameter are byte-identical (tests `test_job019_unknown_method_error_message_unchanged`, `test_job019_shadowed_builtin_parameter_is_used_as_local`, `test_job019_dynamic_builtin_reference_still_works`, `test_job019_unknown_native_operation_is_runtime_error`) — while §146's single native-call boundary is what actually executes every built-in (`call_native`, vm.rs:2527, dispatches both the opcode and the dynamic path).

**Resolved (2026-10-01):** verified — decision implemented as written; cited tests: `test_job019_dynamic_builtin_reference_still_works`, `test_job019_shadowed_builtin_parameter_is_used_as_local`, `test_job019_unknown_method_error_message_unchanged`, `test_job019_unknown_native_operation_is_runtime_error` (located in source and passing; full suite cargo test -> 834 passed, 0 failed, 2026-10-01).

### 68d. Bytecode version value, magic, and header layout (§124, §125 silent) — **Resolved (2026-10-01)**
§124/§125 require an explicit version identifier checked before execution, and say it must differ from the language version, but they state no magic value, no field order, no numeric version, and no list of accepted versions.

**Decision (spec-silent mechanical detail, reported as such):** header = `BytecodeHeader { magic: "ZERO", version: 1 }` with `BYTECODE_VERSION: u32 = 1` and `SUPPORTED_BYTECODE_VERSIONS: &[u32] = &[1]` (compiler.rs:6-35). Written by `compile()` and `merge_modules()`; checked by `validate_header` (vm.rs:663) as the first validation stage: bad magic → `VmError::InvalidHeader`, unsupported version → `VmError::UnsupportedBytecodeVersion` (§125's "VM Error"). The header lives on `BytecodeProgram` in memory only — the spec defines no bytecode file format, so nothing serializes it; bumping `BYTECODE_VERSION` alone changes nothing observable because `SUPPORTED_BYTECODE_VERSIONS` is what the VM accepts.

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 834 passed, 0 failed (2026-10-01). Evidence: test_job019_header_and_version_written_by_compiler (header + version emitted by the compiler) passing; the chosen version/magic as stated in the decision.

### 68e. Validation strictness beyond §124's list — **Resolved (2026-10-01)**
§124 requires recognized opcodes, well-formed operands, in-range constant references, valid jump targets, valid function metadata, a valid entry point, and structure within bounds — but not: how a *foreign* unknown opcode must be reported, what "well-formed operand" covers, or whether a function body's copy appended to the main stream is validated separately.

**Decision:** (a) `Opcode::Unknown(u64)` is a representable-but-unemittable tag so a foreign/modified program yields `InvalidOpcode` instead of a panic (vm.rs:1074); (b) empty variable names, empty native-operation names and empty object keys count as malformed operands (`InvalidOperand`), as do function-index-out-of-range and `functions.len() != function_bodies.len()` / `params.len() != param_defaults.len()`; (c) every body is validated on its own instruction list *and* implicitly inside the main stream where it is appended, and its entry must satisfy `entry_point + body.len() <= instructions.len()` (§143 pairing); (d) constant indices inside bodies, parameter-default indices, and jump targets inside bodies are checked (`InvalidConstantIndex`, `InvalidJumpTarget`). All of it runs in `ZeroVM::validate()` (vm.rs:603), called as the first statement of `run()` (vm.rs:772) and `run_modules()` (vm.rs:824), so a failing program executes nothing (`test_job019_*_rejected_before_execution` all assert an empty output).

**Resolved (2026-10-01):** verified — implementation checked against source and matches the decision; full suite cargo test -> 834 passed, 0 failed (2026-10-01). Evidence: the cited test_job019_* validation suite is present (test_job019_header_and_version_written_by_compiler, test_job019_collection_instructions_emitted, test_job019_unknown_method_keeps_dynamic_call, and related validation tests in src/compiler.rs / src/vm.rs) and passing.

### 68f. Observations recorded but not changed — **Resolved (2026-10-01)**

- **Unreachable trailing `return null`.** The compiler appends its implicit return (compiler.rs:772-775) even when the body already ends in an explicit `return`, so bodies carry a dead `LoadConst(Null); Return`. Unobservable at runtime (the explicit `Return` exits first) and §142 says nothing about it — left alone in this housekeeping job.
- **Merged module program under plain `run()`.** `compile()` terminates the main stream with `Halt` before function bodies are appended (compiler.rs:605); `merge_modules` does not — module streams end in `EndModule`, which is a no-op outside `run_modules` (vm.rs:1069). Executing a merged module program through `ZeroVM::run()` therefore falls through into appended bodies. No product path does this (`zsc`/`zpm` use `run_modules`; single files use `compile()` + `Halt`), so it is recorded as an observation, covered by `test_job019_module_programs_get_header_and_validate` which exercises the real `run_modules` path.

**Decision (both observations accepted, user 2026-10-01):** (1) the compiler keeps appending its implicit `return null` even after an explicit `return` — the trailing `LoadConst(Null); Return` is dead bytecode, unobservable at runtime (the explicit `Return` exits first) and §142 says nothing about it; left unchanged; (2) a merged module program driven through plain `ZeroVM::run()` can fall through into appended bodies because `merge_modules` does not close the stream with `Halt` — unreachable on product paths (`zsc`/`zpm` → `run_modules`; single file → `compile()` + `Halt`); neither is changed in this job.

**Resolved (2026-10-01):** closed per user answer (jawaban.md, option A — both observations accepted as-is); evidence: `test_job019_module_programs_get_header_and_validate` exercises the real `run_modules` path; both observations remain visible as CHECK.md §18 candidate-gap rows; full suite → 834 passed, 0 failed.

## 69. Un-awaited Task Failure Reporting (2026-10-02)

### 69a. Async.md §108 was silent on reporting un-awaited task failures — **Resolved (2026-10-02)**

Problem: §108 stated only that an un-awaited failure's error is not thrown into another scope ("tidak secara otomatis dilempar ke scope lain"), which made such a failure permanently invisible — especially for `main()`, which can never be awaited from top level because top-level await is a compile error (§49/§101). No diagnostic, no stderr line, nothing.

**Decision (user 2026-10-02):** Async.md §108 gains an "Un-awaited Task Failure" rule block: when a task fails and is never awaited, the runtime prints one warning to stderr when the program finishes; the warning does not throw into another scope and does not stop the program; every failed-un-awaited task produces exactly one warning; an awaited task produces no additional warning for the same failure. User chose option B for combinator observation: an error delivered through `task.all()`, `task.race()`, `task.wait()`, or `task.timeout()` that itself reaches an `await` counts as observed — the source member produces no warning (recorded as a fourth bullet in the spec block).

**Resolved (2026-10-02):** spec text supplied by the user was inserted verbatim (with quotes/adaptation to document style plus the option-B bullet) into Async.md §108 as the "Un-awaited Task Failure" block; implemented in `src/vm.rs`: `Task.awaited` set by `execute_await`, `Task.failed_from` recorded at the four combinator propagation sites (all/race/wait/timeout in `reevaluate_synthetic`), `failure_was_observed` walks the chain (member ids are always smaller than their combinator's, so recursion terminates), `emit_unawaited_failure_warnings` appends one line per unobserved failed task to `err_output` (the stderr channel `print.err` uses; `zsc` prints it to stderr) at the single normal exit of `run_scheduler`, which covers both `run()` and `run_modules()`. Tests: `test_job014_unawaited_failure_emits_warning`, `test_job014_unawaited_failure_one_warning_per_task`, `test_job014_awaited_failure_emits_no_warning`, `test_job014_task_all_observed_failure_emits_no_warning` (option B), `test_job014_no_failures_emits_no_warning`; full suite → 846 passed, 0 failed. Note: the warning wording (`ZeroScript warning: task "<name>" failed but was never awaited: <error>`) is an implementation choice — the spec requires one warning per task but does not prescribe text (disclosed here, not spec-derived).

## 70. Job 022 Ambiguity Analyses (2026-10-04)

### 70a. Job file's closure sample (test req 7) vs §31 lexical scoping — **Resolved (2026-10-04)**

**Problem:** `jobs/022-default-parameters-call-time.md` test req 7 gives a sample whose expected first output is `20` but whose snippet puts `x = 30` at top level and calls `f()` (a closure whose default is `a = x`) from inside `outer()` where `let x = 20` shadows the name. §31 says closure name resolution is lexical — inside `outer()`, `x` resolves to the local binding, so the call-time value is `20`, never the top-level `30`. The job file's own first expectation ("20 — closure's captured env") agrees with §31; the `30` reading would require dynamic (call-site) name lookup, which the language does not have.

**Decision (reading adopted):** lexical resolution per §31 — `function(a = x)` inside `outer()` reads `outer`'s captured `x` (verified: `t4` prints `20, 20`). The top-level `x = 30` is unreachable through that name from that lexical position. The test `test_job022_closure_call_default_uses_captured_env` therefore asserts `20` for the live captured value, plus two extra variants covering what the sample was reaching for: a default that observes an outer mutation made before the return (`99`), and a real closure whose default also reads a global (`10` → `30` across calls).

**Resolved (2026-10-04):** decided reading implemented as stated; no language rule invented (§31 governs); evidence: `test_job022_closure_call_default_uses_captured_env` passing; suite → 890 passed, 0 failed.

### 70b. Closure / function / arrow expression as a default parameter value — **Resolved (2026-10-04, recorded as decided behavior)**

**Problem:** §30 defines default parameter behavior ("default dievaluasi saat function dipanggil") but says nothing about *what expressions* are legal there, and nothing specifically about closure-creating expressions (`f(cb = () => x)`, `f(h = function() { ... })`). The evaluation environment for such a default (which environment would the created closure capture — the callee's scope? the caller's? the eval frame's?) is unspecified.

**Decision (job scope, no semantics invented):** out of scope for Job 022 per the job file's Scope/Out-of-Scope. No syntax is rejected and no special semantics are implemented — a closure expression in a default position, if it compiles at all, evaluates like any other expression inside `evaluate_default_body`'s eval frame, and its capture resolution falls out of the existing §130/§31 rules. Nothing in the spec was changed and no rule was invented.

**Resolved (2026-10-04):** recorded as decided behavior; spec gap tracked in CHECK.md "Candidate Gaps for Future Work (discovered during Job 022)"; evidence: suite → 890 passed, 0 failed (no test asserts closure-as-default semantics — deliberately untested pending a spec rule).

### 70c. `await` inside a default parameter expression — **Resolved (2026-10-04, recorded as decided behavior)**

**Problem:** §30 does not mention async defaults at all, and §101's task-start timing covers when an *async function call* begins, not what happens if the *default itself* contains `await`. The eval frame in `evaluate_default_body` carries no task context (`task_id: None`, instruction buffer swapped in), so an `await` opcode in a default body has no suspension mechanism.

**Decision (recorded, no rule invented):** default evaluation is synchronous — it runs to completion inside `build_function_scope` before the function body (or, for async functions, before the task is ever scheduled, at `prepare_task_start`). A default containing `await` is not specially rejected at compile time (no spec text forbids it) and, if one ever compiles, its behavior is undefined by the current spec. Async function *calls* still defer their whole default evaluation to task start per §101 (tested: `test_job022_async_default_evaluated_at_task_start`).

**Resolved (2026-10-04):** recorded as decided behavior; spec gap tracked in CHECK.md "Candidate Gaps for Future Work (discovered during Job 022)"; evidence: suite → 890 passed, 0 failed.

### 70d. §4 / §58a vs the module pipeline: same source, two assignment behaviors — **Resolved (2026-10-05)**

**Problem (discovered while writing Job 022 req 2's test, pre-existing at baseline bb542a9):** the same source behaves differently under the two compile pipelines:

- `compile()` (used by unit-test `run_source`): top-level `let` → `DeclareGlobal`; inside a function, `resolve_for_store` takes the §4/§58a reading-2 path (`StoreLocal` + `create_local`) → §4's example prints `20` then `10` ✓ (test_job008_function_scope_assignment_does_not_change_outer passes).
- `module::load` (used by `zsc` for every real script): top-level `let` → `DeclareLocal` + `MakeClosure` capture (top-level bytecode: `DeclareLocal("A")`, `LoadFunction(0)`, `MakeClosure(0, [("A", false)])`); a plain/compound assignment inside a function then resolves to `StoreCaptured` and mutates the shared module env → §4's example via `zsc` prints `20` then `20` ✗ (Language.md §4 states Output: `20`, `10`; verified directly: `zsc /tmp/opencode/t7.zs` → `20\n20`, identical on the clean baseline worktree at bb542a9).

**Readings considered:**

1. **§4/§58a reading 2 is normative** (already decided 2026-10-01 in §58a): assignment inside a plain function creates a local; §4's stated output `20/10` is normative. → the module pipeline violates it (CLI = the product path).
2. **Module path is reading 3 (explicit capture)**: `MakeClosure` at module compile is "compile-time capture detection", so `StoreCaptured` is legitimate shared-env mutation (§130). → but §58a explicitly rejected reading 3 for §4's example ("capture is a separate mechanism reserved for functions that close over the name at compile time"), and §4's own text says the assignment "tidak mengubah A milik outer scope".
3. **Top-level `let` means different things per pipeline by design** (global vs module-local-capturable) and §4 only speaks about globals. → possible, but no spec section says a module's top-level `let` is a mutable-through-closure local that overrides §4's example; §4's example *is* a top-level `let`.

**No rule invented.** Out of Job 022's scope (Job 022 touches only default-parameter evaluation). Recorded so it is not silently dropped: two unit-tested behaviors (`run_source` global path, §4-correct) and one product-tested behavior (`zsc` module path, §4-divergent) disagree about the same spec example, and Job 022's own tests had to be written against the `run_source` path (closure-captured counter / object-property counter) because the top-level-counter variant only mutates under the module pipeline.

**Recorded (2026-10-04), superseded by the resolution below:** candidate gap tracked in CHECK.md "Candidate Gaps for Future Work (discovered during Job 022)"; at the time no decision had been made on which pipeline §4 governs before either path is changed.

**Resolved (2026-10-05, `jobs-issue/jobs-70d.md`):** fixed in the module pipeline; neither spec text nor `compile()`'s behavior was changed. Decision — the discriminator between readings 1 and 2 is **the export status of the binding OR of the function being compiled**, not function-kind alone (Job 012's `test_job012_private_captured_by_export`, where a plain `export function increment()` mutates the *private* `count` and the importer must observe `get()` returning `2`, shows that export context must keep shared mutation even for a private binding; §4's example has neither an exported binding nor an exported function):

1. `new_module()` now sets `module_mode` (compiler.rs:343) and `compile_module` statically seeds `exported_names` before any statement compiles (`seed_export_names` compiler.rs:377, called compiler.rs:2293 — declaration order independent, mirroring §78's static import seeding; only local `export` declarations/`export list`, not re-exports).
2. `resolve_for_load`'s enclosing walk is index-aware (compiler.rs:540-551): a binding in the module scope (`offset == 0`) with no export status and compiled outside any exported function (`!fn_is_exported`, set at `Stmt::FunctionDecl` compiler.rs:1050 and inherited by nested functions/defaults) resolves to the new `VarRes::ModuleScope` / `ConstModuleScope`; the same binding under an exported binding or exported-function context still resolves to `Captured`.
3. `resolve_for_store` gained only arms for the two new variants (compiler.rs:584-585): `ModuleScope` → `(Local, true)` — i.e. §4/§58a reading 2: the store declares a function-local, and every later load in the same function resolves to that local; `ConstModuleScope` → const-assignment error (compiler.rs:602). Existing variants' behavior is untouched — rule 5's "don't change `resolve_for_store()` globally" is respected, and genuine closures over *function*-scope bindings (§31/§130) are untouched because those never have `offset == 0`.
4. Loads are unchanged: reads still go through `LoadCaptured` + `track_captured`, so env retention (§65: exported functions reading private state) and read-before-store semantics (`print(A)` sees the outer `10`, then the local `20`) hold; only store classification changed. Assignment/compound/postfix store arms were extended accordingly (compiler.rs:1659/1663, 1735/1738, 1829/1832).
5. `compile()`/`run_source` path untouched (`module_mode` false) — all pre-existing tests unaffected.

Evidence: `zsc /tmp/opencode/t7.zs` (§4 example) → `20` then `10`; `test_job70d_genuine_closure_shared_mutation` → `1,2,3` (§31/§130); `test_job70d_live_export_binding_still_shared` → `0,1` (§66); `test_job012_private_captured_by_export` still `0,2` (export context shares private state). Suite → 893 passed, 0 failed (warnings 2, baseline). Spec-silent edge recorded: a *private* plain function called by an exported one (wrapper pattern) stores module-scope bindings locally — no spec section covers it; classification follows the binding/function export status above.

**Audit against §31/§130 (2026-10-05, probes run on both pipelines):** every normative bullet was verified on the module pipeline, not just the three job-required tests:

- §31: read-only closure → `Kael`; modify captured function-scope variable → `1,2,3`; captured variables live while needed → Job 020/021 liveness tests; const immutable → compile error.
- §130: reads and changes of captured `let` ✓; **sibling visibility** (`Perubahan captured variable dapat terlihat oleh closure lain yang berbagi environment`) now covered on the module pipeline by `test_job70d_sibling_closures_share_function_scope_env` → `1,2,2` — Job 021's sibling tests all run through `run_source` (compile path) and never exercised `module::load`; env retention on the Heap → Job 020/021; scope/lookup rules → read-before-write `10,20,10` (`test_job70d_function_reads_outer_then_writes_local`).
- Closure over a non-exported module-scope binding: store → local (`1,1,0`) on **both** pipelines, pinned by `test_job70d_closure_module_scope_store_stays_local`. Grounding: §58a reading 2 (Resolved 2026-10-01) — "assignment inside a function always creates a function-local, never touching outer bindings" — and empirical equality with the `compile()` reference pipeline; no §31/§130 example covers top-level bindings (both examples use function-scope `nama`/`A`), so this cell is decided-behavior, not an invented rule.
- Cross-pipeline probe table (9 sources, compile path vs `zsc`): 7 identical (`Kael`; `1,2,2`; `10,0,0`; `1,1,0`; `10,20,10`; `11,10`; §4 `20,10`). Two **pre-existing** divergences recorded, not fixed (out of §70d scope, unchanged by this job): (a) `export` declarations only mean anything in the module pipeline — `compile()` treats them as plain declarations, so the export-binding probe differs (`0` vs `1`) by design of the unit-test pipeline, not by a §70d regression; (b) private `const` assigned inside a plain function: the module pipeline errors (pre-fix it errored too, via `ConstCaptured`) while `compile()` allows a function-local shadow per §58a — fixing this would require distinguishing imported bindings from own `const` (both are `is_const` at module scope; §66 mandates the import error), and the spec states no shadow rule for a module-scope `const`; no rule invented.
- Audit added 4 regression tests: `test_job70d_sibling_closures_share_function_scope_env` (§130 sibling on module path), `test_job70d_function_reads_outer_then_writes_local` (§4 + §3 read chain), `test_job70d_plain_function_mutates_exported_binding_shared` (§66/§74: export let mutable di module asal from any function), `test_job70d_closure_module_scope_store_stays_local` (decided reading above). Suite → 897 passed, 0 failed.

---

## 71. Job 023 Ambiguity Analyses (2026-10-08)

### 71a. Error kind per limit — §150 "runtime/VM error sesuai jenis batas" vs §133/§154 — **Resolved (2026-10-08)**

§150 says an execution limit "dapat dihentikan dengan runtime/VM error sesuai jenis batas yang dilanggar" but never says which limit maps to which error kind. §133 offers memory as "VM Error atau memory error". §154 makes only runtime errors catchable and lets categories be adjusted "selama jenis kegagalan dapat dibedakan secara konsisten". §153 lists "resource limit" as a termination cause alongside runtime error and VM Error.

**Decision (reading adopted):** time, call stack depth, task count and instruction count raise a **catchable runtime error** (`VmError::RuntimeError`, routed through the ordinary `raise_runtime_error` machinery, so try/catch, task-failure and uncaught termination all behave like any instruction error); the message prefix is `Execution limit exceeded: …` naming the specific limit, and `classify_runtime_error` maps that prefix to a new category **`LimitError`** (§154 permits category adjustment; the message distinguishes which limit). **Memory limit raises `VmError::MemoryLimit`, a VM Error — not catchable** — chosen from §133's two allowed outcomes, consistent with §153 (VM-level termination) and with the GC-then-fail sequence being VM-internal; it aborts `run`/`run_modules` like any other non-runtime `VmError`. A limit error raised while **no execution context is loaded** (scheduler idle: HTTP park in `run_scheduler`) surfaces to the host as an uncaught runtime error (§153) — catchability applies only where a handler frame is live, which is ordinary machinery, not a new rule. Error wording (`Execution limit exceeded: wall clock time …`, etc.) is implementation text; §154 leaves format details out of core semantics.

**Re-trip behavior (disclosed, same date):** limits are state checks, not one-shot events. Time and instruction budgets, once exceeded, keep exceeding: a delivered catch handler for the **time** limit gets the amortized-sampling grace (up to 1024 instructions) and can finish or loop-until-re-trip; for the **instruction count** the very next instruction re-raises, so the catch clause is entered but cannot execute — the limit behaves in practice like a termination (§153), and a crossed `finally` is cut after its first instruction on an exhausted budget. **Depth** and **task-count** limits clear when the unwind removes frames or the handler creates no further task, so their handlers run normally. No re-arm hangs: an uncaught delivery aborts `run`/`run_modules`. §43's finally guarantee is therefore fully observable for depth/task/time and reduced to §153 termination semantics for an exhausted instruction budget — recorded here rather than inventing a latch mechanism the spec does not describe.

### 71b. "Execution time" — real time vs the virtual clock (§64d) — **Resolved (2026-10-08)**

§64d decided that sleep/timeout run on a **virtual** monotonic clock advanced only when nothing is runnable. §150's execution time is listed under "resource yang digunakan oleh program" — a runtime resource, not a program-observable value. A busy `while true {}` never advances the virtual clock at all, and virtual sleeps consume no real time.

**Decision (reading adopted):** the §150 execution time limit measures **wall-clock elapsed time of the execution** (`Instant` stamped at `run`/`run_modules` entry), sampled amortized at instruction boundaries (every 1024 dispatched instructions) and at every scheduler idle-park iteration (~1 ms while HTTP is on the wire), so both busy loops and in-flight HTTP waits are bounded. The virtual clock of §64d is untouched: sleeping "one hour" virtually consumes ~no wall time and therefore does not consume this budget — the limit guards the runtime resource, not program-perceived time. Limits checked while no context is loaded (park) are uncaught, per §71a.

### 71c. Is "instruction count" sanctioned? §150's list does not contain it — **Resolved (2026-10-08)**

CHECK.md's §150 row audits "instruction count, time, call depth" but §150's own list is "execution time, memory usage, call stack depth, task count". §150 uses "dapat mencakup" (may include), and §149 says an "Execution limit dapat digunakan untuk mencegah program berjalan tanpa batas" without enumerating kinds.

**Decision (reading adopted):** the §150 list is **illustrative, not exhaustive** ("dapat mencakup"), so a host-configurable **instruction count limit** is sanctioned as an additional execution limit under §149's purpose sentence; it is also the deterministic companion to the wall-clock sample (exactly N instructions may dispatch before the error). It is implemented and disclosed as a fifth field of the host `Limits` API; the CHECK row can then be marked Implemented against its own wording.

### 71d. Memory metric — §133 does not state a unit — **Resolved (2026-10-08)**

§133 caps "Heap memory" but names no unit; it explicitly leaves strategy to the implementation ("Detail strategi allocation dan reclamation tetap menjadi tanggung jawab ZeroVM") and requires that GC may run before allocation is declared failed.

**Decision (reading adopted):** the metric is **occupied heap slots** (`heap.slots.len() - heap.free_list.len()`), O(1) to read, checked at instruction boundaries: if occupancy exceeds the cap, `run_gc()` runs first (§133: "Garbage collection dapat dijalankan sebelum runtime menyatakan allocation gagal"); re-measured occupancy still above the cap → `VmError::MemoryLimit` (§71a). Reclaimable garbage therefore does not count toward the cap. Known bound, disclosed: occupancy can overshoot the cap by at most the allocations of the single instruction that ran before the boundary check — §133 requires no hard allocation-time refusal, and refusing inside `Heap::allocate` would either change its signature or trigger GC at unsafe points (Job 020: GC only at boundaries).

### 71e. "Task count" — total table vs active tasks — **Resolved (2026-10-08)**

§150 says "task count" without defining which tasks are counted. `ZeroVM.tasks` is monotonic (terminal tasks are retained — §152 gap), so total-table size is a retention/memory concern, while §150 sits among execution resources.

**Decision (reading adopted):** the limit counts **non-terminal tasks** (pending, running, suspended — `tasks.len() - count_terminal()`), checked before each task-table push (`create_task`, `create_synthetic_task`): creating a task when the active count already equals the limit raises the §71a runtime error. Terminal tasks keep filling the table and are governed by the §133/§152 memory dimension instead. Rationale: §150 guards execution pressure; a task that reached a terminal state no longer executes.

### 71f. Invalid configuration (§149 startup error) — **Resolved (2026-10-08)**

§149: "Configuration yang tidak valid sebelum program dijalankan menghasilkan startup/runtime configuration error" — without saying what is invalid or where the check happens.

**Decision (reading adopted):** a limit of **`0` is invalid** for every field (a zero budget cannot be satisfied by any execution — even an empty program dispatches instructions / holds the root frame), and `None` means unlimited. `ZeroVM::set_limits` validates and returns `Err(VmError::InvalidRuntimeConfiguration(…))` immediately (host feedback before anything runs), and `run`/`run_modules` re-validate the stored limits **before executing the first instruction** as defense in depth — both surfaces satisfy "sebelum program dijalankan". Values are unsigned; there is no negative case.

### 71g. Configuration surface — **Resolved (2026-10-08)**

§147 makes the runtime context internal ("Detail struktur Runtime Context merupakan detail internal ZeroVM"); §149 says configuration format "bukan bagian wajib dari syntax ZeroScript dan dapat ditentukan oleh implementasi runtime" and that the program must not change resource limits itself.

**Decision (reading adopted):** the surface is a **host API only** — a public `Limits` struct (fields: `execution_time_ms`, `heap_slots`, `call_stack_depth`, `task_count`, `instruction_count`; all `Option`, `None` = unlimited) settable via `ZeroVM::set_limits`, mirroring the existing `set_program_args` host seam. There is **no in-language surface** (no binding, no stdlib call — §149 forbids the program changing limits) and **no `zsc` CLI flags** in this job: inventing CLI UX is out of scope (recorded as a candidate gap/future job in CHECK.md rather than invented here). §149's latitude makes the API shape an implementation detail, disclosed in the job report.

---

## 72. zsc Evaluation Mode — spec-writing session (2026-10-08)

### 72a. `zsc -e` had no specification — **Resolved (2026-10-08, user-requested spec)**

The user requested a spec for running code without a file (`zsc -e`, "eval-like"). No spec text existed anywhere for it — the only invocation rules in the corpus are §100's binary names/exit codes and §52's `system.args()` example — so implementing the flag would have violated AGENTS §3/§12, and §71g had just recorded that `zsc` flags need a spec section first.

**Decision (spec written):** a "Mode Evaluasi" rule block was added to Dependencies.md §100 (the section that already owns `zsc` binary names, usage errors and exit codes), as a block in the style of "Nama Binary dan Kode Keluar" — no new global section number, so INDEX.md is unchanged. Adopted rules:

1. `-e` / `--eval` is recognized only as the **first** argument; the code is the argument immediately after the flag; missing code → usage error, exit 1 (§100). Every argument after the code is a program argument and is never re-parsed as a `zsc` flag (so `zsc -e "code" -e x` runs `code` with `system.args()` = `["-e", "x"]`).
2. Empty code is a valid empty program (no output, exit 0) — chosen over "missing argument" because `""` is an argument that was provided; disclosed as a mechanical choice (spec was silent).
3. Execution uses the identical compile → bytecode → ZeroVM pipeline; every language/runtime rule (modules, async, error handling, §150 limits) applies unchanged.
4. **Import base (the one genuine ambiguity):** §64 states "Path resolution tidak bergantung pada current working directory; acuannya adalah lokasi file yang melakukan import" — and an eval program has no file. Two readings were possible: strict (relative import → ERROR, rule unsatisfiable) vs. virtual file. **User answer (2026-10-08): the eval program is treated as located at the current working directory** — `"./lib"` → `<cwd>/lib.zs`. §64's rules for file-based programs are untouched; one sentence added to §100. Package imports (§87) are unaffected either way.
5. `system.args()` yields exactly the arguments after the code (no flag, no code, no script name) — consistent with §52's "nama file program tidak termasuk" (eval has no file at all).
6. Syntax/runtime/usage errors follow the §100 exit-code block (stderr, exit 1).

**Not specified (not requested):** stdin mode (`zsc -e` reading from a pipe), concatenating several `-e` flags into one program (falls under rule 1: only the first `-e` is a flag), and any new exit-code categories.

**Follow-up — implementation closed (2026-10-08, Job 025):** `zsc -e` / `zsc --eval` implemented in src/main.rs (`cmd_eval`, dispatch on the literal first argument) with the entry compiled via the new `module::load_source(virtual_path, source)` (src/module.rs) — the entry source is parsed straight from the argument string; the virtual path `<cwd>/__zsc_eval__.zs` is never created on disk and exists only as the import-resolution parent (rule 4) and `Project::discover` root; imported files are read normally (§64/§87 machinery unchanged). The help text gained the `-e` line (the omission noted in §73a's mechanical list was contingent on this job being pending and is now moot). Mechanical decision recorded here: a file literally named `-e` is shadowed by the eval flag exactly like `compile`/`run` are shadowed by the §158 subcommands — run it as `./-e`. Evidence: `test_zsc_eval_*` (11 tests in tests/zero025_zsc_eval.rs: run/alias/missing-code/empty-code/program-args/second-`-e`-as-data/cwd-import/syntax-error/runtime-error/`./-e` disambiguation/help+file-mode-unchanged); full suite → 938 passed, 0 failed.

---

## 73. Bytecode Serialization — spec-writing session + Job 024 decisions (2026-10-08)

### 73a. Serialization had no specification — **Resolved (2026-10-08, user-requested feature)**

CHECK.md had tracked "**No bytecode file format**" since Job 019 (§124/§125 define a header and a version, but no serialization, no on-disk artifact, no rule for loading foreign bytecode — ISSUES §68d explicitly recorded that "the spec defines no bytecode file format, so nothing serializes it"). The user decided bytecode work takes priority and requested the feature with the instruction to surface every decision needing approval.

**User decisions (2026-10-08, all four via question round — recommended options adopted):**

1. **Purpose = explicit compile + distribution**: `zsc compile <file> [-o <out>]` produces one artifact; `zsc run <file.zsbc>` executes it without source; plain `zsc <file.zs>` stays compile+run in one step (no implicit cache I/O).
2. **Granularity = one file = the whole merged program** (entry + all imports + resolved package dependencies + ModuleMeta table), mirroring the existing `module::load` → `merge_modules` pipeline; §124 validation runs once at load. Per-module cache artifacts rejected for v1.
3. **CLI shape = subcommands** `zsc compile` / `zsc run` (zpm precedent; no extension-based mode guessing), default extension `.zsbc`.
4. **Integrity = SHA-256 over the payload** (reusing the §97 hand-rolled SHA-256 in src/hash.rs), appended after the payload; mismatch → ERROR before any parse/execution.

**Mechanical details decided here (disclosed, spec does not state them):**

- Placement: new section **§158 "ZeroVM Bytecode Serialization"** at the end of Bytecode.md + INDEX.md row (new global number by user request, precedent §130) — no existing section's text was rewritten.
- Byte encoding: little-endian; `usize` ↔ `u64` (try_from on load → `Malformed` if it does not fit); `f64` as raw IEEE-754 bits via `to_bits`/`from_bits` (NaN payloads survive); strings = `u64` byte-length + UTF-8; vectors = `u64` count + elements; `bool` = `u8` 0/1 (any other byte → Malformed); `Option<usize>` = `u8` tag + value.
- Opcode tags = explicit `u32` literals mirroring `compiler.rs` declaration order (0..=56, 57 variants). The encode `match` is exhaustive, so adding a variant is a compile error until the codec is updated. **Decode maps an unrecognized tag to `Opcode::Unknown(tag)`** rather than a format error — so a foreign/modified stream is rejected by the existing §124 validation as a VM Error (`InvalidOpcode`, ISSUES §68e), exactly as §124 requires for unknown opcodes.
- File layout: `payload || sha256(payload)[32]`. The checksum is verified *before* parsing; magic/version live inside the payload and are still checked by the existing `validate_header` path during `run_modules` (§124/§125 unchanged — a wrong-version file with a correct self-hash still fails as VM Error, keeping integrity and compatibility as separate concerns).
- Payload section order: header (magic string, version u32), entry_point, constants, instructions, functions, function_bodies, default_bodies, modules.
- `zsc compile` default output: same directory as the source file, extension `.zsbc` (replacing `.zs`, else appended); `-o` overrides relative to cwd.
- `zsc compile` does **not** re-validate before writing: §124 mandates validation before *execution*, which `zsc run` (and plain `zsc file.zs`) perform via the existing `validate()`; roundtrip tests pin that the writer's output always passes it.
- Subcommand disambiguation: first argument `compile`/`run` selects the mode; a real file with that name runs as `./compile` / `./run` (same convention as `zpm`). Usage/help text updated to list the subcommands; `-e`/`--eval` is **not** added to help because §100's eval mode has no implementation yet (separate pending job, CHECK.md candidate gap).
- Error classes: checksum/truncation/parse failures are plain load errors at the CLI (message on stderr, exit 1 — no program exists yet, so catchability is not in question); magic/version/structural problems keep flowing through `VmError` from `run_modules` as today.
- v1 artifact carries no source and no debug info: the compiler emits none today (`e.location` is null everywhere, ISSUES §59a), and §155 requires bytecode to be runnable without debug information. Adding debug payloads later is a bytecode-version bump under §125.

### 73b. Asymmetric foreign-tag handling: opcodes vs constants — **Resolved (2026-10-08)**

§124 mandates that an unknown *opcode* is a VM Error, and ISSUES §68e deliberately gave the in-memory enum an `Opcode::Unknown(u64)` representation for exactly that. The spec is silent on an unknown *constant* tag inside a foreign payload; §158 only groups "struktur file tidak terbaca" under ERROR.

**Decision (mechanical, disclosed):** unknown opcode tags decode to `Opcode::Unknown(tag)` so the existing §124 validation reports `InvalidOpcode` at run time (evidence: `test_job024_foreign_opcode_tag_becomes_unknown_and_fails_validation`); unknown *constant* tags are `BytecodeFileError::Malformed` at load — a malformed constant table has no §124 analog and cannot be interpreted even approximately, while opcodes do have one. Trailing bytes after a fully parsed payload are likewise `Malformed` (the §158 layout is exactly `payload || hash`; extra bytes mean parser and file disagree). All three surface as ERROR before any execution at the CLI (§158, §100 exit 1).

---

## 74. `zsc init` scaffold — implementation decisions (2026-10-08)

### 74a. No specification exists for an init command — **Resolved (2026-10-08, user-requested feature)**

No section of the corpus mentions `zsc init` (or `zpm init`); the only related text is §82's definition of the `project.zson` manifest itself. The user requested the command ("`zsc init` belum bro") with the instruction to keep it small, and answered the one product question: **create only `project.zson`** (recommended option — no `src/`, no `.gitignore`).

**Mechanical details decided here (disclosed, spec does not state them):**

- Content = a minimal valid §82 manifest written via the existing `json::render_zson` (bare keys, 4-space indent): `name` = the current directory's file-name component (empty/root → ERROR, cannot determine), `version = "0.1.0"` (§82 validates project version as any string; the literal is a choice), `dependencies = {}`. The rendered text is re-parsed with `package::parse_manifest` before writing so the tool can never emit a manifest its own parser rejects.
- If `project.zson` already exists in the directory → ERROR (message names the directory), exit 1; the existing file is never overwritten or merged. Extra arguments → usage error, exit 1 (consistent with `zsc compile`/`zsc run` taking no free-form extras beyond their defined flags).
- Silent on success (no "created ..." stdout) — consistent with `zsc compile`.
- First-argument dispatch: `init` selects the subcommand; a real file named `init` runs as `./init` (same §158/§100 convention as `compile`/`run`/`-e`).
- Error classes: plain CLI errors (stderr, exit 1) per §100; no program is involved.
- Evidence: `test_zsc_init_*` (4 tests in tests/zero026_zsc_init.rs); CHECK.md §12 manifest row updated with one sentence. No new spec row was added to the taxonomy — the command has no spec section; it is scaffolding for the already-tracked §82 feature.

### 74b. Expanded to interactive scaffold — **Resolved (2026-10-08, same day, user correction)**

The minimal first cut (74a) was rejected by the user as too bare: "script mana? license, author desk, keyword, mana? ask nya mana?" — i.e. `zsc init` is expected to behave like `npm init`: ask the questions interactively and produce a usable starter project.

**Adopted design (user-directed; mechanical details disclosed):**

- Prompts, in order, one line each on stdout (flushed; English, matching the CLI's existing message style): Project name (default = directory file-name component), Version (default `0.1.0`), Description (optional), Author (optional), License (optional), Keywords comma-separated (optional), Entry script (default `main.zs`). Empty answer = default; EOF (e.g. non-interactive stdin) = every remaining prompt takes its default, so `zsc init < /dev/null` works.
- `project.zson` content: §82 core (`name`, `version`, `dependencies = {}`); optional metadata keys **`description`/``author`/`license` reuse the §96 package-metadata names** (spec defines them for `package.zson`, not `project.zson` — both parsers ignore unknown fields, so this is tolerated; recorded here as a user-directed choice, not spec-derived); **`keywords` is an npm-style addition with no spec section anywhere** (user-directed; rendered as a ZSON string array). Optional fields are omitted entirely when left empty. The rendered text is still re-parsed with `parse_manifest` before writing (§82 self-check).
- Entry script: default `main.zs` per §82's example project structure ("my-app/ ├── project.zson ├── main.zs"); a custom answer is created relative to the project root, with parent directories created as needed; content = `print("Hello from <name>!")`. An **existing entry script is never overwritten** (only the manifest is written); an existing `project.zson` still aborts the whole command (74a rule kept).
- Evidence: `test_zsc_init_*` (6 tests — EOF defaults, interactive answers incl. keywords array + custom nested entry, refuse-existing, entry-not-overwritten, usage error, `./init` disambiguation).

### 74c. Default `scripts` section — **Resolved (2026-10-08, same day, user request)**

The user noticed the scaffolded manifest had no `scripts` field, though `parse_manifest` already accepts and validates one and §86 defines `zpm run <script>` over it (spec example: `start: "zsc index.zs"`).

**Decision (mechanical, disclosed):** `zsc init` always writes `scripts: { start: "zsc <entry>" }` — the `start` key mirrors §86's example shape, and the command tracks whatever entry script was chosen (default `main.zs` → `start: "zsc main.zs"`). No extra prompt; not configurable at init time. Evidence: `test_zsc_init_eof_defaults_scaffold` and `test_zsc_init_interactive_answers` both assert the `start` value.

## 75. Value stack per frame (ZeroVM.md §116) — Job 027 decisions (2026-10-11)

### 75a. Value yang hidup ketika TIDAK ada execution frame — **Resolved (2026-10-11, keputusan mekanis)**

§116 baris 158: "Value Stack bersifat lokal terhadap execution frame". Refactor memindahkan seluruh operand ke field baru `Frame.stack` (src/vm/types.rs); `ZeroVM` tidak lagi punya field `stack` gabungan. Satu kondisi tak tercakup aturan tersebut: **top-level `return`** — `execute_return` membuang root frame (len 1, non-task) lalu nilai kembali harus ditaruh di suatu tempat *sebelum* ada frame tujuan. Spec §116/§115/§117 tidak mendefinisikan `return` di skala program (dan perilaku lamanya sudah janggal: `return_pc` root = 0 sehingga program **dieksekusi ulang dari awal**; di mode module jalur itu berakhir dengan `Invalid execution state: No root frame` pada src/vm/mod.rs:503).

**Keputusan:** pertahankan observabel persis refactor-sebelumnya lewat slot fallback kecil `ZeroVM.orphan_stack` — hanya terisi pada jendela degen "root frame sudah dibuang, belum ada frame tujuan". Helper `stack()`/`stack_mut()` memakai `Frame.stack` frame aktif dan jatuh ke `orphan_stack` hanya bila `call_stack` kosong (satu kasus ini). Konskuensinya:

- Alur top-level return (eksekusi ulang, second-hit branch `call_stack.is_empty()`, pesan `No root frame` mode module) identik byte-per-byte — diverifikasi membandingkan binary refactor dengan binary baseline (`zsc -e 'return 1'` dan program print/return: exit code, stdout, stderr sama).
- `gc_roots` tetap me-root `orphan_stack` (src/vm/mod.rs:194) — nol risiko koleksi nilai mengambang.
- Observasi pre-existing (eksekusi ulang top-level return + kegagalan mode module) dicatat sebagai candidate gap di CHECK.md §18 — **tidak diperbaiki di job ini** (spec senyap; butuh aturan bahasa dulu).

### 75b. `TaskContext` tidak lagi membawa field `stack` — **Resolved (2026-10-11, mekanis)**

Karena stack hidup di dalam `Frame`, save/load context (`save_active_context`/`load_active_context` src/vm/task.rs) cukup menukar `call_stack` — stack ikut terbawa otomatis (§112 konteks tugas). `TaskContext.stack` dihapus; field literal `Frame` bertambah `stack: Vec::new()` di semua 7 konstruktor (mod.rs ×5, task.rs ×1, tests.rs ×1). Pengamatan: semua task context yang disimpan/dipulihkan selalu punya ≥1 frame pada titik suspend, jadi `orphan_stack` selalu kosong di perbatasan pergantian konteks — tanpa mekanisme sinkronisasi tambahan.

### 75c. Batas maksimum value stack — **catatan (bukan gap baru)**

§116 baris 162: batas stack "dapat ditentukan oleh implementasi". Tidak ada batas yang ditambahkan — `Limits` (§150) tidak diubah, konsisten dengan pendahulunya (ISSUES §71). Jika suatu saat diperlukan, tempat alaminya adalah `Limits` + enforcement di `stack_mut()`.

### 75d. Arti `ZeroVM::get_stack()` setelah refactor — **Resolved (2026-10-11, mekanis)**

`get_stack()` sekarang mengembalikan `stack()` — stack frame aktif (atau `orphan_stack` bila tanpa frame). Pada akhir program normal root frame berisi tepat apa yang stack lama berisi (callee seimbang: `Return` menaruh tepat satu nilai return ke pemanggil), sehingga slice yang dilihat embedder/test identik — 48 pembacaan test lama (kini `vm.get_stack()`) dan seluruh suite membuktikannya. Tidak ada API baru yang diekspor selain itu.

### 75e. Aturan top-level `return` — **Resolved (2026-10-12, keputusan user opsi B + Job 032)**

`§115`-`§117` (ZeroVM.md), `§26` (Language.md), dan blok Aturan RETURN (Bytecode.md) sebelumnya tidak mendefinisikan `return` di top level; perilaku VM janggal (eksekusi ulang dari awal; mode module → `Invalid execution state: No root frame`, exit 1 — tercatat sebagai candidate gap CHECK.md §18 dan oleh §75a dibiarkan identik sengaja).

**Keputusan (opsi B):** `return` di top level program = **mengakhiri program**. Frame utama program adalah frame aktif terakhir; nilai `return` diabaikan (program tidak memiliki pemanggil); statement setelahnya tidak dieksekusi; program selesai normal (exit 0, output sebelum `return` tetap dihasilkan); berlaku sama pada `run()` dan `run_modules`.

**Revisi spec (2026-10-12, tambahan tanpa rewrite paragraf lama):**
- `Language.md` §26: dua paragraf aturan top-level setelah "`return` hanya menghentikan function tempat ia berada" (selaras: di top level frame "tempat ia berada" = frame utama program).
- `Bytecode.md` (Aturan RETURN): bullet "Jika `RETURN` dijalankan pada frame utama program (top level, tanpa pemanggil), nilai `RETURN` diabaikan dan program selesai normal."
- `ZeroVM.md` §117 (Aturan): bullet "`return` pada top level program (frame utama, di luar semua function) mengakhiri program; nilai return diabaikan karena frame tersebut tidak memiliki pemanggil, dan eksekusi berhenti pada titik return."

**Implementasi (Job 032):** `execute_return` + `complete_pending_return` men-set `state = Completed` saat frame non-task terakhir ter-pop (src/vm/mod.rs); `run_module_range` break pada Completed (tanpa re-execute dari pc 0); `run_modules` break dari loop module dan melewati export capture. Bukti: `test_job032_top_level_return_module_path` / `_run_path` / `_bare_return` / `_runs_finally` / `test_job032_function_return_unchanged` (5 unit) + `tests/zero032_top_level_return.rs` (`zsc_top_level_return_exits_0_with_output`, `zsc_eval_top_level_return_exits_0`); suite → 968 passed, 0 failed. Terkait: kandidat gap parser `return\n<stmt>` dicatat di CHECK.md (Job 032 discovered).

### 71h. Flag execution limits di `zsc` — **Resolved (2026-10-12, keputusan user + spec Job 033, implementasi Job 034)**

§71g sebelumnya mencatat bahwa `zsc` sengaja tanpa flag limits karena spec tidak mendefinisikan apapun. Kebutuhan flag diungkapkan sebagai gap (CHECK.md:484, README "Disclosed gaps"). **Keputusan user (2026-10-12, "approve apa adanya"):** lima flag `--max-instructions` / `--time-limit` / `--max-memory-slots` / `--max-call-depth` / `--max-tasks`, hanya dikenali sebelum kode/mode (argumen setelah kode tetap argumen program per §52/§100), nilai 0/bukan-angka = usage error exit 1. Ditulis sebagai blok "Flag Execution Limits" di Dependencies.md §100 (surface CLI; §150 tetap sumber aturan limits). **Evidence (Job 034):** `tests/zero034_zsc_limits_flags.rs` (8 tests: breach instruction/time/call-depth, invalid/zero value, flags setelah kode = argumen program, mode file/eval/run); suite 976 passed, 0 failed. Diimplementasikan di `src/main.rs` (`split_limits_flags`, `set_limits` di `run_file`/`cmd_eval`/`cmd_run`).

### 75f. `e.location` runtime error — **Resolved (2026-10-13, keputusan user + Job 036)**

`Language.md:1020` menjanjikan `e.location` = object `{file, line, column}` atau `null`; `Runtime.md` §154 menyatakan error "**dapat memiliki** source location ... **jika informasi tersebut tersedia**" (opsional, bukan gap compliance). Sebelum Job 036 location selalu `null` karena VM tidak menyimpan posisi sumber.

**Keputusan (user, 2026-10-13, dikunci):**
1. `file` = path input apa adanya untuk entry (tanpa kanonisasi); kanonik hanya untuk dependency resolution (ModuleMeta.display_path).
2. Posisi = token pemicu operasi (span `self.current_span()` di parser: `[`, `.`, `(`, operator biner, `=`, `+=`, `**`); parser Binary/Index/Member/Call sebelumnya memakai span left-expr — sudah di-patch.
3. Lookup memakai pc instruksi yang **gagal** (`instr_pc`), bukan pc+1; binary-search exact-match pada side table sparse (hanya instruksi yang di-`emit_located`).
4. Tanpa metadata (pecahan bytecode lama, task fail path) → `location` = `null`; jangan mengarang posisi.
5. Side table di-serialize ke `.zsbc`; format version 1 → **2**. File v1 ditolak dengan pesan `Unsupported bytecode version: 1` (exit non-zero), tanpa panic. `SUPPORTED_BYTECODE_VERSIONS = [2]`.

**Implementasi (Job 036):**
- `compiler.rs`: `BYTECODE_VERSION=2`; `pub struct DebugEntry {pc, line, column: u32}`; `Compiler.emit_located()`; field `debug_entries` / `function_bodies_debug` / `default_bodies_debug` / `source_file`; swap 4-site saat body compile; `FunctionInfo` + `source_file`/`debug`/`default_debug`; `BytecodeProgram.debug`; `ModuleUnit.debug`; `merge_modules` offset main debug pc + stamp `f.source_file = unit.path`.
- `parser.rs`: span token pemicu (Binary/Assignment/Compound/Call/Member/Index/Power).
- `module.rs`: `ModuleMeta.display_path`; `finish(loader, entry_display, entry_canon)`.
- `bytecode_io.rs`: `put_debug_entry`/`get_debug_entry`; FunctionInfo/ModuleMeta/BytecodeProgram serialize +deserialize field baru.
- `vm/mod.rs`: field `active_debug`/`active_source`/`current_error_loc`/`pending_error_loc`; `swap_instructions(instr, debug, source)`; `lookup_debug_loc(pc)`; `note_error_pc(instr_pc)` dipanggil di 3 run-loop + task step loop; `make_error_object` mengisi location object plain (`HeapObject::object`, bukan error_object) atau `Null`; 2-site `pending_error_loc` companion; `run_modules` set `active_source = meta.display_path` per module; execute_call x2 capture return_debug/source + swap; unwind/complete/return restore; evaluate_default_body swap default_debug.
- `vm/types.rs`: `Frame.return_debug`/`return_source`; `TaskContext.debug`/`source_file`.
- `vm/task.rs`: prepare_task_start / save_active_context / load_context membawa debug/source.

**Perubahan test lama (expected):** `test_job059a_e_fields_type_message_location` (null → object line/column), `test_job059a_print_e_shows_object` (`location: null` → `location: {file:`), `test_job019_unsupported_version_constant` (contains 1 → contains 2), `test_job024_foreign_opcode` payload hand-built v1 → v2 + debug vec.

**Evidence (Job 036):** `tests/zero036_error_location.rs` (9 tests: 5 kategori IndexError/KeyError/DivisionError/TypeError/ReferenceError, `location.file` = path verbatim termasuk `./`, roundtrip `.zsbc` location tetap, file v1 rejected with clear message, unit debug-table sparse sorted); suite → **987 passed, 0 failed**; warnings tetap baseline 2.

### 75g. Bare return boundary — **Resolved (2026-10-14, keputusan user opsi A + Job 038)**

`Language.md` §26 melarang statement setelah `return` dieksekusi, tapi parser memperlakukan newline sebagai whitespace: `return\nprint("x")` terparse sebagai `return (print("x"))` — print **dieksekusi** sebagai nilai return (kandidat gap CHECK.md sejak Job 032; pre-existing, diverifikasi identik di binary lama).

**Keputusan (opsi A, 2026-10-14):** nilai `return` hanya diparsing jika ekspresinya dimulai pada baris yang sama dengan kata kunci `return` (restricted production, gaya Rust/Swift). Token pertama di baris berikutnya = bare return; statement berikutnya statement baru. Opsi B (newline = batas semua statement global) ditolak terlalu invasif; opsi C (ubah §26 agar legal) ditolak kontradiktif dengan aturan RETURN.

**Revisi spec (additive):** `Language.md` §26 — paragraf "Batas statement untuk `return`" setelah "Code setelah `return` valid tetapi tidak pernah dieksekusi".

**Implementasi (Job 038):** `parse_return` (src/parser.rs) — cek `current().line > return_line` → value = None. Ekspresi lintas baris tetap jalan selalu dimulai di baris return (`return 1 +\n 2`, list literal, paren — expression parser tidak peduli baris). `;` tetap ditolak di seluruh bahasa (bukan bug `return` terpisah).

**Evidence:** `test_job038_return_*` (3 unit parser) + `tests/zero038_bare_return.rs` (8 CLI: function/top-level bare return, null, same-line, trailing-operator, list, paren, recursion §27); suite → **1008 passed, 0 failed**.

### 75h. Multi-argumen `print()` / `print.err()` — **Resolved (2026-10-14, keputusan user + Job 039)**

Sebelumnya argumen ekstra `print(1, 2, true)` di-drop diam-diam (hanya `args[0]` yang dicetak) — spec hanya mencontohkan single-argumen dan tidak pernah menetapkan arity, tercatat sebagai candidate gap CHECK.md ("Multi-arg print / full print.* API").

**Keputusan (user, 2026-10-14):** `print()` dan `print.err()` menerima 0 atau lebih argumen; tiap argumen diformat lalu digabung dengan **satu spasi**; hasil = satu baris + newline; tanpa argumen = baris kosong; aturan sama untuk keduanya.

**Revisi spec (additive):** `StandardLibrary.md` §44 "Argumen" — contoh `print("a", 1, true)` → `a 1 true`, `print.err("gagal:", 404)` → `gagal: 404`, kalimat pemisah spasi tunggal untuk keduanya.

**Implementasi (Job 039):** kedua arm `call_native` (src/vm/mod.rs) memetakan `format_value` (print.err: `error_message` dulu per §59a) lalu `join(" ")`; kasus 0 argumen menghasilkan "" seperti sebelumnya. Behavior single-argumen identik byte-per-byte (join 1 elemen = elemen itu).

**Evidence:** `tests/zero039_print_multi_arg.rs` (8: join campuran tipe, tanpa argumen, single-arg unchanged, string berspasi utuh, print.err join, error object + argumen ekstra, print.err kosong); suite → **1016 passed, 0 failed**.
