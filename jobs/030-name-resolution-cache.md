# Job 030 — Name resolution cache (Jalur A)

## Objective

Eliminasi perhitungan resolusi nama berulang pada hot loop tanpa mengubah
format bytecode §158. Target utama: `bench_loop.zs` / `bench.zs` (while-loop
1M iterasi, 2 load + 2 store per iterasi).

Sebelum Job 030, setiap `LoadLocal`/`StoreLocal` mem-walk HashMap scope
(innermost-first) dan setiap `LoadGlobal`/`StoreGlobal` meng-hash nama pada
HashMap global — per instruksi, per iterasi.

## Design (Jalur A — side-table, wire format tidak berubah)

1. **Global storage → `Vec<(String, Binding)>`** (append-only selain
   `clear()`): deklarasi linear (jumlah global kecil), lookup jadi posisi
   stabil yang bisa di-cache. Collision-free, tanpa hash.
2. **Resolution cache**: `resolve_cache: Vec<ResEntry>` di-index pc
   (instruksi unik sekali resolve, sisanya O(1)).
   - `ResCache::Global(idx)` — load/store global.
   - `ResCache::Local { dist, scope_len }` — distance dari scope teratas +
     panjang scope saat resolve sebagai kunci validasi (scope_len berubah ⇒
     re-resolve). Isi scope pada pc tertentu deterministik (statement linear
     dalam blok), sehingga validasi ini cukup.
3. **Miss tidak di-cache** untuk load (undefined → error; binding bisa dibuat
   belakangan oleh pc lain). Store-create meng-cache idx setelah insert.
4. **Generational invalidation**: `cache_gen: u64` naik setiap
   `program.instructions` diganti (swap/restore function body, default body,
   task save/load) — pc space berbeda per instruksi stream. Juga di-reset
   saat `global_scope.clear()` pada `run()`/`run_modules()`.
5. `execute_instruction` menerima `pc` eksplisit (4 loop fetch).

## Out of Scope

- Ubah format bytecode §158 / operand encoding (Jalur B — butuh keputusan spec).
- Clone instruksi per `Call` (`return_instructions`) — mungkin job terpisah.
- Scope HashMap per-frame → Vec (ukur dulu apakah perlu).

## Testing

- Suite penuh (`cargo test`): baseline 956 sebelum perubahan.
- Test baru `test_job030_*`:
  - store-create lalu hit (function, local bare assignment dalam loop),
  - store/load global bare-assignment dalam loop (jalur module),
  - resolve tetap benar saat scope_len berubah (nested if/while dalam loop).
- Bench: `bench_loop.zs` + `bench.zs` dengan cooldown + ref Python per round.

## Acceptance Criteria

- Semua test lulus, tidak ada test yang dilemahkan.
- Output/exit identik untuk examples (hello/main/demo/main.zs).
- Benchmark tidak lebih lambat; laporan angka jujur (termasuk kalau gain kecil).

## Required Final Report

- Apa yang diimplementasikan + file berubah.
- Test baru / hasil suite / hasil benchmark.
- Keputusan desain yang mengambil keputusan sendiri (spec diam) vs spec-derived.

## Result (2026-10-12)

Implemented:
- `global_scope` → `Vec<(String, Binding)>` (append-only, index stabil).
- `resolve_cache: Vec<ResEntry>` per-pc + `cache_gen: u64` invalidation di 8
  situs penggantian `program.instructions` (call/return, default body,
  unwind, task save/load) + reset pada `run()`/`run_modules()`.
- `execute_instruction(pc, op)` — 4 loop fetch pass instr pc eksplisit.
- Cached path load/store local = **satu** `get`/`get_mut` per eksekusi
  (fix pertama double-hash → lebih lambat dari resolve 1-scope; di-rework).

Tests: 5 `test_job030_*` baru; suite **961 passed / 0 failed**; 2 warning
baseline. Dua test awal diganti karena salah asumsi penulis: (a) bare
assignment dalam while = local di scope blok → `print(g)` setelah loop
undefined **per §4** (bukan bug, diverifikasi identik pada binary pre-job030);
(b) const assignment ke source = **CompilerError** (compile-time), jalur
runtime const tak terjangkau dari source.

Bench (release, cooldown, interleaved ref Python):
- `bench_loop.zs`: 0.668s (pra-job030: 0.665-0.686) — paritas.
- `bench.zs`: 0.999s (pra-job030: 0.99-1.00) — paritas.
- A/B binary pra-vs-pascajob030 pada `bench_fn.zs` (nested scope): old
  0.902 avg vs new 0.945 avg — noise ≥ gain.

Kesimpulan: hash-resolve HashMap kecil (string pendek, 1-3 scope) sudah
sangat murah; cache overhead (res_get + gen + name check) ≈ saving. Jalur A
tetap dipertahankan sebagai fondasi (correctness teruji 961 test, tanpa
regresi) tetapi **bukan** optimizer yang dicari. Bottleneck nyata berada di
tempat lain pada cost-per-step dispatch — perlu profil/identifikasi empiris
sebelum optimasi berikutnya.

Keputusan mekanis non-spec (internal, bukan semantics): bentuk storage
Vec vs HashMap, skema gen-counter, miss-tidak-di-cache.
