Job 021 — Closure in Data Structures: Shared Environment Semantics

Objective

Menutup gap "Closure in data structures" yang tercatat di "CHECK.md" (§19
candidate gap Job 008/009, baris audit Job 008 #6, dan §18 bullet): saat
sebuah closure disimpan ke dalam list/object/environment binding,
runtime MENCLONE environment-nya sehingga salinan di dalam struktur data
menjadi closure dengan state yang terpisah — melanggar shared-state
semantics closure.

Situs bug (Bukti di source):

- src/vm.rs:215-226 — `heap_object_for_element` (jalur penyimpanan ke
  list/object/env): arm `Value::Closure` membaca bindings dari env
  asli, `bindings.clone()`, lalu meng-allocate `Environment` BARU, dan
  `HeapObject::Closure` menunjuk ke env baru itu.
- src/vm.rs:178-193 — pola clone IDENTIK pada konversi
  `Value -> RuntimeValue` (jalur boundary native §145/§146).
- Efek: `let holder = [bump]` membuat `holder[0]` berbagi TIDAKHING
  dengan `bump` — mutasi captured variable lewat satu tidak terlihat
  oleh yang lain, padahal keduanya adalah closure yang sama dari lexical
  scope yang sama.

Dasar spec:

- ZeroVM.md §130 (Closure Environment): "Closure menyimpan function
  value bersama environment tempat fungsi tersebut dibuat", "Beberapa
  closure dapat berbagi environment yang sama", "Perubahan captured
  variable dapat terlihat oleh closure lain yang berbagi environment
  tersebut."
- Language.md §31 (Closure): lexical closure, captured variable dapat
  dimodifikasi dan tetap hidup selama dibutuhkan closure.
- Language.md §35 (List / Object References): "List dan object
  menggunakan shared reference semantics" — nilai yang disimpan ke
  dalam struktur data mempertahankan identitasnya.

Reading yang diadopsi (wajib dilaporkan, lihat Required Final Report):
menyimpan sebuah closure value ke dalam struktur heap harus MEMPERTAHANKAN
identitas environment — clone environment untuk penyimpanan tidak
diijinkan oleh §130/§31/§35. Jika selama implementasi ditemukan sinyal
spec yang bertentangan atau keheningan yang membuat reading ini ragukan:
BERHENTI, catat di "ISSUES.md", jangan memutuskan diam-diam.

Depends on: Job 008 (shared reference semantics, closure/scope stack)
dan Job 020 (GC roots) — perbaikan ini menyimpan env yang sama ke dalam
struktur heap, sehingga ketersediaan/tracing env yang sudah dijamin
`child_refs` (heap.rs) dan `gc_roots()` (vm.rs) wajib diverifikasi ulang
dengan test tekanan GC. Perilaku hasil Job 001-020 WAJIB bertahan.

---

Required Reading — WAJIB

Sebelum mengubah apa pun:

1. "AGENTS.md"
2. "ZeroVM.md" §130 (Closure Environment — fokus pada aturan berbagi
   environment), §118 (Heap), §119 (Runtime Values), §121/§131 (roots &
   tracing — untuk verifikasi GC), §130's interaksi dengan §122
3. "Language.md" §28 (Nested Functions), §29 (Anonymous Functions),
   §31 (Closure), §35 (List / Object References), §66/§74 jika menyentuh
   live binding (Modules.md untuk §66/§74)
4. "ISSUES.md" — baca entri terkait (mis. §58b family) sebelum menambah
   entri baru
5. "CHECK.md": baris audit Job 008 #6, §19 candidate gap closure,
   §18 bullet, §8 rows GC (setelah Job 020), bullet §17 Job 020 (868)
6. jobs/008-correctness-scope-refs.md, jobs/020-gc-roots.md
7. source: src/vm.rs (`heap_object_for_element`, `to_runtime_value`,
   `from_heap_ref`, seluruh pemanggil boxing — daftar di Scope B),
   src/heap.rs (`HeapObject::Closure`, `Environment`, `child_refs`),
   src/gc.rs (tidak berubah — hanya dibaca)

Spec adalah source of truth. Jangan mengarang aturan closure, jangan
mengubah semantics lain "sekalian".

---

Scope

### A. Diagnosis dan perbaikan inti

- Perbaiki arm `Value::Closure` di `heap_object_for_element`
  (vm.rs:215-226): simpan env ASLI pada `HeapObject::Closure` —
  tanpa meng-allocate `Environment` clone.
- Perbaiki pola clone identik pada konversi `Value -> RuntimeValue`
  (vm.rs:178-193) dengan aturan yang sama.
- Audit seluruh repo untuk situs clone environment lain
  (`Environment(...)` + `bindings.clone()`) di luar dua situs di atas —
  jika ada, termasuk scope; jika ternyata ada clone yang
  dibutuhkan alasan spesifik, laporkan sebelum mengubahnya.
- Selama diagnosis, telusuri KENAPA clone itu ada (komitmen history jika
  tersedia, atau asumsi yang terlihat dari code/comment/test) — hasilnya
  bagian wajib Required Final Report. Pastikan penghapusan clone tidak
  bergantung pada perilaku yang ditest oleh test lain.

### B. Audit seluruh jalur boxing (13 pemanggil, daftar kerja)

`heap_object_for_element` dipanggil dari: list.add (vm.rs:3165),
build_http_response (3472, 3484), json conversion (3550), values_to_list
(3618), populate shared env saat closure creation (3963, 3977),
store_in_env (4160), execute_make_list (4207), execute_make_object
(4238), set_index (4334, 4356), set_property (4444).

Untuk SETIAP site: tentukan apakah nilai bertipe closure dapat melewatinya
dan apakah perilaku setelah perbaikan sesuai §130. Jalur `store_in_env`
dan populate-env (3963/3977/4160) adalah kasus "closure menangkap
closure" — wajib ikut berbagi. Hasil audit per-site masuk Required Final
Report.

### C. Integritas round-trip

- `from_heap_ref` pada `HeapObject::Closure` harus menghasilkan
  `Value::Closure` yang menunjuk ke env yang SAMA (identitas terjaga).
- Arm `Value::Function`/`Value::BuiltIn` (boxing TANPA env) dipertahankan
  — verifikasi invariant: fungsi yang menangkap variable SELALU berupa
  `Value::Closure` (lihat execute_load_function); jika ditemukan jalan
  di mana fungsi capturing beredar sebagai `Value::Function`, JANGAN
  memperluas scope diam-diam — laporkan sebagai temuan.
- `param_defaults` pada closure tetap ikut terbawa (sudah di-clone —
  tetap benar, nilai copy untuk list of usize).

### D. Verifikasi interaksi GC (tanpa mengubah Job 020)

- `child_refs` sudah men-trace `HeapObject::Closure.env` (heap.rs) dan
  `gc_roots()`/`collect_value_refs` sudah mengumpulkan
  `Value::Closure.env` (vm.rs) — verifikasi ini benar untuk env yang
  kini BERSAMA (satu env, banyak penunjuk), termasuk ketika closure berada
  DI DALAM list/object yang juga di-scan.
- Test tekanan threshold-0 wajib untuk closure-dalam-list (memakai
  helper `run_source_with_gc_pressure` hasil Job 020).
- JANGAN mengubah root set, trigger, atau algoritma GC (out of scope
  Job 020).

### E. Bukti observable (§130)

Shared-state harus teramati DI KEDUA ARAH dan antar saudara:

1. mutasi lewat salinan di dalam list terlihat oleh closure asli;
2. mutasi lewat closure asli terlihat oleh salinan di dalam list;
3. dua closure dari scope yang sama tetap saling terlihat setelah salah
   satunya disimpan ke struktur data (aturan §130 "berbagi environment").

---

Out of Scope

- Default parameters saat call time (Language.md §30 — "Default
  dievaluasi saat function dipanggil"): gap terpisah, kandidat job
  berikutnya; JANGAN disentuh.
- `e.location`/source position, task slot reclamation, export alias,
  bytecode file format, multi-arg print — gap terpisah.
- Mekanisme pembuatan closure (execute_load_function, penentuan
  captured set, pembentukan env) — hanya diubah jika audit Scope C
  membuktikan kebutuhan; jika demikian, laporkan sebelum melanjutkan.
- Live binding (§66/§74), const/aliasing rules (ISSUES §58b), async
  task semantics, module semantics — tidak berubah.
- Root set/trigger/algoritma GC (milik Job 020).
- Redaksi spec — hanya jika menemukan ambiguitas: catat di "ISSUES.md".

---

Testing Requirements

1. Shared mutation dua arah: closure disimpan ke list, dipanggil lewat
   list, lalu dipanggil langsung — captured variable harus berubah
   SECAKAR bersama (dan kebalikannya). Harus berlaku juga untuk storage
   lewat object property (`obj.f = closure`, make_object, set_property)
   dan lewat `list.add`.
2. Closure menangkap closure (jalur store_in_env / populate env):
   mutasi captured variable milik closure inner terlihat oleh versi
   lainnya.
3. Round-trip perilaku: closure di dalam list yang dibaca kembali dan
   dipanggil berperilaku identik dengan closure aslinya (state bersama).
4. Test tekanan GC: closure-dalam-list pada threshold 0
   (`run_source_with_gc_pressure`) — mutasi tetap benar, nol crash
   (verifikasi Scope D).
5. Test baru diberi prefix `test_job021_*`.
6. Regression PENUH seluruh test suite (868 passing saat job ini
   dimulai, mencakup seluruh test_closure_*, test_job008_*,
   test_job020_*, modules/async/stdlib/package) — syarat terpenting;
   perbaikan ini mengubah perilaku penyimpanan yang menyentuh banyak
   jalur.
7. Seluruh unit test gc.rs dan test Job 020 tetap pass tanpa modifikasi —
   JANGAN melemahkan test yang ada untuk membuat implementasi lulus; jika
   sebuah test lama menahan perilaku clone (tidak ditemukan saat audit
   awal — tidak ada test yang menyimpan closure ke struktur data), itu
   temuan: laporkan, jangan dihapus.

---

Acceptance Criteria

- Penyimpanan closure ke list/object/env binding mempertahankan env
  asli (clone dihapus di kedua situs); dibuktikan Testing 1-3.
- Aturan §130 teramati di kedua arah dan antar saudara; aturan §35
  (identitas nilai dalam struktur data) tidak dilanggar untuk tipe
  closure.
- Closure-dalam-list aman di bawah koleksi threshold-0 (Testing 4) —
  tracing `child_refs` + `gc_roots` atas env bersama terverifikasi.
- Seluruh suite hijau (baseline 868), nol regresi Job 001-020, tanpa
  warning baru.
- "CHECK.md" diperbarui dengan evidence konkret (file:line / nama test):
  baris audit #6 → Fixed; §19 candidate gap closure → RESOLVED; §18
  bullet closure → resolved note.
- Ambiguitas (jika ada) tercatat di "ISSUES.md"; tidak ada aturan yang
  dikarang.

---

Required Final Report

1. Files changed
2. Akar masalah: kenapa clone environment ada (temuan audit history/
   asumsi), dan perbaikan persisnya di dua (atau lebih) situs
3. Hasil audit Scope B: tabel 13 pemanggil boxing — jalur closure?,
   perilaku sesudah perbaikan
4. Integritas round-trip: identitas env dari boxed `HeapObject::Closure`
   ke `Value::Closure`; status invariant Value::Function tanpa env
   (capture-check via execute_load_function)
5. Verifikasi GC: kutip baris `child_refs`/`collect_value_refs` yang
   men-trace env bersama + nama test tekanan threshold-0
6. Tests added/changed, total test count, hasil `cargo test` PENUH
   (semua job sebelumnya)
7. Spec traceability: aturan §130/§31/§35 mana yang dibuktikan tiap
   test; reading "penyimpanan harus berbagi env" + entri "ISSUES.md"
   jika ada ambiguitas — dan eksplisitkan setiap keputusan yang tidak
   bersumber langsung dari spec
8. Konfirmasi eksplisit: nol regresi dari Job 001-020
9. Konfirmasi scope: default params (§30), e.location, task
   reclamation, export alias, bytecode format TIDAK disentuh; GC
   (Job 020) tidak berubah selain verifikasi
10. "CHECK.md" rows yang diperbarui, masing-masing dengan evidence
