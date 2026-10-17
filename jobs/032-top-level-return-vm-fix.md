# Job 032 — Top-level `return`: VM implementation (keputusan B)

## Objective

Mengimplementasikan aturan top-level `return` (ISSUES §75e, keputusan user
opsi B): program berhenti normal pada titik `return`, exit 0, output sebelum
`return` tampil, code setelahnya tidak dieksekusi — pada jalur `run()` dan
`run_modules`.

## Changes

- `src/vm/mod.rs`:
  - `execute_return` + `complete_pending_return`: saat frame **non-task
    terakhir** ter-pop (`call_stack` kosong) → `state = Completed`; nilai
    return jatuh ke orphan slot (§75a, tak teramati program sesuai §26).
  - `run_module_range`: break saat `state == Completed` (menggantikan
    re-execute dari pc 0).
  - `run_modules`: break dari loop module + skip export capture (root frame
    sudah tidak ada); scheduler tetap jalan seperti `run()`.
- `run()` tidak diubah — loop-nya sudah `while state == Running`.

## Tests (7 baru)

- Unit `test_job032_*` (5): module path, run path, bare return (EOF),
  function return tak berubah, return top-level melewati `finally`.
- CLI `tests/zero032_top_level_return.rs` (2): exit 0 + stdout tepat
  `a\n` untuk file mode dan `-e` mode.

## Out of Scope / ditemukan & dicatat

- Parser: `return\n<stmt>` menelan statement berikutnya sbg expression
  (pre-existing; `Language.md` §26 dilanggar utk kasus ini) → **kandidat gap
  baru di CHECK.md "Candidate Gaps (Job 032)"** — butuh aturan grammar
  statement-boundary dari spec; tidak ditebak di job ini.
- Residual `;` setelah `return` gagal parse (bahasa newline-based, pre-existing).

## Result (2026-10-12)

- Suite: **968 passed / 0 failed**, 2 warning baseline.
- `zsc file.zs` top-level return → stdout `a`, exit 0; `-e` mode identik.
- ISSUES §75e → Resolved (dengan evidence test); CHECK §18 candidate-gap
  row → RESOLVED (Jobs 031+032); spec 3 tempat (Job 031) selaras.
