Job 020 — GC Roots: Root Coverage & Root Updates per §121/§131

Objective

Menutup gap "Root identification" yang tercatat di "CHECK.md" — §8 baris
Root identification (status saat ini: Partially Implemented, "live_refs only;
local vars and params not scanned"), baris audit #17 (Root coverage
incomplete), §18 "GC root coverage for local vars/params", serta dua
candidate gap pada §19 (frame locals/params/captured envs tidak di-scan;
Binding HeapRefs pada scope stack tidak di-enumerate saat mark phase).

Kondisi saat ini ( Bukti di source, bukan asumsi ):

- ZeroVM::run_gc (src/vm.rs:607-612) memakai salinan "live_refs" sebagai
  satu-satunya root. live_refs diisi lewat add_live_ref (vm.rs:591) dari
  heap_alloc (vm.rs:614-620) dan sejumlah site manual; remove_live_ref
  (vm.rs:597) tidak punya production caller. Akibatnya root efektif adalah
  RIWAYAT ALOKASI, bukan sumber runtime yang didefinisikan §121.
- Enam sumber root §121 lain tidak pernah dipindai: local variables/parameter
  pada call_stack (Frame.scopes, vm.rs:349-358), nilai Value Stack (stack,
  vm.rs:505), global bindings (global_scope, vm.rs:537), module bindings
  aktif, task yang masih berjalan (tasks + TaskContext, vm.rs:436-476), dan
  closure environment (Frame.captured_env/scope_env) selain lewat pin manual.
- Kebalikannya juga terjadi: sejumlah alokasi langsung TIDAK pernah di-pin —
  mis. make_error_object (vm.rs:3651-3660) dan operasi string di
  runtime_value.rs:155-191. Ref tersebut hanya selamat jika kebetulan
  terjangkau dari live_refs lain; ketika threshold (default 128 alokasi)
  terlampaui, berisiko di-sweep saat masih dipakai — pelanggaran §122
  ("Garbage collector tidak boleh mereklamasi object yang masih reachable").
- Sebaliknya, objek yang scope-nya sudah berakhir tetap ter-pin selamanya,
  padahal §131 mewajibkan: "Runtime harus memperbarui reference yang relevan
  ketika binding, frame, closure, atau task dibuat dan dihentikan", dan §121
  mendefinisikan root sebagai sumber runtime — bukan seluruh riwayat alokasi.

Job ini adalah perbaikan correctness/memory-safety fundamental. Semua job
(stdlib, modules, async/tasks, package) berdiri di atas perilaku GC ini,
sehingga gap ini dipilih sebagai job pertama setelah rangkaian follow-up
2026-10-02.

Depends on: Job 006 + Job 006.5 (algoritma mark-and-sweep dan trigger GC sudah
selesai) dan Job 008 (scope stack, reference semantics, shared HeapRefs).
Perilaku hasil Job 010-019 (stdlib, modules, async tasks, bytecode hardening,
package) WAJIB bertahan tanpa regresi.

---

Required Reading — WAJIB

Sebelum mengubah apa pun:

1. "AGENTS.md"
2. "ZeroVM.md" §116 (Value Stack), §117 (Call Stack & Execution Frame),
   §118 (Heap), §120 (Memory Management), §121 (Garbage Collection — daftar
   Root Reference dan Aturan), §122 (Memory Safety), §130 (Closure
   Environment), §131 (Reference Tracking — Reference Sources dan Aturan),
   §132 (Heap Allocation), §133 (Memory Limits)
3. "Runtime.md" §152 (Resource Cleanup)
4. "CHECK.md": §8 (baris Root identification), §17 baris audit #7, §18
   (GC root coverage), §19 (candidate gaps GC roots), §8 baris child_refs
5. "ISSUES.md" — baca entri yang ada sebelum menambah entri baru
6. jobs/006*.md, jobs-006.5-gc.md, jobs/008-correctness-scope-refs.md
7. source: src/gc.rs (collect/mark), src/vm.rs (run_gc, heap_alloc, Frame,
   Binding, Task, TaskContext, global_scope, jalur module run_modules),
   src/heap.rs (child_refs tracing), src/runtime_value.rs, src/native.rs

