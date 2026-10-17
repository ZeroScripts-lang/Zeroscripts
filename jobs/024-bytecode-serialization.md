# Job 024 — Bytecode Serialization (§158, §124, §125, §155)

## Objective

Implementasikan serialisasi bytecode ke file dan muat-ulangnya tanpa source code,
sesuai section **§158 "ZeroVM Bytecode Serialization"** (Bytecode.md, ditulis pada
sesi spec 2026-10-08) — menutup gap lama CHECK.md "No bytecode file format"
(ISSUES §68d: "the spec defines no bytecode file format, so nothing serializes it").

Hasil yang diminta:

1. Codec `save`/`load` untuk `ModuleBundle` (program gabungan + metadata module)
   dengan checksum SHA-256 diakhiri payload (§97 hash).
2. Subcommand CLI `zsc compile <file.zs> [-o <out>]` dan
   `zsc run <file.zsbc> [args...]`.
3. `zsc <file.zs>` dan seluruh perilaku CLI lama **tidak berubah**.

## Required Reading

- Bytecode.md **§158** (spesisi baru — aturan wajib), §124 (validation),
  §125 (versioning), §143 (function metadata pairing).
- Runtime.md §155 (debug info opsional; VM tidak boleh bergantung pada source
  asli untuk menjalankan bytecode).
- StandardLibrary.md §52 (`system.args()` — argumen setelah nama file).
- Dependencies.md §100 ("Nama Binary dan Kode Keluar" — exit code 0/1).
- ISSUES.md **§73a** (empat keputusan user + seluruh detail mekanis yang
  diadopsi — keputusan di sini otoritatif, jangan diubah diam-diam).
- CHECK.md baris "No bytecode file format" (candidate gap yang ditutup job ini).
- src/compiler.rs (BytecodeProgram, Opcode 57 varian, Constant, FunctionInfo,
  BytecodeHeader, BYTECODE_MAGIC/VERSION), src/module.rs (ModuleBundle,
  ModuleMeta, load), src/hash.rs (Sha256 §97), src/main.rs (CLI), src/vm/mod.rs
  (validate, run_modules).

## Scope

A. **Codec** — modul baru (mis. `src/bytecode_io.rs`, terdaftar di lib.rs):

   - `save(bundle: &ModuleBundle) -> Vec<u8>` = payload || sha256(payload).
   - `load(bytes: &[u8]) -> Result<ModuleBundle, BytecodeFileError>` —
     verifikasi checksum DULU, lalu parse payload (little-endian, layout per
     ISSUES §73a; gagal parse/tidak muat → error, tanpa panic).
   - Decode opcode tak dikenal → `Opcode::Unknown(tag)` (§124 + ISSUES §68e:
     ditolak sebagai VM Error saat validasi, bukan format error).
   - Match encode **eksaustif** atas 57 varian Opcode (varian baru = compile
     error sampai codec diupdate).

B. **CLI** — src/main.rs:

   - `zsc compile <file.zs> [-o <path>]`: `module::load` → `save` → tulis.
     Default output: direktori yang sama dengan source, ekstensi `.zsbc`.
     Tidak mengeksekusi program.
   - `zsc run <file.zsbc> [args...]`: baca → `load` → `ZeroVM::new` →
     `set_program_args` → `run_modules`. Error load → stderr, exit 1.
   - `zsc <file.zs>` / `--help` / `--version` tidak berubah kecuali teks
     usage yang menambahkan dua baris subcommand (JANGAN menambah `-e` —
     eval belum diimplementasi, job terpisah).
   - Disambiguasi: argumen pertama persis `compile`/`run` = mode; file dengan
     nama itu dijalankan `./compile` / `./run`.

C. **Tests** (prefix `test_job024_*` di src/bytecode_io.rs dan/atau
   tests/zero024_*.rs; CLI e2e mengikuti pola `env!("CARGO_BIN_EXE_zsc")`).

## Out of Scope

- `zsc -e` / `--eval` (spec §100 sudah ada, implementasi = job terpisah —
  sudah tercatat sebagai candidate gap).
- Cache otomatis / staleness / per-module artifacts (keputusan user §73a:
  satu file = seluruh program, compile eksplisit saja).
- Debug information / penyertakan source di artefak (§155: opsional; compiler
  belum meng-emit line info — menambahnya = versi bytecode baru, §125).
