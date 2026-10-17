# Job 038 — Bare return boundary (§26 "Batas statement untuk return")

## Keputusan (user, 2026-10-14, opsi A)
Nilai `return` hanya diparsing jika ekspresinya dimulai pada **baris yang
sama** dengan kata kunci `return` (restricted production, gaya Rust/Swift).
Token di baris berikutnya = bare return; statement berikutnya tetap
statement terpisah dan TIDAK menjadi nilai return — menutup pelanggaran
§26 "Code setelah return valid tetapi tidak pernah dieksekusi" yang membuat
`return\nprint("x")` mengeksekusi print sebagai nilai return.

Opsi B (newline = batas semua statement) dan C (ubah §26) ditolak.

## Spec
`Language.md` §26: satu paragraf additive "Batas statement untuk return"
setelah paragraf "Code setelah return...". Tidak ada paragraf lama diubah.

## Impl
`src/parser.rs` `parse_return`: tambah cek `self.current().line >
return_line` → value = None. Ekspresi multi-line tetap jalan selama
dimulai di baris return: `return 1 +\n 2`, `return [\n1,\n2\n]`,
`return (\n…\n)` — expression parser tidak peduli baris, hanya token
pertama yang menentukan.

## Tests
- `src/parser.rs`: `test_job038_return_newline_is_bare_return`,
  `_same_line_expression_kept`, `_multiline_list_literal_kept` (3 unit).
- `tests/zero038_bare_return.rs` (8 CLI): bare return tidak menelan
  statement berikutnya (function + top-level), nilai null, same-line
  tetap, trailing-operator lintas baris, list literal lintas baris, paren
  lintas baris, contoh recursion §27 tidak berubah.
- Suite → **1008 passed, 0 failed**; warning baseline 2.

## Catatan
- `;` tetap ditolak di seluruh bahasa (`let x = 1;` → parser error) —
  konsisten; `return;` bukan bug terpisah, termasuk dalam ketiadaan
  dukungan semicolon global.
