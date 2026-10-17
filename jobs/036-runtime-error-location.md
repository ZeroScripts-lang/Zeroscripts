# Job 036 — `e.location` runtime error + bytecode version bump

## Keputusan (user, 2026-10-13, dikunci)
1. `location.file` = path input apa adanya untuk entry (tanpa kanonisasi);
   kanonik hanya untuk dependency resolution (ModuleMeta.display_path).
2. Posisi = token pemicu operasi (span parser `[`, `.`, `(`, operator biner,
   `=`, `+=`, `**`); span lama Binary/Index/Member/Call = left-expr (di-patch).
3. Lookup memakai pc instruksi yang **gagal** (`instr_pc`), bukan pc+1.
4. Tanpa metadata (pecahan bytecode lama, task fail path) → `location` = `null`.
5. Side table di-serialize ke `.zsbc`; version 1 → **2**. File v1 ditolak
   dengan pesan jelas, tanpa panic. Spec §154 "jika informasi tersebut
   tersedia" — keputusan mekanis, dicatat di ISSUES.md §75f tanpa mengubah
   spec.

## Scope
- compiler.rs: DebugEntry, emit_located, BYTECODE_VERSION=2, debug side
  tables (top-level + per-function body + param-default bodies), swap saat
  body compile, merge_modules offset + stamp source_file.
- parser.rs: span token pemicu.
- module.rs: ModuleMeta.display_path, finish(entry_display, entry_canon).
- bytecode_io.rs: serialize/deserialize debug sections (v2 layout).
- vm/{mod,types,task}.rs: active_debug/active_source, lookup_debug_loc,
  note_error_pc di 3 run-loop + task step, make_error_object isi
  {file,line,column} atau Null, Frame.return_debug/return_source,
  TaskContext.debug/source_file.

## Out of Scope
- Spec rewrite (§154/§1020 tidak diubah — location sudah opsional per spec).
- Call stack exposure (§154 opsi kedua, tidak diminta).
- Perubahan output unhandled RuntimeError (tetap sama).

## Tests
- tests/zero036_error_location.rs (9 tests): 5 kategori
  (IndexError/KeyError/DivisionError/TypeError/ReferenceError) via -e,
  location.file = path verbatim (termasuk `./`), roundtrip `.zsbc`
  (location tetap, file = sumber compile), file v1 rejected
  (Unsupported bytecode version: 1, tanpa panic), unit debug-table
  sparse+sorted.
- Test lama di-update: test_job059a_e_fields_type_message_location (null →
  object), test_job059a_print_e_shows_object (location: null →
  location: {file:), test_job019_unsupported_version_constant (contains 2),
  test_job024_foreign_opcode payload v1 → v2.
- Suite → **987 passed, 0 failed**; warnings baseline 2.
