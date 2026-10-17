# Job 033 — Flag execution limits di `zsc` (spec)

## Objective
Menulis aturan CLI flag execution limits (gap CHECK.md:484 / README
"Disclosed gaps"): §150 (Runtime.md) hanya mengatur limits di level runtime
environment; Dependencies.md §100 tidak mendefinisikan flag zsc untuk limits.

## Keputusan (user: "Approve apa adanya", 2026-10-12)
- Lima flag, satu per field `Limits`: `--max-instructions=N`,
  `--time-limit=MS`, `--max-memory-slots=N`, `--max-call-depth=N`,
  `--max-tasks=N`.
- Hanya dikenali **sebelum** kode/mode (argumen pertama-non-flag); argumen
  setelah kode tetap argumen program (§52/§100 tanpa perubahan).
- Berlaku utk mode file, eval, dan run bytecode.
- Nilai bukan angka atau 0 → penggunaan perintah yang salah (kode keluar 1).

## Revisi
- `Dependencies.md` §100: blok baru "Flag Execution Limits" setelah
  "Mode Evaluasi". §150 tetap sumber aturan limits; blok ini hanya surface CLI.
- ISSUES.md §71g: catat Decision + spec reference.
- CHECK.md:484 row → Resolved menyusul Job 034 (evidence test).
- README.md baris "Disclosed gaps" CLI flags → diupdate di Job 034.

## Required Final Report
Kutipan blok spec + file berubah.
