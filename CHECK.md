# ZeroScript Implementation Audit

Generated: 2026-09-22 (Job 008 update: 2026-09-23; Job 009 update: 2026-09-23; Job 010 update: 2026-09-24; Job 011 update: 2026-09-24; Job 012 update: 2026-09-24; Job 013 update: 2026-09-24; Job 014-016 updates: 2026-09-25; Job 017 update: 2026-09-26)
Tests: 727 passed, 0 failed (Job 017 baseline 623; +93 unit tests = 716 lib tests; +11 CLI integration tests in tests/zero017_zpm_cli.rs = 727)
Source files audited: src/*.rs (16 files — lib.rs, package.rs, version.rs added Job 017) + src/bin/zpm.rs + tests/zero017_zpm_cli.rs
Spec files audited: Language.md, Bytecode.md, ZeroVM.md, Runtime.md, Modules.md, Async.md, StandardLibrary.md, Dependencies.md, INDEX.md

---

## Status Legend

- **Implemented** — Feature works per spec, evidence in code and tests
- **Partially Implemented** — Feature exists but incomplete or deviates from spec
- **Not Implemented** — No code, no AST nodes, no opcodes
- **Unknown** — Cannot determine without runtime testing

---

## 1. Lexer (token.rs, lexer.rs)

| Feature | Spec | Status | Evidence |
|---|---|---|---|
| All 19 keywords (let, const, function, return, if, else, while, loop, break, continue, switch, case, default, try, catch, finally, and, or, not) | §2-§43 | Implemented | token.rs:15-33 |
| All 22 operators (+, -, *, /, %, **, =, ==, !=, ===, >, <, >=, <=, +=, -=, *=, /=, %=, ++, --, !) | §8-§15 | Implemented | token.rs:36-57 |
| Arrow `=>` | §32 | Implemented | token.rs:60 |
| Literals (number, string, boolean, null) | §6 | Implemented | token.rs:4-8 |
| `//` comments (silently skipped) | §1 | Implemented | lexer.rs:311-316 |
| `&&` / `||` symbolic operators | §16 | Implemented | lexer.rs:131+ — lex to TokenKind::And/Or; test_job008_ampersand_and_alias |

---

## 2. Parser (parser.rs, ast.rs)

| Feature | Spec | Status | Evidence |
|---|---|---|---|
| `let` / `const` declarations | §2-§3 | Implemented | parser.rs:41 |
| `function` declarations with params, defaults | §24-§30 | Implemented | parser.rs:62 |
| `return` | §26 | Implemented | parser.rs:78 |
| `if` / `else if` / `else` | §17 | Implemented | parser.rs:95 |
| `while` | §19 | Implemented | parser.rs:135 |
| `loop` | §20 | Implemented | parser.rs:149 |
| `break` / `continue` | §21-§22 | Implemented | parser.rs:158-164 |
| `switch` / `case` / `default` | §23 | Implemented | parser.rs:170 |
| `try` / `catch` / `finally` | §43 | Implemented | parser.rs:208 |
| Expression statement | — | Implemented | parser.rs:239 |
| Ternary `? :` | §18 | Implemented | parser.rs:248 |
| Assignment `=` | §13 | Implemented | parser.rs:268 |
| Compound assignment (`+=` etc.) | §12 | Implemented | parser.rs:282 |
| Logical `or` / `and` | §16 | Implemented | parser.rs:297-315 |
| Equality `==`, `!=`, `===` | §14 | Implemented | parser.rs:333 |
| Comparison `>`, `<`, `>=`, `<=` | §15 | Implemented | parser.rs:357 |
| Arithmetic `+`, `-`, `*`, `/`, `%`, `**` | §8-§9 | Implemented | parser.rs:382-429 |
| Unary `-`, `+`, `!` | §10 | Implemented | parser.rs:447 |
| Postfix `++`, `--` | §11 | Implemented | parser.rs:483 |
| Function call, member, index | §25, §33-§39 | Implemented | parser.rs:513 |
| Primary (literals, identifier, arrow, list, object, function expr) | §29-§32 | Implemented | parser.rs:557 |
| `not` keyword as unary operator | §16 | Implemented | parser.rs:469 — matches `Bang | Not`; test_job008_not_non_boolean_error |

---

## 3. AST (ast.rs)

| Feature | Spec | Status | Evidence |
|---|---|---|---|
| 10 statement nodes (VariableDecl, FunctionDecl, Return, If, While, Loop, Break, Continue, Switch, Try, Expression) | §2-§43 | Implemented | ast.rs:1-60 |
| 16 expression nodes (NumberLiteral, StringLiteral, BooleanLiteral, NullLiteral, Identifier, Unary, Binary, Assignment, CompoundAssignment, Postfix, Call, Member, Index, List, Object, Function, ArrowFunction, Ternary) | §6-§54 | Implemented | ast.rs:62-168 |
| `BinaryOp::RawEqual` for `===` | §14 | Implemented | Renamed from StrictEqual (was misleading). ast.rs:193; behavior unchanged: `10 === "10"` true, `10 == "10"` false (test_job008_raw_eq_still_true_for_number_string) |

---

## 4. Compiler (compiler.rs)

| Feature | Spec | Status | Evidence |
|---|---|---|---|
| All core opcodes (LoadConst, DeclareLocal, LoadLocal, StoreLocal, Add, Sub, Mul, Div, Mod, Pow, Neg, Pos, Not, Eq, Neq, RawEq, Gt, Lt, Gte, Lte, And, Or, Pop, Dup, Swap, Jump, JumpIfTrue, JumpIfFalse, Call, Return) | §127-§142 | Implemented | compiler.rs:5-70 |
| `MakeClosure`, `LoadCaptured`, `StoreCaptured` | §130 | Implemented | compiler.rs:63-65 |
| `MakeList`, `MakeObject`, `GetIndex`, `SetIndex`, `GetProperty`, `SetProperty` | §140-§144 | Implemented | compiler.rs:56-62 |
| `Print` (non-spec built-in) | — | Implemented | compiler.rs:68 |
| `Halt` | — | Implemented | compiler.rs:67 |
| Constant pool (Number, Str, Boolean, Null, Function) | §126 | Implemented | compiler.rs:5-11 |
| Short-circuit `and`/`or` via jumps | §139 | Implemented | compiler.rs:752-768 |
| Function body isolation (compile after main, swap at call) | — | Implemented | compiler.rs:388-436 |
| Capture detection (parent scope walk) | §130 | Implemented | compiler.rs:731-736 |
| Default parameter compilation | §30 | Implemented | compiler.rs:331-342 |
| Arrow function expression body | §32 | Implemented | compiler.rs:1099-1134 |
| `LOAD_GLOBAL` / `STORE_GLOBAL` | §127 | Implemented | Opcode::LoadGlobal/StoreGlobal (compiler.rs:14-15); DeclareGlobal(String, is_const) |
| `LIST_ADD` / `LIST_REMOVE` / `OBJECT_REMOVE` | §144 | **Not Implemented** | Delegated to stdlib only |
| `CALL_NATIVE` | §145-§146 | **Not Implemented** | No native call mechanism |
| `LOAD_FUNCTION` (standalone) | §141 | **Not Implemented** | Uses `LoadConst(Function(idx))` instead |
| Bytecode header / versioning | §124-§125 | **Not Implemented** | No version field |
| Bytecode validation pass | §124 | **Not Implemented** | No validator |
| `const` enforcement | §3 | Implemented | check_assign_const (compiler.rs:291); DeclareLocal/Global(name, is_const); store checks is_const; runtime Binding.is_const + mark_const_if_collection. Tests: test_job008_const_* |
| `else if` chains beyond depth 2 | §17 | Implemented | recursive compile_else_branch (compiler.rs:388,418,637); test_job008_else_if_chain_depth_gt_2 |
| `break`/`continue` outside loop validation | §21-§22 | Implemented | compiler.rs:710,720 — compile errors; saved_loop_contexts=take at function boundaries (505,1187,1295); tests break/continue_outside_loop + function_boundary |

---

## 5. ZeroVM Execution (vm.rs)

| Feature | Spec | Status | Evidence |
|---|---|---|---|
| PC, Value Stack, Call Stack | §115 | Implemented | vm.rs:342-352 |
| Execution loop (fetch, decode, execute) | §116 | Implemented | vm.rs:448-510 |
| Frame struct (function_index, locals, return_pc) | §117 | Implemented | vm.rs:332-340 |
| Frame.captured_env / scope_env | §130 | Implemented | vm.rs:337-338 |
| Frame scopes: Vec<HashMap<String, Binding>> | §3 | Implemented | vm.rs:270 Binding{value,is_const,initialized}; Frame.scopes push/pop via PushScope/PopScope |
| Function call (push frame, bind params, swap instructions) | §129, §141 | Implemented | vm.rs:928-996 |
| Missing arg → `null` | §25 | Implemented | vm.rs:961 |
| Extra arg → ERROR | §25 | Implemented | vm.rs:965-970 |
| Return (pop frame, restore pc, push result) | §142 | Implemented | vm.rs:1040-1108 |
| No return → `null` | §26 | Implemented | compiler.rs:177-179 |
| Closure capture (HeapRef to Environment on Heap) | §130 | Implemented | vm.rs:1128-1173 |
| `LoadCaptured` / `StoreCaptured` | §130 | Implemented | vm.rs:1261-1302 |
| `const` captured var immutability | §3 | Implemented | EnvBinding.is_const (heap.rs:26); StoreCaptured checks; test_job008_captured_const_mutation_error |
| Self-reference in closures | — | Implemented | vm.rs:974-976 |
| All arithmetic opcodes | §136 | Implemented | vm.rs:629-710 |
| All comparison opcodes | §138 | Implemented | vm.rs:541-580 |
| Boolean logic opcodes | §139 | Implemented | vm.rs:778-839 |
| `print` built-in | — | Implemented | vm.rs:488 |
| Error propagation (RuntimeError) | §150 | Implemented | vm.rs:1190-1220 |
| Shared reference semantics for lists/objects | §118 | Implemented | Value::List(HeapRefHandle)/Object(HeapRefHandle) (vm.rs:16-17); heap-backed; shared on assign/arg-pass. Tests: test_job008_*_shares_reference, *_passed_to_function_mutation_visible, shared_reference_via_raw_eq |
| SetIndex / SetProperty mutation | §118 | Implemented | execute_set_index/execute_set_property (vm.rs:1707,1741) mutate heap via get_mut; new keys OK; const-guard rejects. Tests: test_job008_set_index_mutates_heap, set_property_mutates_heap_existing/new_key |
| `try`/`catch`/`finally` runtime dispatch | §43 | Implemented | TryPush/TryPop/FinallyEnd; handler stack; raise_runtime_error; test_job009_try_catch_*, finally_*, nested_*, return_from_try |
| Value stack per frame (spec §116) | §116 | **Not Implemented** | Single shared stack across all frames |
| Execution limits (instruction count, time, call depth) | §150 | **Not Implemented** | Spec says optional |

---

## 6. Runtime Values (runtime_value.rs)

| Feature | Spec | Status | Evidence |
|---|---|---|---|
| RuntimeValue::Number, Boolean, Null (immediate) | §119 | Implemented | runtime_value.rs:6-9 |
| RuntimeValue::HeapRef (heap-backed) | §119 | Implemented | runtime_value.rs:10 |
| `runtime_type_name()` for all types | §151-§156 | Implemented | runtime_value.rs:14-27 |

---

## 7. Heap (heap.rs)

| Feature | Spec | Status | Evidence |
|---|---|---|---|
| HeapObject::Str, Number, Boolean, Null | §118 | Implemented | heap.rs:26-35 |
| HeapObject::List, Object | §118 | Implemented | heap.rs:36-37 |
| HeapObject::Function | §118 | Implemented | heap.rs:38-43 |
| HeapObject::Environment | §130 | Implemented | heap.rs:44 |
| HeapObject::Closure | §118 | Implemented | heap.rs:45 |
| Slot-based allocation with free list | — | Implemented | heap.rs:8-20 |
| Generation-based invalidation (HeapRef) | — | Implemented | heap.rs:58-62 |
| get / get_mut / deallocate | — | Implemented | heap.rs:64-98 |
| mark / is_marked / clear_marks / sweep | §121 | Implemented | heap.rs:100-180 |
| child_refs (for GC tracing) | §121 | Implemented | heap.rs:182-220 |
| get_env_binding / set_env_binding | §130 | Implemented | heap.rs:222-244 |
| HeapRefHandle (Drop = deallocate) | — | Implemented | heap.rs:251-268 |
| to_runtime_value / from_runtime_value | — | Implemented | heap.rs:270-292 |

---

## 8. Garbage Collector (gc.rs)

| Feature | Spec | Status | Evidence |
|---|---|---|---|
| Mark-and-sweep algorithm | §121 | Implemented | gc.rs:20-75 |
| Root identification | §121 | **Partially Implemented** | gc.rs:31-36 — live_refs only; local vars and params not scanned |
| Sweep with generation bump | §121 | Implemented | gc.rs:68-75 |
| Threshold-based triggering | §121 | Implemented | gc.rs:80-82 |
| Cycle handling | §121 | Implemented | Tested in test_gc_collect_unreachable_cycle |
| 12 tests covering all scenarios | — | Implemented | gc.rs:84-337 |

---

## 9. Modules (Modules.md §46-§80)

| Feature | Spec | Status | Evidence |
|---|---|---|---|
| `import` / `export` syntax | §46-§47 | Implemented | token Import/Export; ast Import/ExportList + `exported` flag; parser parse_import/parse_export; compiler Import/ExportList arms; test_job012_basic_import_export, test_job012_export_list_form |
| Module path resolution | §64 | Implemented | module.rs `Loader::resolve_path` (relative to importer, auto `.zs`, dir/missing → ERROR); bare paths (`pkg`, `pkg/sub`) route to package resolution instead of NotFound (§81, Job 017 — see §12); test_job012_path_resolution_*, test_job012_directory_import_error, test_job012_module_not_found, test_job017_bare_path_is_package_not_local_module |
| Module scope / encapsulation | §65, §75-§76 | Implemented | compile_module scopes [placeholder, module], function_base=1 → DeclareLocal; runtime per-module scope push/pop + export cache; test_job012_private_not_accessible, test_job012_isolation_distinct_exports, test_job012_same_name_no_conflict |
| Module initialization order | §71-§72, §78 | Implemented | DFS post-order topo in module.rs load; EndModule ranges; test_job012_init_order, test_job012_init_order_nested, test_job012_import_mid_file_static, test_job012_init_once_two_imports_diff_names |
| Circular dependency detection | §62, §77 | Implemented | gray-stack DFS; ModuleError::Circular; test_job012_circular_direct, test_job012_circular_indirect, test_job012_self_import |
| Export not found / case-sensitive | §63 | Implemented | load-time check against dep.exports_set; test_job012_export_not_found, test_job012_export_case_sensitive |
| Duplicate import binding | §79 | Implemented | declare_variable on import names; test_job012_duplicate_import_same_binding, test_job012_duplicate_import_diff_bindings |
| Import not at module level | §78 | Implemented | parser module_level flag; test_job012_import_inside_{function,if,while}_error |
| Module evaluation failure | §73 | Partially Implemented | failed module not cached (run_modules returns Err, scope dropped); §73 vs §78 try/catch interaction documented ISSUES.md §62 — not in-language catchable under static import |
| Import alias (`as`) | §69 | Implemented | parser.rs parse_import (export_name, local_name) pairs; compiler declare_variable(local, true); test_job013_import_alias_basic, test_job013_alias_conflicts_with_local, test_job013_alias_avoids_duplicate_local, test_job013_multiple_imports_partial_alias |
| Re-export | §67 | Implemented | parser ExportList.from; compiler record_re_export; module.rs dep resolve + ExportNotFound + ExportConflict guards; vm.rs run_modules copies ExportBinding (Live chains); test_job013_re_export_barrel, test_job013_re_export_missing_source, test_job013_re_export_missing_export |
| Export conflict detection | §68 | Implemented | compiler record_export/record_re_export duplicate check; module.rs re-export list guard ModuleError::ExportConflict; test_job013_duplicate_export_local_vs_reexport, test_job013_duplicate_export_two_reexports |
| Live binding | §66, §74 | Implemented | vm.rs ExportBinding::{Value,Live}; Binding.live HeapRef to source Environment; StoreCaptured syncs non-live Binding.value; StoreLocal write-through to shared scope_env; test_job013_live_binding_counter (§66 exact: 0 then 1), test_job013_assign_to_imported_{let,const}_error |

---

## 10. Async / Task (Async.md §49, §101-§113)

| Feature | Spec | Status | Evidence |
|---|---|---|---|
| `async function` / `await` | §49 | Implemented | lexer keywords Async/Await (test_async_keywords); ast Stmt::FunctionDecl.is_async + Expr::Await; parser async decl + await expr (test_async_function_declaration incl. export async, test_await_expression); compiler `in_async` gate → compile ERROR `'await' is only allowed inside an async function` (test_job014_await_outside_async_is_compile_error: top level, sync fn, nested sync fn); Opcode::Await + vm execute_await (test_job014_await_returns_result §107 example → 30) |
| Task states (pending, running, completed, failed, cancelled) | §106 | Implemented | vm.rs TaskState{Pending,Running,Completed,Failed,Cancelled} (vm.rs:326) + Value::Task(id) + HeapObject::Task; all five states observed: pending/running/completed/failed (test_job014_task_state_transitions), cancelled reachable since Job 015 cancel_task (vm.rs:1900; test_job015_cancel_running_task → running→cancelled, test_job015_cancel_completed_noop) — ISSUES.md §63f closed by Job 015 |
| `task.all()`, `task.race()` | §103, §104 | Implemented | builtins vm.rs:2783 (all) / vm.rs:2793 (race) + TaskKind::All/Race + notify_dependents/reevaluate_synthetic; input-order results (test_job015_all_results_input_order), empty list (test_job015_all_empty_list), fail-fast + sibling untouched (test_job015_all_fail_fast), non-task → ERROR (test_job015_all_non_task_error), first finisher wins w/ losers uncancelled (test_job015_race_first_finisher_wins), first failure forwarded (test_job015_race_first_finisher_error), race([]) → ERROR (test_job015_race_empty_error) |
| `task.cancel()`, `task.state()` | §105, §106 | Implemented | task.state builtin (vm.rs:2774) + property, non-task → ERROR (test_job014_task_state_non_task_error); task.cancel builtin (vm.rs:2809) → cancel_task (vm.rs:1900): running → cancelled (test_job015_cancel_running_task), completed → no-op (test_job015_cancel_completed_noop), await of cancelled → cancellation error (test_job015_await_cancelled_is_error), non-task → ERROR (test_job015_cancel_non_task_error), self-cancel → ERROR (test_job015_self_cancel_is_error, ISSUES.md §64b) |
| `task.wait()`, `task.timeout()`, `task.sleep()` | §109, §110, §111 | Implemented | builtins vm.rs:2819/2829/2843 + TaskKind::Wait/Timeout/Sleep + wake_due_timers (vm.rs:2055) on virtual clock; wait on completed/failed/cancelled (test_job015_wait_terminal_states), timeout success (test_job015_timeout_success), timeout → ERROR without cancelling (test_job015_timeout_error_task_keeps_running), §112 sleep ordering (test_job015_sleep_ordering_not_creation_order), sleep(0) yields (test_job015_sleep_zero_yields), negative duration → ERROR (test_job015_sleep_negative_error) |
| Task scheduler | §112 | Implemented | Job 015 replaced drain_pending_tasks with full cooperative `run_scheduler`/`run_task_step` in run()/run_modules(): round-robin FIFO ready_queue, per-task swapped context (TaskContext), suspension via Task.wait, virtual monotonic clock advanced to earliest deadline only when idle; §112 example logic (test_job015_sleep_ordering_not_creation_order → B before A), non-blocking sleeps (test_job015_timeout_error_task_keeps_running), §108 un-awaited failure still holds (test_job014_unawaited_failure_does_not_stop_program), await-cycle guard → deadlock ERROR (test_job015_await_cycle_deadlock_error, ISSUES.md §64c); §112 top-level-await example executed via main() wrapper (ISSUES.md §63a/§64a) |
| Concurrent execution | §102 | Implemented | tasks created before any await progress concurrently under the scheduler; §102 example logic (test_job015_concurrent_creation_before_await: dataTask 10 ms completes while userTask 60 ms still awaited); results stay separate, sibling failure does not cancel others (test_job015_all_fail_fast) |

Job 014 behavior tests (vm.rs): test_job014_async_call_creates_pending_task, test_job014_await_outside_async_is_compile_error, test_job014_await_returns_result, test_job014_await_failed_task_catchable (§108), test_job014_task_state_transitions (§106), test_job014_await_twice_does_not_rerun (§113), test_job014_nested_async_calls, test_job014_async_recursion, test_job014_task_state_non_task_error (§106), test_job014_await_non_task_is_error (ISSUES §63b), test_job014_unawaited_failure_does_not_stop_program (§108), test_job014_try_catch_inside_task_body (§49), test_job014_second_await_of_failed_task_reraises (§107/§113), test_job014_async_closure_capture.

Job 015 behavior tests (vm.rs): test_job015_concurrent_creation_before_await (§102), test_job015_all_results_input_order, test_job015_all_empty_list, test_job015_all_fail_fast, test_job015_all_non_task_error (§103), test_job015_race_first_finisher_wins, test_job015_race_first_finisher_error, test_job015_race_empty_error (§104), test_job015_cancel_running_task, test_job015_cancel_completed_noop, test_job015_await_cancelled_is_error (§105), test_job015_wait_terminal_states (§109), test_job015_timeout_success, test_job015_timeout_error_task_keeps_running (§110), test_job015_sleep_ordering_not_creation_order, test_job015_sleep_zero_yields (§111, §112), test_job015_cancel_non_task_error, test_job015_self_cancel_is_error (ISSUES §64b), test_job015_await_cycle_deadlock_error (ISSUES §64c), test_job015_sleep_negative_error.

---

## 11. Standard Library (StandardLibrary.md §42-§52, §58-§60)

| Feature | Spec | Status | Evidence |
|---|---|---|---|
| `print()` | §42 | Implemented | call_builtin "print" → format_value (all types incl. list/object/function); test_job010_print_* |
| `print.err()` | §42 | Implemented | execute_builtin "print.err" → err_output; works outside catch; test_job009_print_err_*, test_job010_print_err_* |
| `string.*` (length, contains, trim, split, replace, startsWith, endsWith, lower, upper, indexOf) | §42, §58 | Implemented | call_builtin string.* arms; type-error guards; test_job010_string_* (62 tests) |
| `list.*` (add, remove, length) | §34 | Implemented | call_builtin list.* arms; const + shared-ref checks; test_job010_list_* |
| `object.*` (remove) | §39 | Implemented | call_builtin object.remove; const + missing-key ERROR; test_job010_object_remove_* |
| `color.*` (red, green, yellow, blue, bold) | §45 | Implemented | call_builtin color.* arms (vm.rs); GetProperty known color methods; test_job011_color_* (8 tests incl. bold nesting + catchable) |
| `file.*` (read, write, exists, remove, mkdir) | §48 | Implemented | call_builtin file.* arms → std::fs, IOError → RuntimeError catchable; test_job011_file_* (7 tests incl. try/catch) |
| `http.*` (get, post, put, patch, delete) + query/headers/timeout (§50, §50.1, §59, §60) | §50, §50.1, §59, §60 | Implemented | Cargo.toml: ureq 3.4.2; call_builtin http.* arms → http_request (options validation §59/§60, percent-encode query, worker thread, TaskKind::Http slot) + scheduler poll/park (poll_http_completed, http_inflight); response object {status, headers, body} via build_http_response; tests: test_job016_http_get_basic_response, _methods_with_body_and_headers, _body_never_auto_parsed, _4xx_5xx_are_normal_responses, _query_encoding, _query_non_object_error, _request_headers_sent_as_given, _headers_non_object_error, _header_value_non_string_error, _timeout_is_catchable_error, _network_failure_catchable, _json_stringify_body_roundtrip, _options_non_object_error (local mock server; ISSUES.md §65) |
| `json.*` (parse, stringify) | §51 | Implemented | src/json.rs (parser+stringifier, 0 deps); call_builtin json.parse/stringify; json_value_to_value / value_to_json_value (vm.rs); test_job011_json_* (10 tests) + json::tests (7) |
| `system.*` (args, env, os, arch) | §52 | Implemented | call_builtin system.* arms; ZeroVM.program_args + set_program_args; test_job011_system_* (7 tests incl. args exclude program name, env null) |
| `type()` (Type Introspection, §53) | §53 | Implemented | type_name() builtin; test_job009_type_* |
| `number()`, `string()`, `boolean()` (Type Conversion, §54) | §54 | Implemented | call_builtin number/string/boolean; success+error tests (test_job009_number_*, string_*, boolean_*) |

---

## 12. Dependencies (Dependencies.md §81-§100)

| Feature | Spec | Status | Evidence |
|---|---|---|---|
| `project.zson` manifest | §82 | Implemented | src/package.rs `parse_manifest` (name/version required strings, dependencies object, ZSON via `json::parse_zson`); test_job017_manifest_* (9: minimal, §82 example, missing field, wrong type, root not object, invalid selector, unsafe dep name, scripts validation, unknown fields ignored); not executable / not importable (test_job017_project_zson_not_importable) |
| `project.lock.zson` lockfile | §84 | Implemented | `parse_lockfile`/`render_lockfile` (§84 shape `{ packages: { "n": "v" } }`, bare `packages` key, 4-space indent); Job 018 adds §97's object entry `{ version, source, commit, hash }` when identity fields exist, plain string form otherwise (src/package.rs:516), and §90's `name@version` keys for multi-version installs (`Lockfile::entry`/`entries_for` match both, src/package.rs:389); used when valid (test_job017_install_honours_lockfile, cli_install_honours_existing_lockfile), deleted → resolver picks newest (test_job017_lockfile_removed_resolves_newest), invalid → ignored and rewritten (test_job017_invalid_lockfile_is_ignored_not_fatal), stale selector entry → re-resolved (test_job017_stale_lockfile_entry_is_re_resolved), not importable (test_job017_lockfile_not_importable), reproduced byte-identically on reinstall (job018_locked_resolution_reproduces_on_reinstall) |
| `package.zson` metadata | §96, §94 | Implemented | `parse_package_meta` (name + SemVer version required, dependencies optional, §96 "metadata tidak valid → ERROR"); validated on the installed copy at import time (module.rs `check_locked` — test_job017_lockfile_version_mismatch_is_error, test_job017_broken_installed_metadata_is_error, test_job017_missing_installed_metadata_with_lock_is_error); registry-side metadata mismatch rejected (test_job017_metadata_mismatch_is_error, test_job017_missing_package_zson_in_registry_is_error); not importable (test_job017_package_zson_not_importable); optional description/author/license/repository validated as strings (job018_optional_metadata_fields_are_accepted); consumed by registry/publish/git flows: `check_meta` (src/package.rs:1225) rejects a stored copy whose name/version differ from the requirement before use (job018_registry_copy_with_wrong_name_is_error, job018_git_source_with_wrong_package_name_is_error) |
| `zpm` package manager | §85, §86 | Implemented | src/bin/zpm.rs: `install`, `run <script> [args]`, `--registry`, `--help` (exit 0) / unknown command (exit 1); tests/zero017_zpm_cli.rs (11 end-to-end tests against the real binaries). Job 018 adds `zpm install [name|git:<url>[#ref]]` (declares the dependency first, §100), `remove <package>` (src/bin/zpm.rs:137), `update` (src/bin/zpm.rs:153), `publish` (src/bin/zpm.rs:168), repeatable `--registry` (§95: flags > `$ZPM_REGISTRY` > `<project>/registry`, src/bin/zpm.rs:53); tests/zero018_zpm_cli.rs (8 end-to-end tests) |
| `.zsc_m/` dependency directory | §82, §85, §87, §90 | Implemented | Flat one-copy-per-package layout written by `install()`; §90 multi-version layout `versions/<v>/` written/pruned by `materialize`+`prune` (src/package.rs:2255/2375, `is_versioned_layout`); read by module.rs `Loader::resolve_package` (versioned target → scope lookup, src/module.rs:208-350); test_job017_install_full_flow, cli_install_writes_zsc_m_and_lockfile, test_job017_package_import_resolves_in_zsc_m, test_job018_disjoint_selectors_resolve_to_two_versions |
| Git package source | §93 | Implemented | src/git.rs: `GitSource::parse`/`spec` (`git:<url>[#ref]`), `checkout` (clone + ref checkout via `--end-of-options`, reads package.zson, records `rev-parse HEAD`), `checkout_commit` (locked commit pinned first, src/git.rs:153); resolution treats a `git:` dependency as its own §99 group and records `source`+`commit` in the lockfile (`InstallPool::fetch_git`, src/package.rs:1377+); unreachable/invalid repo → ERROR (job018_git_source_unreachable_is_error), locked commit honoured (job018_git_dependency_installs_and_records_commit), CLI `zpm install git:...` (cli_install_git_source_records_commit) |
| Package registry | §95 | Implemented | Local filesystem registry (minimum implementation of §95 — no HTTP service, see ISSUES.md), layout mirrors §95 (`<root>/<name>/<version>/` + `package.zson`): `Registry::versions`/`package_dir`/`source` (src/package.rs:741+); optional `<name>/index.zson` per §95's index (version → `source` string or `{ source, hash }`, Git or relative path; invalid index → ERROR, src/package.rs:777+); multiple registries — official + additional (`Sources.registries` order, src/bin/zpm.rs:53); selectors choose the version, result locked (§95); job018_additional_registry_supplies_missing_package, job018_invalid_registry_index_is_error, job018_package_unknown_to_every_registry_is_error, test_job017_install_* |
| Version resolution (SemVer) | §83 | Implemented | src/version.rs `Version` (MAJOR.MINOR.PATCH, Ord) + `Selector` {Exact, `N.M.x`, `N.x`, `x`} + `highest_matching`; range operators `^ ~ >=` rejected (test_job017_manifest_invalid_selector_rejected_at_parse); selector matrix (test_job017_install_selector_matrix); no match → ERROR (test_job017_install_no_version_matches_selector_error) |
| Package integrity verification | §97 | Implemented | `hash::hash_dir` (SHA-256, `.git` excluded, src/hash.rs:160) + `verify_integrity` (src/package.rs:1258): recorded hash (lockfile §97 rule first, else registry index) compared with the computed one, mismatch → ERROR and nothing installed; `Pool::hash` reads index hashes, Git packages record `source`+`commit`+`hash` (§97 lockfile example); job018_index_hash_is_verified_and_recorded, job018_index_hash_mismatch_is_error |
| `.zsc_cache/` package cache | §98 | Implemented | `Cache` (src/package.rs:1007+): per-project/`$ZPM_CACHE`/`$HOME/.zsc_cache` root, `<name>/<version>/` + `<version>.sha256` sidecar, `entry()` refuses content whose digest no longer matches (§98 "cache tidak valid tidak boleh digunakan"); `store()` runs after every verified acquisition (registry copy and Git checkout); staging dirs never pollute the cache; clearing the cache leaves manifest/lock/`.zsc_m` untouched; job018_valid_cache_is_used_when_registry_copy_is_gone, job018_corrupt_cache_is_not_used, job018_clearing_cache_leaves_project_untouched |
| Package import resolution (bare path → `.zsc_m/`) | §81, §87 | Implemented | module.rs `Loader::resolve_package`: path without `./`/`../` → package; `pkg/sub` → `.zsc_m/pkg/sub.zs`, bare `pkg` → `.zsc_m/pkg/index.zs`; NOT installed → ERROR (test_job017_package_not_installed_is_error), not declared → ERROR (test_job017_package_not_declared_is_error), missing module / directory import → ERROR; reuses the single Job 012 loader (`visit`/`Loader`), no second loader |
| Package boundary | §81 | Implemented | canonicalized target must stay under `.zsc_m/<pkg>/` (module.rs resolve_package + resolve_module); test_job017_package_cannot_escape_its_boundary; internal relative import still works (test_job017_package_internal_relative_import_works) |
| `zpm run` project scripts | §86 | Implemented | package.rs `run_script`: `sh -c '<command>'` from project root, extra args appended shell-quoted, child exit code propagated; unknown script → ERROR (test_job017_run_script_unknown_name_is_error, cli_run_unknown_script_is_error); §86 example `start: "zsc index.zs"` runs end-to-end (test_job017_run_script_example_shape_with_zsc, cli_run_script_then_package_import_works) |
| Dependency tree / nested dependencies | §88 | Implemented | `install()` requirement accumulation + breadth-first expansion, shared package installed once (test_job017_nested_dependency_tree, test_job017_shared_dependency_installed_once, test_job017_shared_version_intersects_selectors), no satisfying version → ERROR (job018_selector_without_matching_version_is_error; supersedes test_job017_unresolvable_version_conflict_is_error, which expected disjoint selectors to fail before §90 existed), circular → ERROR (test_job017_circular_package_dependency_is_error, test_job017_self_dependency_is_circular), init order depth-first (test_job017_nested_package_init_order). Multi-version conflict from §88's last example → §90 row below |
| Dependency scope | §89 | Implemented | Visible set = project's own `dependencies` for project modules, the owning package's `package.zson` dependencies for package modules (module.rs `resolve_package` + `package_dependencies`); project cannot import transitive dep (test_job017_project_cannot_import_transitive_dependency), package can import its own (test_job017_package_imports_its_own_dependency), cannot import another package's dep (test_job017_package_cannot_import_undeclared_package), installed-but-transitive is not global (test_job017_installed_transitive_package_is_not_global); no dev/runtime categories invented (§89 defines none). Job 018: §90 versioned installs — owner scope taken from `versions/<v>/` path, installed/locked version, or `PROJECT_SCOPE` (module.rs `owner_scope_key`:307), visible set read from the owner's own directory (`package_dependencies` keyed by dir), scope graph recovered via `resolve_installed` (src/package.rs:2086) only when the target has multiple versions |
| Dependency conflict resolution | §90 | Implemented | `resolve_graph` fixpoint over `(scope, package, selector|git)` requirements (src/package.rs:2030, `place`:1789): selectors share one version when possible and split per scope-group when not, each group's highest matching version with a still-valid lock entry winning (no auto-upgrade, §92/§99); explicit `git:` demands form their own groups and never merge with selectors; `versions/<v>/` layout when a name keeps >1 version, flat otherwise; lock keys become `name@version`; scope graph checked for cycles after convergence (`check_scope_cycles`/`detect_cycle`); unresolvable → ERROR (no matching version); job018_one_version_shared_by_several_needs, job018_disjoint_selectors_resolve_to_two_versions (supersedes Job 017's flat-conflict-error test), job018_selector_without_matching_version_is_error, job018_locked_resolution_reproduces_on_reinstall |
| Dependency removal | §91 | Implemented | `zpm remove <pkg>` → `remove_dependency` (src/package.rs:2483): undeclare from `project.zson` (JSON surgery via `json::render_zson`, src/json.rs:80) → re-resolve → `prune` (src/package.rs:2375) removes packages no longer required while keeping still-needed transitive ones → lockfile rewritten; undeclared package → ERROR (`undeclare_dependency`:2427); source code untouched; job018_remove_prunes_only_unneeded_packages, job018_remove_undeclared_package_is_error, cli_remove_updates_manifest_and_prunes, cli_remove_undeclared_is_error |
| Dependency update | §92 | Implemented | `zpm update` → `update` (src/package.rs:2248): resolution without lockfile pinning (`InstallPool::for_update`) so the newest selector-matching version (transitives included) is chosen, then re-locked; `zpm install` never upgrades a locked version (test evidence below); selector no longer satisfiable → ERROR; job018_update_bumps_locked_version_install_does_not, job018_update_keeps_version_when_no_newer_exists, job018_update_selector_no_longer_satisfiable_is_error, cli_update_bumps_locked_version, cli_update_with_argument_is_usage_error |
| Package publishing | §94 | Implemented | `zpm publish` → `publish` (src/package.rs:2587): requires package.zson (walks up), validates metadata, refuses a version already in the registry (versions are final), copies the package into the local registry directory (minimum local implementation of §94's Git distribution — see ISSUES.md), and in a Git repo marks/releases the version with the `v<version>` tag (`git::create_tag`, tag exists → ERROR); job018_e2e_install_publish_reinstall_from_another_project, job018_publish_git_repo_marks_release_tag_and_is_final, job018_publish_without_package_zson_is_error, cli_publish_then_install_by_name |
| Package source priority | §99 | Implemented | Deterministic order in `resolve_graph`/`acquire` (src/package.rs:2106): lockfile while its source+package remain valid → explicit `git:` source (never consulted against a registry) → local cache (verified by sidecar digest) → registries in configuration order; priority never overrides selectors (version chosen before source); unreachable source with no valid alternative → ERROR; job018_explicit_git_source_wins_over_registry, job018_git_source_with_wrong_package_name_is_error, job018_valid_cache_is_used_when_registry_copy_is_gone |
| Package system completion | §100 | Implemented | Checklist pass over §81-§100 after Jobs 017+018: explicit dependency declaration (`add_dependency`:2497, `zpm install <name>`/`git:` declare first), deterministic conflict resolution (§90 row), reproducible lockfile installs (§84 row), integrity gate before use (§97 row), isolated project environments (§89/§91 rows), package manager never edits project source (§91 row), commands `install/remove/update/run` + `git:` install + `publish` (zpm.rs rows); cli_help_lists_all_commands, job018_e2e_install_publish_reinstall_from_another_project |

*(Job 017: rows above updated in place — spec citations corrected where they were off (lockfile §83→§84, package.zson §85→§96, zpm §81→§85/§86, registry §91→§95, SemVer §92→§83, integrity §93→§97, cache §88→§98, git §90→§93) — and 5 rows added (package import, boundary, scripts, dependency tree, scope). Ambiguities → ISSUES.md §66.)*

*(Job 018: rows above updated in place (git/registry/integrity/cache → Implemented, lockfile/metadata/zpm/.zsc_m/tree/scope rows extended with Job 018 evidence) — and 6 rows added (§90 conflict resolution, §91 removal, §92 update, §94 publishing, §99 source priority, §100 completion). Ambiguities → ISSUES.md §66-§67.)*

---

## 13. Error Handling (Language.md §43)

| Feature | Spec | Status | Evidence |
|---|---|---|---|
| `try` / `catch` / `finally` syntax | §43 | Implemented | parser.rs:208 |
| try/catch/finally runtime dispatch | §43 | Implemented | TryPush/TryPop/FinallyEnd opcodes; handler stack (TryHandler); raise_runtime_error; test_job009_try_catch_*, finally_*, nested_*, return_from_try |
| `print.err()` | §42 | Implemented | execute_builtin "print.err" → err_output; GetProperty print.err; test_job009_print_err_displays_error |
| `type()` (§53) | §53 | Implemented | type_name() builtin; all 7 types tested (test_job009_type_*) |
| Explicit conversion `number()`/`string()`/`boolean()` (§54) | §54 | Implemented | call_builtin number/string/boolean; success+error cases tested |
| RuntimeError propagation | §150 | Implemented | vm.rs — catchable VmError::RuntimeError via raise_runtime_error |
| Division by zero → ERROR | §7 | Implemented | catchable (RuntimeError) |
| Modulo by zero → ERROR | §7 | Implemented | catchable (RuntimeError) |
| Type mismatch → ERROR | §6 | Implemented | Throughout VM |
| Undefined variable → ERROR | §4 | Implemented | catchable (RuntimeError) |
| Parse/syntax error not caught | §43 | Implemented | test_job009_parse_error_not_caught |
| VM Error distinct from runtime error | §154 | Implemented | Only VmError::RuntimeError is catchable; Invalid* etc. abort |

---

## 14. Semantics Summary

| Domain | Status | Notes |
|---|---|---|
| Arithmetic (+, -, *, /, %, **) | Implemented | With div/mod by zero, NaN/Infinity checks |
| Null arithmetic (= 0) | Implemented | vm.rs:546-560 |
| String concatenation (+) | Implemented | vm.rs:640 |
| Comparison (>, <, >=, <=) | Implemented | Numbers only, reject non-numbers |
| Equality (==, !=, ===) | Implemented | ==: value+type; ===: raw value |
| Boolean (and, or, not) | Implemented | Short-circuit via jumps, non-boolean rejected |
| Ternary (? :) | Implemented | compiler.rs:1204 |
| Postfix ++ / -- | Implemented | Returns value AFTER change |
| Compound assignment (+=, -=, etc.) | Implemented | All 5 operators |
| if / else if / else | Implemented | Depth any (compile_else_branch recursive) |
| while | Implemented | |
| loop | Implemented | |
| break / continue | Implemented | Compile-time validation outside loop; function boundary isolation |
| switch / case / default | Implemented | No fall-through |
| Function declaration | Implemented | Named, anonymous, arrow |
| Function call | Implemented | Missing arg → null; extra arg → ERROR |
| Default parameters | Partially Implemented | Evaluated at compile time, not call time (Job 008 out of scope) |
| Recursion | Implemented | factorial(5)=120 passes |
| Nested functions | Implemented | |
| Closures (lexical capture) | Implemented | Shared env via HeapRef |
| Closure mutation shared | Implemented | test_closure_mutation_shared passes |
| Closure in data structures | **Bug** | heap_object_for_element creates new env, breaking shared state (Job 008 out of scope) |
| `let` mutable | Implemented | |
| `const` immutable | Implemented | Compile-time + runtime. check_assign_const; Binding.is_const; const collection mark. Tests: test_job008_const_* |
| Duplicate declaration | Implemented | declare_variable (compiler.rs:190,199); DeclareLocal/Global runtime check (vm.rs:569,662). All 4 §5 combos tested |
| Block-level scoping | Implemented | Frame.scopes: Vec<..>; PushScope/PopScope on block bodies. Tests: inner_block_variable_does_not_leak, outer_variable_readable_from_inner, shadowing_different_scopes_valid |
| Function scope assignment (§4) | Implemented | resolve_for_store create_local + register_implicit_local (compiler.rs:317,942,1011,1099). test_job008_function_scope_assignment_does_not_change_outer outputs 20,10 |
| Function reads global w/o capture | Implemented | LoadGlobal; test_job008_function_reads_global_without_capture |
| Boolean `&&`/`||`/`not` identical to `and`/`or`/`!` | Implemented | Same And/Or tokens; Not unary. test_job008_ampersand_and_alias, and_or_not_identical_to_keywords |
| No truthy/falsy (§16) | Implemented | JumpIfTrue/False reject non-boolean. test_job008_no_truthy_and/or_non_boolean, not_non_boolean_error |

---

## 15. Critical Bugs Found (Job 008 status)

| # | Bug | Location | Severity | Status |
|---|---|---|---|---|
| 1 | `const` variables silently mutable | Frame.is_const never populated | High | **Fixed** — Binding.is_const + check_assign_const + mark_const_if_collection; test_job008_const_* |
| 2 | SetIndex/SetProperty are no-ops | vm.rs:1424-1489 | High | **Fixed** — execute_set_index/set_property mutate heap; test_job008_set_* |
| 3 | Lists/objects cloned on assignment, no shared refs | Value::List/Object inline | High | **Fixed** — Value::List/Object(HeapRefHandle); test_job008_*_shares_reference |
| 4 | Default parameters evaluated at compile time, not call time | compiler.rs:331-342 | Medium | Remaining (out of scope Job 008) |
| 5 | `not X` keyword not parseable | parser.rs:469 | Medium | **Fixed** — Bang\|Not; test_job008_not_non_boolean_error |
| 6 | Closure cloning in data structures creates independent env | vm.rs:186-201 | Medium | Remaining (out of scope Job 008) |
| 7 | GC root coverage incomplete (local vars not scanned) | vm.rs:595-600 | Medium | Remaining (out of scope Job 008) |
| 8 | `===` / `==` semantics naming reversed in AST | ast.rs:191-193 | Low | **Fixed** — BinaryOp::RawEqual; behavior unchanged |
| 9 | `else if` chains limited to depth 2 | compiler.rs:514-517 | Low | **Fixed** — recursive compile_else_branch; depth>2 tested |
| 10 | Block-level variable scoping broken | vm.rs:595-600 (flat HashMap) | Medium | **Fixed** — Frame.scopes stack; PushScope/PopScope; shadowing/leak tests |

---

## 16. Coverage Summary

| Category | Total Items | Implemented | Partial | Not Implemented |
|---|---|---|---|---|
| Lexer | 22 | 22 | 0 | 0 |
| Parser | 21 | 21 | 0 | 0 |
| AST | 17 | 17 | 0 | 0 |
| Compiler / Opcodes | 37 | 34 | 0 | 3 |
| ZeroVM Execution | 30 | 27 | 0 | 3 |
| Runtime Values | 3 | 3 | 0 | 0 |
| Heap | 15 | 15 | 0 | 0 |
| GC | 6 | 5 | 1 | 0 |
| Modules | 13 | 12 | 1 | 0 |
| Async / Task | 7 | 7 | 0 | 0 |
| Standard Library | 12 | 12 | 0 | 0 |
| Dependencies | 15 | 11 | 1 | 3 |
| Error Handling | 14 | 14 | 0 | 0 |
| Semantics | 36 | 33 | 3 | 0 |
| **TOTAL** | **248** | **233** | **6** | **9** |

*(Job 013: Modules alias/re-export/conflict/live binding rows → Implemented (test_job013_*). Modules now 12 Implemented + 1 Partial (§73 eval failure / try/catch tension — ISSUES.md §62).*
*(Job 014: Async rows updated — async/await Implemented; task states / task.state / scheduler Partially (cancelled, task.cancel, interleaving → Job 015; ISSUES.md §63). Async now 2 Implemented + 3 Partial + 2 Not. Totals: 243 = 216+8+19.)*
*(Job 015: all remaining Async rows → Implemented (task states, task.all/race, task.cancel, task.wait/timeout/sleep, scheduler, concurrent execution). Async now 7 Implemented + 0 Partial + 0 Not. Totals: 243 = 221+5+17. Ambiguities/decisions → ISSUES.md §64.)*
*(Job 016: `http.*` row (§50/§50.1/§59/§60, incl. query encoding + header rules + timeout) → Implemented with mock-server tests. Standard Library now 12 Implemented + 0 Partial + 0 Not. Totals: 243 = 222+5+16. Decisions → ISSUES.md §65.)*
*(Job 017: Dependencies domain 10 Not-Implemented rows → 11 Implemented + 1 Partial + 3 Not (5 rows added: package import, boundary, scripts, dependency tree, scope; registry Partial = local filesystem source only). Totals: 248 = 233+6+9. Decisions → ISSUES.md §66.)*


---

## 17. What Works Well

- All core language syntax (variables, functions, closures, control flow)
- Arithmetic, comparison, boolean logic, ternary
- Arrow functions (single/multi/no param, expression/block body)
- Closures with shared mutable state (HeapRef-based)
- GC with mark-and-sweep and cycle handling
- Heap allocation with generation-based invalidation
- Shared reference semantics for collections (heap-backed List/Object)
- Block-level lexical scope (scope stack per frame)
- Function scope assignment semantics (§4) with register_implicit_local
- Duplicate declaration/key validation (compile + runtime)
- `break`/`continue` outside-loop and function-boundary validation
- `&&`/`||`/`not` parsing (aliases of `and`/`or`/`!`)
- `BinaryOp::RawEqual` naming for `===` (was StrictEqual)
- 489 tests passing with 0 failures (38 in Job 008; 30 in Job 009; 66 in Job 010)
- 531 tests passing with 0 failures (Job 011 added 35 vm tests + 7 json module tests)
- 560 tests passing with 0 failures (Job 012 added 29 module tests)
- 573 tests passing with 0 failures (Job 013 added 13 binding tests: live binding, alias, re-export, export conflict)
- 590 tests passing with 0 failures (Job 014 added 17 tests: 14 vm async/await + 2 parser + 1 lexer)
- 610 tests passing with 0 failures (Job 015 added 20 vm tests: task.all/race/cancel/wait/timeout/sleep + scheduler/concurrency)
- 623 tests passing with 0 failures (Job 016 added 13 tests: 12 required http.* tests + 1 options-type test, against a deterministic local mock server)
- 727 tests passing with 0 failures (Job 017 added 93 unit tests + 11 CLI integration tests: ZSON/manifest/lockfile parsing, version selectors, install flow, nested/shared/circular dependency resolution, package import + boundary + scope, `zpm run`, and end-to-end `zpm install` → `zpm run start` → `zsc` package import)
- 765 tests passing with 0 failures (Job 018 added 25 package integration tests + 8 CLI integration tests + 1 superseded/replacement unit test: §90 conflict resolution, §91 remove, §92 update, §93 git source, §94 publish e2e, §95 registry/index, §96 metadata, §97 integrity, §98 cache, §99 source priority, §100 completion checklist)

---

## 18. Not In Scope (Future Jobs)

- Top-level await as literally written in §49's invocation snippet and §112's example: still a compile error by design (rules normative; examples run via `async function main()` wrapper — ISSUES.md §63a/§64a). Async concurrency itself (§102, §103-§105, §109-§112) implemented in Job 015 — see §10
- http.* (§50/§50.1/§59/§60): implemented in Job 016 — see §11 (nothing of the listed stdlib set remains deferred; only package/zpm completion, bytecode hardening, and the §18 gap list below remain)
- Package management (Jobs 017+018): §81-§100 now Implemented in CHECK.md §12 — manifest/lockfile/`.zsc_m`/selectors/`zpm install`+`run` (Job 017), then §90 multi-version resolution, §91 remove, §92 update, §93 git source, §94 publish, §95 registry+index+additional registries, §96 metadata, §97 integrity, §98 cache, §99 source priority, §100 completion (Job 018). Still open as a transport detail: HTTP/remote registry over the network (§95's registry may be remote; implementation is local-filesystem-only, minimum local implementation recorded in ISSUES.md §67) — see §12
- Default parameters evaluated at call time (Job 008 explicitly out of scope)
- Closure-in-data-structure shared env fix (heap_object_for_element)
- GC root coverage for local vars/params
- Bytecode versioning/validation (Job 019)
- Native call convention (CALL_NATIVE — formalized Job 019; Job 010 uses soft BuiltIn+GetProperty path)
- One-way const marking: `let A=[1]; const C=A` freezes A's object too (documented limitation, ISSUES.md)
- break/continue crossing try-with-finally (documented gap; return-through-finally IS implemented)
- Multi-arg print / full print.* API (still single-arg; Job 010 confirmed types work, arity not expanded)
- CLI runner wiring for system.args(): resolved by Job 017 — `zsc <file.zs> [args...]` (src/main.rs) strips interpreter+script names and calls `ZeroVM::set_program_args` before `run_modules` (§52); verified by test cli_zsc_passes_program_arguments (`zsc index.zs hello world` → 2 / hello / world)

### Candidate Gaps for Future Work (discovered during Job 008/009)

- **Default parameters at call time**: currently compile-time evaluation only; spec §30 implies call-time evaluation for dependent defaults.
- **Closure in data structures**: `heap_object_for_element` creates a new Environment clone, breaking shared closure state when a function/closure is stored inside a list/object.
- **GC roots**: `gc.rs` roots only `live_refs`; frame locals, params, and captured envs are not scanned as roots — only reachable via existing live_refs.
- **One-way const marking**: `mark_const_if_collection` freezes the heap object for any binding that aliases it; `let A=[1]; const C=A` makes both const. Spec silent on aliasing const-flow; see ISSUES.md §58b.
- **Const aliasing through function returns**: returning a const collection and storing in `let` keeps the object marked const (same one-way marking path).
- **break/continue crossing try-with-finally**: return-through-finally is implemented; break/continue that leave a try region do not currently run finally (spec says finally always runs on early exit — treat as defect to fix, possibly Job 010+).
- **GC roots for scope stack**: even with scope stack, individual Binding HeapRefs are not enumerated as GC roots during mark phase.
- **Error structure fields (type/source location/call stack)**: Runtime.md §154 allows optional type/location/callstack info; Job 009 binds `e` as message string only — richer Error object may be needed later without breaking catch.
- **Source location on runtime errors**: not yet attached to caught errors (optional per §154).

### Candidate Gaps for Future Work (discovered during Job 012/013)

- **§73 vs §78 try/catch on module eval failure**: §73 says eval errors can be caught if import is a runtime operation; §78 says import is static declaration processed before module code. Under static analysis (Job 012), dependency failure aborts `run_modules` before importer body runs — not in-language catchable. See ISSUES.md §62. (Only remaining Modules.md Partial; Jobs 012+013 binding features are Implemented.)
- **Export alias (`export { x as y }`)**: §80 lists "export alias / import alias" among locked rules, but §47/§67 only define import `as` and re-export `from`; no section specifies export-list alias syntax. Not in Job 013 Scope C. Recorded as ambiguity — no export-alias syntax implemented.

### Candidate Gaps for Future Work (discovered during Job 014)

- **Top-level await / §112 example** *(resolved by Job 015 — ISSUES.md §64a)*: compile error kept (rules normative); §112's example logic runs wrapped in `async function main()` (test_job015_sleep_ordering_not_creation_order). Literal top-level-await text remains non-runnable — spec-level documentation ambiguity left in place.
- **`await` on a running task under a real scheduler** *(resolved by Job 015 — ISSUES.md §63d/§64c)*: awaits now wait for the target's terminal state; only genuine cycles (no runnable task, no timer) raise the deadlock ERROR (test_job015_await_cycle_deadlock_error).
- **Task memory reclamation**: `ZeroVM.tasks` grows monotonically; task args/callee/env are rooted at creation and results stored at completion, but tasks are never freed. Long-running programs creating many tasks will retain all of them (no `task.dispose` in spec — investigate in Job 015/GC work).

### Candidate Gaps for Future Work (discovered during Job 015)

- **Un-awaited `task.sleep`/`task.timeout` not rejected**: §111 says `task.sleep()` "harus digunakan dengan await" but the spec defines no detection mechanism; an un-awaited sleep becomes a silent background timer task (ISSUES.md §64d). Spec-level rule without an enforceable consequence — needs spec clarification before a language rule can be invented.
- **Virtual clock vs wall-clock pacing**: scheduler uses a virtual monotonic clock (deterministic, instant tests); sleep/timeout do not pace against real time (ISSUES.md §64d). No spec section exposes wall-clock time, so nothing observable is missing — flag if a future stdlib exposes real time.
- **Task memory reclamation (carried from Job 014)**: `ZeroVM.tasks` grows monotonically; results/errors/contexts of terminal tasks are retained forever (no `task.dispose` in spec — GC work).
