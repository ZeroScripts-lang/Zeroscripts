# Job 034 — Flag execution limits di `zsc` (implementasi)

## Objective
Implementasi blok spec Job 033: lima flag execution limits di `zsc`.

## Implemented
- `src/main.rs`: `split_limits_flags(argv)` — scan leading `--name=<value>`
  (5 nama) sampai token non-flag pertama; sisanya = mode/kode + argumen
  program (§52/§100). Nilai bukan-angka → `ERROR: invalid value for ...`;
  nilai 0 → `ERROR: ... must be greater than 0` (keduanya exit 1).
  `set_limits` dipanggil di `run_file`, `cmd_eval`, `cmd_run`.
  `usage()` diperbarui.
- `tests/zero034_zsc_limits_flags.rs` (8 tests).

## Tests
`cargo test` → **976 passed / 0 failed**; `cargo build` warning = 2 (baseline).

## Required Final Report
Spec §100 block + ISSUES §71h + implementation + 8 tests; CHECK.md:484
row Resolved; README disclosed-gaps baris CLI flags dihapus.
