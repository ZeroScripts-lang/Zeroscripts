Job 006.5 — Garbage Collector VM Integration

Objective

Integrasikan Garbage Collector dari Job 006 ke lifecycle eksekusi ZeroVM sehingga GC dapat berjalan secara otomatis berdasarkan allocation/threshold policy yang sudah tersedia.

GC tidak boleh menjadi mekanisme manual yang harus dipanggil oleh program ZeroScript.

Job ini berfokus pada GC trigger dan integrasi lifecycle VM, bukan perubahan algoritma mark-and-sweep.

---

Required Reading — WAJIB

Sebelum melakukan perubahan apa pun, baca:

1. "AGENTS.md"
2. "Language.md"
3. "ZeroVM.md"
4. "Runtime.md"
5. "Bytecode.m"d
6. source code Heap/RuntimeValue/GC/VM yang saat ini digunakan

Gunakan implementation saat ini sebagai dasar. Jangan mengandalkan task lama sebagai source of truth.

Spec adalah source of truth. Jangan mengarang atau mengubah semantics ZeroScript yang tidak tercantum dalam spec.

---

Scope

Implementasikan:

- automatic GC trigger pada lifecycle ZeroVM
- integrasi "maybe_gc()" dengan allocation path yang relevan
- threshold/allocation policy yang sudah tersedia
- GC invocation tanpa membutuhkan pemanggilan manual dari program
- menjaga root set yang saat ini memang tersedia
- integrasi error handling GC dengan VM
- GC statistics minimum yang diperlukan untuk testing
- unit tests
- VM integration tests
- regression testing seluruh test suite

---

GC Trigger

GC harus berjalan secara otomatis berdasarkan mekanisme allocation/threshold yang sudah tersedia dari Job 006.

Contoh lifecycle:

VM allocation
     ↓
Heap allocation
     ↓
allocation threshold check
     ↓
threshold reached?
   ┌───────┴───────┐
   │               │
  no              yes
   │               │
   ↓               ↓
continue        maybe_gc()
                    ↓
              mark → sweep
                    ↓
                continue VM

Jangan membuat heuristic GC yang kompleks jika tidak diperlukan.

Jika spec belum menentukan threshold tertentu, gunakan mekanisme sederhana yang sudah tersedia pada implementation Job 006.

---

Automatic Behavior

Program ZeroScript tidak perlu dan tidak boleh mengetahui kapan GC berjalan.

GC harus menjadi bagian internal dari runtime:

ZeroScript Program
       ↓
ZeroVM
       ↓
Heap allocation
       ↓
Automatic GC when required

Jangan menambahkan syntax bahasa baru hanya untuk mengontrol GC.

Jangan mewajibkan programmer memanggil "maybe_gc()".

Jika terdapat API internal/debug untuk memaksa collection, API tersebut boleh tetap tersedia untuk testing/debugging selama tidak menjadi mekanisme normal eksekusi.

---

Allocation Integration

Identifikasi seluruh allocation path yang relevan pada ZeroVM.

Pastikan allocation yang memang menggunakan ZeroScript Heap dapat menyebabkan threshold check.

Jangan membuat duplicate allocation path.

Jika terdapat jalur seperti:

- RuntimeValue conversion
- String allocation
- List allocation
- Object allocation
- Function/closure allocation jika sudah tersedia

integrasikan hanya jika subsystem tersebut memang sudah tersedia pada implementation saat ini.

Jangan mengimplementasikan closure/module runtime sebagai bagian dari job ini.

---

Root Set

Pertahankan root set yang sudah tersedia dari Job 006.

Jangan mengarang root baru hanya agar test pass.

Root yang belum tersedia tetap menjadi limitation sampai subsystem yang menyediakannya diimplementasikan.

Contohnya:

- closure environments
- global/module bindings

Jika belum tersedia di VM, jangan membuat fake implementation hanya untuk Job 006.5.

---

GC During VM Execution

GC harus aman dipanggil ketika VM sedang menjalankan bytecode.

Pastikan collection tidak menghapus object yang masih reachable dari root VM yang tersedia.

Perhatikan minimal:

- value stack
- active call frames
- runtime values yang sedang digunakan
- heap references yang sedang aktif

Jika terdapat temporary/reference state yang dapat menjadi root selama allocation, pastikan lifecycle tersebut aman terhadap collection.

---

Error Handling

GC failure atau invalid heap state harus menghasilkan runtime error yang sesuai.

Jangan menggunakan:

- Rust panic
- undefined behavior
- dangling Rust references
- unsafe memory access

Jika existing Heap/GC API sudah memiliki error type, gunakan abstraction tersebut.

---

Out of Scope

Jangan mengimplementasikan:

- perubahan algoritma mark-and-sweep
- generational GC
- incremental GC
- concurrent GC
- compacting/moving GC
- advanced allocation strategy
- JIT
- optimizer
- function/closure runtime baru
- closure environment implementation
- module runtime
- global/module binding system
- async/task runtime
- standard library
- debugger
- profiler

Job ini hanya menyelesaikan automatic GC lifecycle integration.

---

Testing Requirements

Tambahkan test untuk:

1. allocation di bawah threshold tidak menjalankan collection yang tidak diperlukan
2. allocation yang melewati threshold memicu GC otomatis
3. object reachable tetap hidup setelah automatic GC
4. object unreachable dikumpulkan melalui automatic GC
5. unreachable cycle tetap dapat dikumpulkan
6. reachable cycle tetap hidup
7. multiple roots tetap aman
8. GC terjadi selama VM execution
9. invalid heap reference tetap menghasilkan error yang aman
10. manual/debug GC, jika masih tersedia, tidak merusak automatic GC
11. regression seluruh test Job 001–006

Jalankan:

cargo test

---

Acceptance Criteria

- GC Job 006 tetap menggunakan mark-and-sweep
- automatic GC trigger terintegrasi dengan ZeroVM
- GC tidak perlu dipanggil manual oleh program ZeroScript
- allocation path yang relevan menjalankan threshold check
- reachable objects tetap aman
- unreachable objects dapat dikumpulkan otomatis
- existing root set tetap digunakan
- tidak ada fake closure/module/global root
- tidak ada duplicate Heap/RuntimeValue/GC system
- tidak ada perubahan semantics bahasa yang tidak diperlukan
- test baru ditambahkan
- seluruh regression test pass
- "cargo test" tidak memiliki failure

---

Required Final Report

Laporkan:

1. Files changed
2. Allocation paths yang diintegrasikan
3. Automatic GC trigger mechanism
4. Threshold policy
5. Root set yang digunakan
6. Perubahan pada VM lifecycle
7. Tests added
8. Total test count
9. "cargo test" result
10. Remaining warnings
11. Known limitations
12. Intentionally unimplemented items

Jika terdapat conflict atau ambiguity antara "AGENTS.md", spec, dan implementation:

jangan menebak.

Laporkan conflict tersebut dan jelaskan keputusan yang diambil.
