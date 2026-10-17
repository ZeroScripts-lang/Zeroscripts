# Job 025 — zsc Eval Mode (`zsc -e` / `--eval`, §100 "Mode Evaluasi")

## Objective

Implementasikan mode evaluasi `zsc` sesuai section **§100 "Mode Evaluasi"**
(Dependencies.md, ditulis pada sesi spec 2026-10-08) — menutup candidate gap
"spec exists, implementation pending" (ISSUES §72a, CHECK.md).

Hasil yang diminta:

1. `zsc -e <code> [args...]` (alias `--eval`) menjalankan kode dari string
   tanpa file script.
2. Import relatif resolve terhadap current working directory (program eval =
   file virtual di cwd — §72a butir 4).
3. Seluruh perilaku CLI lain (`<file.zs>`, `compile`, `run`, `--version`,
   `--help`) tidak berubah (help hanya menambah baris `-e`).

## Required Reading

- Dependencies.md **§100 "Mode Evaluasi"** (aturan wajib — delapan bullet +
  contoh `system.args()`).
- ISSUES.md **§72a** (keputusan: first-arg flag, kode = argumen berikutnya,
  args sesudah kode tak pernah di-parse ulang, kode kosong valid, pipeline
  identik, cwd sebagai file virtual, §52, §100 exit codes; daftar "not
  specified": stdin, konkatensi beberapa `-e`).
- Modules.md §46, §64 (import relatif ke lokasi file yang melakukan import;
  eval = virtual file di cwd).
- StandardLibrary.md §52 (`system.args()`).
- src/module.rs (`Loader::visit`, `parse_and_extract_imports`, `load`,
  `Project::discover`), src/main.rs (dispatch argumen pertama).

## Scope

A. **`module::load_source(virtual_path, source)`** — refactor minimal:
   `Loader::visit` dipecah menjadi baca-disk + badan yang menerima source
   string (entry eval tidak pernah dibaca/ditulis di disk); ekstrak tail
   `load()` (merge + `ModuleMeta`) menjadi helper bersama. `virtual_path`
   = `<cwd>/__zsc_eval__.zs` (tidak dibuat di disk); `Project::discover`
   dan resolusi import berjalan seperti mode file, dengan induk entry =
   cwd.

B. **CLI** — `cmd_eval` di src/main.rs:
   - dispatch: argumen pertama `-e` atau `--eval`; tanpa kode → usage error
     exit 1; kode = argumen kedua; sisanya program args (`set_program_args`).
   - pipeline identik: `load_source` → `ZeroVM::new` → `run_modules` →
     print output; error → stderr, exit 1 (§100).
   - teks usage ditambah baris `zsc -e <code> [args...]` (JANGAN menghapus
     baris compile/run).

C. **Tests** `tests/zero025_zsc_eval.rs` (pola `env!("CARGO_BIN_EXE_zsc")`).

## Out of Scope

- Mode stdin (`zsc -e` membaca pipe) dan konkatensi beberapa `-e` (§72a:
  tidak dispesifikasi).
- Flag CLI untuk execution limits (§71g — job terpisah).
- Perubahan bahasa, compiler, VM, module pipeline untuk file biasa (§64
  program berbasis file tak boleh berubah perilaku).
- Entri eval khusus untuk package import (§87) — mekanismenya sama dengan
  mode file (`Project::discover` + Loader); tidak diuji terpisah di job ini
  (disclosed).

## Testing Requirements

1. `zsc -e "print(1 + 2)"` → output `3`, exit 0; alias `--eval` sama.
2. `zsc -e` tanpa kode → exit 1 (usage); `zsc --eval` tanpa kode → exit 1.
3. `zsc -e ""` → exit 0, output kosong (kode kosong = program valid).
4. Args sesudah kode → `system.args()` (§52): `zsc -e "..." x y` → x, y.
5. `-e` kedua TIDAK diparse ulang: `zsc -e "print(a[0])" -e x` → `-e`
   (program arg, bukan flag).
6. Import relatif dari cwd: `lib.zs` berisi export, cwd = direktori itu,
   `zsc -e "import { f } from \"./lib\"\nprint(f())"` → output export tsb;
   lib.zs TIDAK perlu berada di cwd proses saat test lain berjalan (pakai
   `current_dir`).
7. Syntax error di kode eval → exit 1, stderr non-empty.
8. Runtime error tak ter-catch → exit 1.
9. File bernama `-e` dijalankan via `./-e` (disambiguasi sama dengan
   compile/run — mekanis, dicatat di ISSUES §72a saat job ini).
10. `zsc app.zs`, `zsc --version`, `zsc --help` tidak berubah; help
    memuat baris `-e`.
11. Regresi: full suite baseline (927 passed saat Job 024) → 0 failed;
    `cargo build` warning tetap persis 2; tanpa warning baru di
    `cargo test`.

## Acceptance Criteria

- Seluruh aturan §100 "Mode Evaluasi" terpenuhi dan diuji (termasuk contoh
  `system.args()` pada blok tersebut).
- §72a butir 1-6 dipatuhi; butir 9 (file bernama `-e` → `./-e`) dicatat di
  §72a sebagai keputusan mekanis saat job ini.
- Entry eval tidak menyentuh disk (tidak menulis/membaca file sumber eval);
  import file tetap dibaca normal (§64/§87).
- Nol perubahan perilaku mode file/compile/run; nol perubahan bahasa/VM.

## Required Final Report

1. Files changed + refactor apa saja di module.rs (dan mengapa aman).
2. Peta aturan §100/§72a → test.
3. Full suite (angka) + warning count.
4. Konfirmasi: entry eval tanpa disk I/O, `-e` kedua = program arg, help
   updated, mode lain tak berubah.
5. Keputusan mekanis baru (jika ada) + lokasi pencatatan (ISSUES §72a
   follow-up, CHECK.md gap closure dengan bukti test).