Spec adalah source of truth. Jangan mengarang root source, kebijakan, atau
aturan yang tidak tercantum di spec. Jika spec ambigu atau tidak lengkap
untuk keputusan yang diperlukan: berhenti, catat di "ISSUES.md", laporkan
dalam Required Final Report.

---

Scope

### A. Root enumeration sesuai §121 Root Reference

run_gc harus memindai (setidaknya — §121 memakai "termasuk", §131
memberi daftar Reference Sources yang lebih lengkap):

1. local variables pada Execution Frame yang aktif — call_stack[*].scopes
   (termasuk parameter fungsi pada scope terdalam)
2. nilai pada Value Stack — stack
3. global bindings — global_scope
4. module bindings yang masih aktif — identifikasi di mana state module hidup
   setelah evaluasi module (frame module_scope pada run_modules, heap
   Environment via Binding.live, pin env di vm.rs:985) dan pastikan seluruh
   export/binding aktif terjangkau
5. task yang masih berjalan — tasks[*]: kind (args/callee/env), result, dan
   context tersuspensi (TaskContext.stack/call_stack/handlers/pending_return);
   konteks task yang sedang aktif sudah berupa field ZeroVM aktif dan ikut
   terpindai bersama stack/call_stack
6. reference yang sedang digunakan oleh runtime — live_refs DIPERTAHANKAN
   sebagai pin untuk ref yang hanya dipegang sisi Rust (lokal native, jendela
   sebelum hasil disimpan), lihat hazard pada Scope C
7. closure environment (§131) — Frame.captured_env/scope_env dan turunannya
   lewat child_refs (heap.rs:284); termasuk pending_return dan handlers
   sebagai runtime-held values

### B. Audit seluruh allocation site

Setiap alokasi production — 11 site "self.heap.allocate" di vm.rs (termasuk
make_error_object vm.rs:3651-3660, pembuatan object/list di jalur runtime,
Environment di vm.rs:3908) dan heap.allocate di runtime_value.rs:155-191 —
WAJIB berakhir dapat dijangkau dari root §121 ATAU ter-pin selama dipegang
runtime. Hasil audit per-site (sebelum/sesudah) menjadi bagian wajib Required
Final Report.

### C. Root updates saat binding/frame/closure/task berakhir (§131)