- Perubahan bytecode grammar, opcode, compiler, validator, atau VM execution.
- Perubahan zpm / package system / registry.
- Signing, enkripsi, kompresi artefak (tidak ada di spec).

## Testing Requirements

1. **Roundtrip identik**: compile program multi-module → save → load →
   bundle ekuivalen (header, entry, constants, instructions, functions,
   function_bodies, default_bodies, modules) DAN hasil eksekusi identik
   dengan eksekusi langsung.
2. **Roundtrip seluruh varian Opcode**: satu test membangun stream
   `Opcode` yang memuat ke-57 varian (termasuk `TryPush` dengan
   catch/finally Some dan None, `MakeClosure` dengan captured, payload
   String/usize/Option/Vec) → save → load → ekuivalen.
3. **Distribusi tanpa source**: `zsc compile` di direktori berisi `.zs`,
   salin artefak ke direktori lain TANPA file `.zs` → `zsc run` menghasilkan
   output yang sama (§155).
4. **Checksum**: byte di tengah payload diubah → `load` menolak (dan CLI
   exit 1, tanpa eksekusi); file terpotong (< 32 byte hash / potongan
   payload) → ditolak.
5. **Header/versi (§124/§125)**: payload dengan magic salah atau versi tak
   didukung namun hash konsisten → lolos `load`, ditolak saat run sebagai
   VM Error (jalur `validate_header` yang sudah ada).
6. **Opcode tak dikenal (§124)**: tag opcode asing di payload dengan hash
   konsisten → `load` menghasilkan `Opcode::Unknown` → run → VM Error
   InvalidOpcode, tidak dieksekusi.
7. **`zsc compile` tidak mengeksekusi**: source dengan side-effect (print)
   → compile menghasilkan artefak dengan output kosong.
8. **`zsc app.zs` tidak berubah**: perilaku, exit code, dan output program
   lama identik (test yang sudah ada tetap hijau + test eksplisit compile+run
   satu langkah vs `compile`+`run` dua langkah menghasilkan output sama).
9. **`system.args()` via `zsc run`**: `zsc run app.zsbc a b` → `["a","b"]`.
10. **CLI**: `zsc compile` tanpa file → exit 1; `zsc compile` file hilang →
    exit 1; `zsc run` file hilang → exit 1; `zsc run` file bukan bytecode →
    exit 1; sukses → exit 0; default output `.zsbc` dibuat di direktori
    source; `-o` dihormati.
11. **Regresi**: full suite baseline (912 passed saat Job 023 selesai) tetap
    0 failed; `cargo build` warning tetap persis 2 (function_index,
    execute_load_variable); tanpa warning baru di `cargo test`.

## Acceptance Criteria

- §158 seluruh Aturan terpenuhi dan diuji (terutama: checksum sebelum
  eksekusi; tanpa source saat `run`; compile tanpa eksekusi; ekstensi
  `.zsbc`; disambiguasi subcommand; exit code §100).
- §124/§125 tidak diubah: validasi tetap berjalan di `validate()` sebelum
  instruksi pertama, baik untuk jalur file maupun jalur bytecode.
- Nol perubahan grammar/compiler/validator/VM; nol permukaan in-language;
  `-e` tidak ikut terimplementasi.
- Keputusan §73a dipatuhi apa adanya; tidak ada keputusan mekanis baru yang
  diam-diam — yang muncul dicatat di ISSUES.md §73 (sub-butir baru) dan
  disebut di laporan.

## Required Final Report

1. Files changed.
2. Bentuk file (layout payload + checksum) + mengapa cukup untuk §158.
3. Tabel opcode tag / strategi encode (dan bagaimana kegagalan eksaustif
   terjaga).
4. Peta spec → test (§158 aturan → test mana; §124/§125 → test mana;
   §155 → test mana; §52 → test mana; §100 → test mana).
5. Hasil full suite (angka passed/failed) + warning count.
6. Konfirmasi: `zsc <file.zs>` tak berubah, `-e` tak diimplementasi, nol
   perubahan bytecode grammar/compiler/validator/VM execution.
7. Keputusan mekanis baru (jika ada) + lokasi pencatatan di ISSUES §73.
8. CHECK.md: baris/entry yang diupdate ("No bytecode file format" →
   closed/diubah sesuai bukti test; §16 jika relevan; candidate gap `-e`
   tetap terbuka).
