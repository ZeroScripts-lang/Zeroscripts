# Job 028 — Optimasi Dispatch Interpreter (Build Profile + Opcode `Rc<str>`)

## Objective

Turunkan waktu eksekusi loop interpreter (baseline `bench_loop.zs` 1 juta
iterasi = 1,02s; `bench.zs` = 1,42s; Python 3.13 = ~0,3s) lewat dua langkah
kecil berdampak besar, **tanpa mengubah perilaku observable maupun format
bytecode**:

- **Langkah 0**: tambah `[profile.release]` (`opt-level = 3`, `lto = "fat"`,
  `codegen-units = 1`) — Cargo.toml saat ini tidak punya profil sama sekali.
- **Langkah 1**: operand bertipe `String` pada `Opcode` → `Rc<str>` — saat ini
  run loop melakukan `.clone()` instruksi per langkah (3 titik:
  `src/vm/mod.rs:388`, `:580`, `:1441`), sehingga `LoadLocal(String)` dll.
  menghasilkan **alloc+free String ±13 juta kali** per run benchmark.

## Required Reading

- Bytecode.md §158 (format serialisasi — wire format wajib identik), §124-§125
  (validasi).
- ISSUES §71b/§71c (limits — `check_execution_limits` harus tetap per-
  instruksi ketat, tidak boleh di-batch).
- src/compiler.rs (`enum Opcode` + semua situs konstruksi), src/bytecode_io.rs
  (encode/decode operand String), src/vm/{mod,tests,validate}.rs (match arm +
  pemakaian operand).

## Scope

A. Cargo.toml: `[profile.release]` opt-level 3, lto fat, codegen-units 1.
   Tidak menyentuh panic strategy (perlu audit `catch_unwind` dulu).
B. Semua varian `Opcode` pembawa string → `Rc<str>`:
   `DeclareLocal`, `LoadLocal`, `StoreLocal`, `DeclareGlobal`, `LoadGlobal`,
   `StoreGlobal`, `CallNative(name, _)`, `MakeClosure(_, Vec<(Rc<str>, bool)>)`,
   `LoadCaptured`, `StoreCaptured`, `MakeObject(Vec<Rc<str>>)`,
   `GetProperty`, `SetProperty`.
C. Penyesuaian: konstruksi compiler (`… .clone()` → `.into()` / `Rc::from`),
   encode/decode bytecode_io (tulis `as_ref()`, baca `Rc::from`), match arm VM
   (`&**name` saat dipakai sebagai `&str`), test konstruksi.
D. Pengukuran: bench loop+sum diukur sebelum/langkah0/langkah1 (release).

## Out of Scope

- Perubahan **apapun** pada wire format §158 (bytecode yang tertulis harus
  byte-identik; tidak ada versi bump).
- Slot-index lokal / perubahan model scope (langkah 2 — terpisah, menunggu
  profiling ulang).
- Superinstruction/peephole yang butuh opcode baru (keputusan format → ISSUES).
- Optimasi `check_execution_limits`/`maybe_gc` (profil dulu).
- Autobenchmark, dependency baru, perubahan limits §150/§153.

## Testing Requirements

1. Full suite 951 test tetap hijau (0 failed) — terutama 15 test roundtrip
   bytecode_io dan validator §124.
2. `cargo build` warning tetap persis 2 (baseline).
3. Bytecode hasil `zsc compile` sebelum vs sesudah perubahan **identik
   byte-per-byte** (dibuktikan test roundtrip + `cmp` bila memungkinkan).
4. Bench: `bench_loop.zs` + `bench.zs` release diukur; laporan angka
   sebelum/sesudah.

## Acceptance Criteria

- Bench loop menurun (target kasar ≥20% dari baseline 1,02s setelah langkah 0,
  kumulatif lebih setelah langkah 1; angka aktual dilaporkan apa adanya).
- Nol perubahan perilaku observable; wire format §158 identik; suite 951
  hijau; warning 2.
- Job terdokumentasi + laporan final sesuai struktur.

## Required Final Report

1. Files changed + keputusan mekanis (wire format invariance, varian yang
   ikut diubah, panic strategy tidak disentuh).
2. Tabel bench sebelum → sesudah langkah 0 → sesudah langkah 1.
3. Full suite (angka) + warning count.
4. Konfirmasi: §158 tak berubah, limits per-instruksi tak berubah, sisa
   kandidat optimasi (slot index lokal, superinstruction) tercatat di laporan.

## Result (2026-10-12)

- **Suite: 951 passed / 0 failed; warning persis 2 (baseline).**
- **Wire format identik**: `zsc compile` 7 contoh `examples/*.zs` byte-identik
  pre-Rc vs post-Rc (`cmp`). Selisih vs binary `zsc-baseline` 11 Okt hanya
  pada `demo.zs` = efek switch-fix Job 027, bukan regresi format.
- **Bench** (Termux aarch64, 3 round, ref Python ~0,36s stabil, median):

  | Binary | loop 1M | sum 1M |
  |---|---|---|
  | kode lama + default | 0,948s | 1,495s |
  | **kode baru + default** | **0,665s (−30%)** | **1,000s (−33%)** |
  | kode lama + LTO | 0,929s | 1,539s |
  | **kode baru + LTO** | **0,686s (−26%)** | **0,990s (−36%)** |

- **Langkah 0 (profil release)**: kontribusi ~±3% (noise) pada workload ini —
  tetap dipertahankan sebagai baseline build standar, tapi BUKAN sumber
  kemenangan. Kemenangan datang dari **langkah 1 (`Rc<str>`)**: −26…−36%.
- Status sebelum/sesudah vs Python: loop 2,6× → **1,8×**; sum 4× → **~2,2×**.
- Catatan thermal: dua kali build 5 menit menaikkan suhu device ±40% semua
  benchmark (termasuk Python); semua angka akhir diambil setelah cooldown
  dengan ref Python per round.
- Sisa kandidat (TIDAK dikerjakan, sesuai Out of Scope): slot-index lokal
  (langkah 2, target masuk rentang Python), superinstruction (butuh keputusan
  format §158), batching limit-check (profil dulu).