§131: "Runtime harus memperbarui reference yang relevan ketika binding,
frame, closure, atau task dibuat dan dihentikan." Artinya pin/akses yang
dibuat saat binding/frame/closure/task terbentuk harus dilepas ketika yang
bersangkutan berhenti (PopScope/frame pop/scope keluar/task terminal), agar
data yang unreachable benar-benar dapat direklamasi (§121 "Object yang tidak
dapat dijangkau dapat direklamasi"), bukan ter-pin selamanya seperti sekarang.

Hazard yang WAJIB ditangani tanpa kembali ke perilaku "semua alokasi permanen":

- jendela alokasi→simpan: heap_alloc memanggil maybe_gc (vm.rs:618) sebelum
  hasil alokasi disimpan di mana pun; ref segar harus tetap aman
- ref yang dipegang lokal Rust di native/loop VM lintas pemanggilan yang
  mengalokasikan (§121 sumber terakhir: "reference yang sedang digunakan
  oleh runtime")
- nilai yang berpindah antar binding (lokal→global, param→closure) tidak
  boleh kehilangan jangkauan di tengah transisi

### D. Bukti reklamasi yang dapat ditest

Setelah A-C, jalankan program nyata dengan threshold kecil: data yang sudah
unreachable harus benar-benar di-sweep (turunnya heap.count / naiknya stats
sebagai bukti internal), sementara data yang masih dipakai bertahan.
Perhatikan: saat ini sebagian besar test GC (gc.rs, vm.rs) memakai
heap.allocate mentah atau threshold yang tidak pernah tercapai, sehingga
koleksi tidak pernah benar-benar terjadi pada eksekusi program — job ini
harus menutup celah test itu juga.

---

Out of Scope

- Trigger/threshold policy (Gc::new(128), penempatan maybe_gc) — TIDAK
  diubah kecuali diwajibkan oleh keselamatan root; §120/§121: waktu eksekusi
  GC bukan bagian dari semantics
- Algoritma GC tetap mark-and-sweep (gc.rs) — tanpa generational/hybrid baru
- Task memory reclamation (pembebasan slot Task terminal di ZeroVM.tasks,
  CHECK.md §19) — gap terpisah; job ini hanya memastikan ISI task
  (args/callee/env/result/context) tercakup sebagai root
- Closure-in-data-structure (heap_object_for_element), default parameters
  call-time, e.location source position, export alias, bytecode file format
  — gap terpisah yang tidak disentuh
- Perubahan spec — hanya jika menemukan ambiguitas, catat di "ISSUES.md"
  (spec/ISSUES.md); jangan mengubah redaksi spec dari job ini

---

Testing Requirements

1. Survival per sumber root §121: minimal satu test per sumber — objek yang
   HANYA dapat dijangkau dari frame locals, parameter, value stack, global
   bindings, scope stack, closure env, dan suspended task context bertahan
   melewati run_gc paksa / threshold kecil selama masih dipakai program
2. Reklamasi: objek yang hanya dijangkau dari binding scope yang sudah
   di-pop, frame yang selesai, atau task yang sudah terminal TIDAK lagi
   ter-root dan dapat di-sweep (bukti: heap.count menurun / stats)
3. Jalur error di bawah tekanan GC: memicu runtime error (make_error_object)
   setelah threshold terlampaui tetap menghasilkan error yang benar —
   membuktikan audit Scope B
4. Koleksi saat eksekusi nyata: test yang menjalankan program source dengan
   threshold kecil sehingga koleksi benar-benar terjadi (stats.collections > 0)
   dan output program tetap benar — bukan hanya run_gc manual di atas
   allocate mentah
5. Path native/string di bawah tekanan GC: operasi string (runtime_value.rs)
   lintas alokasi tetap benar ketika GC berjalan di tengah eksekusi
6. Regression PENUH seluruh test suite (856 passing saat job ini dimulai,
   mencakup Job 001-019) — ini syarat paling penting job ini karena root GC
   menyentuh seluruh subsystem
7. Seluruh unit test gc.rs lama tetap pass; API collect(&mut heap, &roots)
   boleh dipakai apa adanya (roots kini dihasilkan pemanggil)

---

Acceptance Criteria

- Root set saat mark = sumber runtime §121/§131, bukan riwayat alokasi.
- TANPA premature free: objek yang masih reachable dari root man pun tidak
  direklamasi (§121 Aturan "GC tidak boleh menghapus data yang masih digunakan
  oleh program"; §122) — dibuktikan Testing Requirements 1, 3, 4, 5.
- Root diperbarui saat binding/frame/closure/task berakhir (§131) —
  dibuktikan Testing Requirements 2 (unreachable benar-benar direklamasi).
- "CHECK.md" diperbarui dengan evidence konkret (file:line / nama test):
  baris §8 "Root identification" → Implemented; baris audit #7 → Fixed;
  §18 "GC root coverage" → resolved note; §19 candidate gaps (frame
  locals/params/captured envs; scope stack Binding) → RESOLVED.
- Seluruh suite hijau, nol regresi Job 001-019, tanpa warning baru.
- Setiap ambiguitas spec yang ditemui tercatat di "ISSUES.md" — tidak ada
  aturan yang dikarang di dalam implementasi.

---

Required Final Report

1. Files changed
2. Desain root enumeration: fungsi/lokasi pemindai root, struktur yang
   dipindai, cakupan task aktif vs tersuspensi vs terminal
3. Keputusan soal live_refs: peran barunya sebagai runtime pin, dan bagaimana
   jendela alokasi→simpan serta ref Rust-side native dijamin aman (§121
   sumber terakhir)
4. Hasil audit allocation site: daftar site, status rooting sebelum/sesudah
5. Bukti reklamasi: nama test + angka heap.count/stats sebelum-sesudah
6. Tests added/changed, total test count, hasil "cargo test" PENUH (semua
   job sebelumnya, bukan hanya test baru)
7. Ambiguitas dan keputusan yang diambil — sebutkan entri "ISSUES.md", dan
   eksplisitkan jika ada keputusan yang TIDAK bersumber langsung dari spec
   (jangan menyajikan keputusan implementasi sebagai aturan spec)
8. Konfirmasi eksplisit: nol regresi dari Job 001-019
9. Konfirmasi: trigger policy dan algoritma GC tidak berubah (atau alasannya
   jika terpaksa berubah)
10. "CHECK.md" rows yang diperbarui, masing-masing dengan evidence
