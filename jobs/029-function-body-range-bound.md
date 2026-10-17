# Job 029 — Fix: `if`/`while` dalam `function` menghentikan eksekusi diam-diam

## Objective

Perbaiki bug pre-existing (ditemukan 2026-10-12 saat inspeksi Job 029-perf,
terbukti ada di semua build: debug/release, kode pra-pasca Job 028):

**`if`/`while` di dalam `function` → setelah jump dalam body function, eksekusi
berhenti diam-diam (exit 0, tanpa error) — Return tak pernah jalan, instruksi
pemanggil tak dipulihkan, sisa program top-level tak dieksekusi.**

Akar (terbukti via trace `ZS_TRACE`): `run_module_range(start, end)`
(`src/vm/mod.rs:574`) memakai **`end` range modul top-level** sebagai batas
loop `while self.pc < end`. Saat `execute_call` terjadi, `program.instructions`
**di-swap ke buffer body function** (ruang alamat terpisah, jump memang
body-relative — desain `return_instructions`/`remap_body_instr` yang benar), tetapi
`pc` kini dihitung dalam ruang body sementara `end` tetap angka top-level →
jump lokal body (mis. `Jump(10)` saat `end=10`) membuat loop break lebih awal
sebelum `Return` sempat dieksekusi.

## Required Reading

- ZeroVM.md §71-§72 (module ranges), §115 (execution model), §117 (call
  frame & return), §101/§112 (task context — scheduler pakai bound
  `instructions.len()` di task.rs:656, AMAN, jangan disentuh).
- ISSUES §75a (root-frame/orphan_stack — jalur `return` top-level).
- src/vm/mod.rs `run_module_range` (bound loop), `execute_call` (swap
  `program.instructions` + `return_instructions`), `execute_return` (restore).
- src/compiler.rs `remap_body_instr` (jump sengaja body-relative) +
  `compile_program`/`merge_modules` (bodies di-append ke flat utk entry_point).

## Scope

A. Fix **satu titik**: bound `run_module_range` — dalam frame yang
   `return_instructions.is_some()` (frame function/eval: instruksi sedang
   di-swap ke body), loop wajib memakai batas `instructions.len()` (body),
   bukan `end` modul. Di root frame tetap `end` modul.
B. Test baru **`test_job029_*`** yang **meng-assert output lengkap** —
   jalur module (`compile_module` + `merge_modules` + `run_modules`) karena
   itulah jalur `zsc file.zs` yang terbukti rusak; plus guard jalur `run()`.
C. Dokumentasi: laporan final + kandidat gap (test lama bentuk function+loop
   — mis. `test_job059d_break_in_try_inside_function` — MENG-assert output
   tetapi memakai `run_source`/`run()` yang bound-nya `instructions.len()`
   dan memang selalu benar; tak ada test bentuk ini di jalur MODULE
   (`run_module_range`) = jalur `zsc file.zs`. Coverage gap = salah jalur,
   bukan test tanpa assert. Test lama tidak diubah/dilemahkan).

## Out of Scope

- Refactor desain ke flat-address (men-remap jump +entry saat append) —
  desain swap-instructions sudah benar; jangan diganti.
- `run()` main loop, scheduler task.rs, default-body loop (1441) — terbukti
  sudah body-aware (`instructions.len()` / depth check), tak disentuh.
- ~~Bug lain yang terlihat terpisah: `if x` dgn Number → error
  "Conditional jump requires boolean"~~ — **DIKOREKSI (2026-10-12): ini
  bukan bug/gap, itu perilaku benar.** `Language.md` §16/§449: "Condition
  wajib boolean", "Tidak ada truthy/falsy" — keputusan final sejak Job 008
  (ISSUES §16 Resolved 2026-10-01). Menuliskannya sbg "kemungkinan gap
  truthiness" = kesalahan verifikasi penulis yang tidak membaca section-nya
  dulu, bukan temuan spec.
- Job 030-perf (jalur A resolution cache) — tetap tertunda.

## Testing Requirements

1. Test baru gagal dulu (merah) sebelum fix, hijau sesudah — membuktikan
   test memicu bug, bukan sekadar lewat.
2. Bentuk wajib: `if true {…} return` dalam function; `while false {} return`
   dalam function; `while` iterasi normal + return; `while true { break }`
   tanpa try; sisa top-level SETELAH `call()` ikut ter-execute.
3. Full suite 951+ test hijau; warning persis 2; `examples/hello.zs`,
   `examples/demo.zs`, `main.zs` output identik sebelum/sesudah fix.
4. Trace `ZS_TRACE` dihapus dari kode final (tidak boleh tersisa).

## Acceptance Criteria

- Semua bentuk (2) menghasilkan output lengkap & exit 0 normal lewat jalur
  `zsc file.zs`; return value sampai ke caller.
- Nol regresi suite; nol perubahan perilaku untuk kasus yang sudah benar
  (output identik).

## Required Final Report

1. Files changed + penjelasan fix (1 kondisi bound) + mengapa jalur lain aman.
2. Daftar test baru + bukti merah→hijau.
3. Full suite (angka) + warning + verifikasi examples.
4. Coverage gap: test lama function+loop ada di jalur run() saja, bukan jalur module — tercatat.

## Result (2026-10-12)

- **Fix**: `run_module_range` (`src/vm/mod.rs:574`) kini mem-break juga bila
  `!in_body_frame && pc >= end`; `in_body_frame = frame.return_instructions
  .is_some()` (penanda "instructions sedang di-swap ke body function").
  Root frame: bound `end` modul (perilaku lama). Frame function: bound
  `instructions.len()` = body. Satu kondisi, ~10 baris.
- **Test**: 5 baru `test_job029_*` (4 jalur module = jalur CLI yang rusak +
  1 guard jalur `run()`). Bukti merah→hijau: 4 FAILED sebelum fix, 5/5 ok
  sesudah.
- **Suite: 956 passed / 0 failed** (951 + 5); warning persis 2.
- **Repro CLI (release)**: p4 → `in/2/top`, p1 → `after-loop/top`,
  t2 → `b/7/top`, t3487 → `b/f/7` — semua lengkap (sebelumnya: kosong/
  terpotong, exit 0 diam).
- **Output identik** hello/main/demo/main.zs vs binary pra-fix (exit code
  identik termasuk main.zs exit 1).
- **Bench** (cooldown, ref Python 0,28s): loop 0,667s vs 0,665s pra-fix —
  nol regresi (fix = satu branch per iterasi range).
- **Mengapa jalur lain aman** (diverifikasi, tak disentuh): `run()` main loop
  & scheduler `run_task_step` (task.rs:656) memakai `instructions.len()`;
  default-body loop (mod.rs:1441) memakai `len` + depth check.
- **Kandidat gap baru tercatat**: test function+loop jalur module belum ada
  sebelum job ini (kini ditutup 5 test `test_job029_*`).
- ~~truthiness `if x` dgn Number → "kemungkinan aturan spec, belum
  diambil keputusan"~~ — **DIKOREKSI (2026-10-12): bukan gap.** §16/§449
  sudah final (condition wajib boolean, no truthy/falsy; ISSUES §16 Resolved).
  Kalimat aslinya ditulis dari ingatan tanpa membaca spec — kesalahan
  penulis, sudah diluruskan agar tidak menyesatkan pembaca berikutnya.
