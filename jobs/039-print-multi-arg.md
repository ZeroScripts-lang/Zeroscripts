# Job 039 — Multi-argumen `print()` / `print.err()` (§44)

## Keputusan (user, 2026-10-14)
- `print(a, b, c)` dan `print.err(a, b, c)`: 0+ argumen, diformat lalu
  digabung dengan **satu spasi**, satu baris + newline. Tanpa argumen =
  baris kosong. Keduanya aturan sama.
- Sebelumnya argumen ekstra di-drop diam-diam (args[0] saja).

## Spec
`StandardLibrary.md` §44: paragraf additive "Argumen" + dua contoh.

## Impl
`src/vm/mod.rs` call_native: arm `print` = map format_value + join(" ");
arm `print.err` = error_message (§59a) + join(" ").

## Tests
`tests/zero039_print_multi_arg.rs` (8). Suite → **1016 passed, 0 failed**;
warning baseline 2.
