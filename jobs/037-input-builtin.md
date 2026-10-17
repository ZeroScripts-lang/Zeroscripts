# Job 037 — `input()` builtin (StandardLibrary §157, Runtime §148)

## Objective
Implementasikan `input()` sesuai StandardLibrary.md §157 (Console Input):
membaca satu baris dari stdin; 0 atau 1 argumen (prompt string dicetak
persis ke stdout sebelum menunggu, diflush); CRLF dinormalisasi; baris
kosong → ""; EOF → null; blocking (eksekusi runtime berhenti, task tidak
jalan); kegagalan I/O → runtime error catchable (§148); >1 argumen atau
argumen non-string → ERROR.

## Required Reading
- StandardLibrary.md §157 (baris 348-389)
- Runtime.md §148 (ZeroVM I/O Boundary)
- INDEX.md baris 157, 148
- src/native.rs (BUILTIN_ROOTS/ROOT_FUNCTIONS), src/vm/mod.rs call_native

## Scope
- `src/native.rs`: "input" ke BUILTIN_ROOTS + ROOT_FUNCTIONS (+ test list
  di native.rs tests).
- `src/vm/mod.rs`: arm `"input"` di `call_native` (satu-satunya boundary,
  §145-§146; static CallNative dan dynamic BuiltIn sama-sama lewat sini).
- Prompt ditulis langsung ke stdout + flush (harus segera terlihat sebelum
  blocking read; `self.output` hanya dicetak setelah run selesai — tidak
  dipakai untuk prompt).
- `tests/zero037_input.rs`.

## Decisions (mechanical — spec silent on exact wording)
- Pesan error mengikuti gaya builtin sedia:
  - arity: `input() expects exactly 0 or 1 argument` (mengandung
    "expects exactly" → kategori ArgumentError §59a).
  - non-string: `Expected a string argument for input()` (starts_with
    "Expected " → kategori TypeError).
  - I/O failure: `input() failed: <io error>` (RuntimeError, catchable
    §148). Dicatat di Required Final Report, bukan spec-derived.

## Out of Scope
- `input()` async/non-blocking (§157 menyatakan blocking; §148 menyebut
  async I/O "dapat" — tidak diminta).
- Runtime I/O permission config (§149) — tidak ada API host yang diminta.
- Spec rewrite.

## Testing Requirements
- Subprocess CLI dengan stdin piped: baris biasa, prompt persis, EOF →
  null, baris kosong → "", CRLF, >1 arg ERROR catchable (e.type ArgumentError),
  non-string ERROR catchable (e.type TypeError), blocking behavior asersi
  implisit (read sinkron).
- Suite penuh tidak boleh regresi.

## Acceptance Criteria
- §157 seluruh behavior tercakup test; suite hijau; warning tidak naik.

## Required Final Report
Apa yang diimplementasi, files changed, tests, hasil suite, keputusan
mekanis (wording error), issue spec apa pun.
