Job 008 — Correctness Fixes: Scope, Const, Shared References, Control Flow

Objective

Perbaiki deviasi implementasi terhadap Spec yang ditemukan lewat audit ("CHECK.md")
sebelum melanjutkan ke fitur baru (Standard Library, Modules, Async, dsb).

Job ini TIDAK menambah fitur bahasa baru. Job ini memperbaiki semantics yang
sudah diklaim "Implemented" tapi ternyata menyimpang dari Spec.

---

Required Reading — WAJIB

1. "AGENTS.md"
2. "CHECK.md" (audit findings — jadikan checklist, jangan diabaikan)
3. "Language.md" §2, §3, §4, §5, §14, §16, §17, §21, §22, §33-§41
4. "ZeroVM.md" §118 (Heap), §131 (Reference Tracking)
5. source: compiler.rs, vm.rs, heap.rs, ast.rs, parser.rs, token.rs, lexer.rs

Jangan membaca "CHECK.md" sebagai spesifikasi baru — itu adalah catatan gap.
Spec (Language.md dkk) tetap satu-satunya source of truth untuk semantics.

---

Scope

### A. Shared Reference Semantics untuk List / Object (§35, §40, §118)

Saat ini `Value::List(Vec<Value>)` dan `Value::Object(HashMap<String, Value>)`
disimpan inline dan di-clone saat assignment/pass-by-argument. Spec §35 & §40
mewajibkan shared reference semantics (mirip bagaimana `Value::Closure.env`
sudah memakai `HeapRefHandle` sejak Job 007).

Migrasikan List dan Object menjadi heap-backed (alokasi lewat `heap.rs`,
representasi `Value` menyimpan `HeapRefHandle`, bukan koleksi inline), sehingga:

- `let B = A` (A adalah list/object) membuat dua binding yang menunjuk objek
  heap yang sama, bukan dua salinan independen.
- Passing list/object sebagai argument function menghasilkan reference yang
  sama di dalam function tersebut (contoh §35: `list.add(A, 40)` di dalam
  function harus terlihat oleh caller).
- Function yang mengembalikan list/object baru menghasilkan reference baru,
  disimpan ke `let` sesuai §35 baris terakhir.

Pastikan GC (`gc.rs`) dapat men-trace heap object List/Object baru ini lewat
`child_refs`, sama seperti Environment sudah ditrace.

### B. Mutasi Nyata: SetIndex / SetProperty (§34, §39)

`execute_set_index` dan `execute_set_property` di `vm.rs` saat ini adalah
no-op — object di-pop by value, value baru langsung didorong balik ke stack
tanpa menulis ke heap. Setelah bagian A selesai, perbaiki agar:

- `A[i] = v` benar-benar menulis ke heap object yang direferensikan `A`.
- `user.key = v` benar-benar menulis property ke heap object Object.
- Key baru pada object dapat ditambahkan (§39).
- Index/key lookup tetap mengikuti aturan error yang sudah ada (out-of-bounds,
  missing key).

### C. `const` Enforcement (§2, §34, §39)

`Frame.is_const` dideklarasikan tapi tidak pernah diisi (`compiler.rs:303`
membuang field ini dengan `_`) dan tidak pernah dicek di `vm.rs`. Implementasikan:

- `const A = 10; A = 20` → compile-time atau runtime ERROR (tentukan salah
  satu secara konsisten, jelaskan alasannya di final report).
- `const A` tanpa value → `A` bernilai `null`, tetap tidak bisa diubah.
- `const` list/object: element/property mutation → ERROR (`A[0] = 30` pada
  const list, `user.nama = "Budi"` pada const object).
- Captured `const` di closure tetap immutable (`StoreCaptured` pada binding
  const → ERROR). Ini melanjutkan pekerjaan Job 007 yang secara eksplisit
  menyebutkan requirement ini tapi belum dieksekusi.

### D. Duplicate Declaration & Duplicate Key (§5, §37)

- `let A = 10; let A = 20` pada scope yang sama → ERROR, termasuk kombinasi
  `let`/`const` apa pun sesuai contoh di §5.
- Nama yang sama pada scope berbeda tetap diperbolehkan (shadowing, §3).
- Duplicate key dalam object literal yang sama → ERROR (§37).

### E. Block-Level Lexical Scope (§3)

Saat ini locals disimpan flat per-Frame (`HashMap<String, Value>`), sehingga
inner block (if/while/loop body) menimpa variable outer dengan nama sama alih-alih
membuat binding baru yang di-scope ke block tersebut. Implementasikan scope
stack per block sesuai §3:

- Variable yang dideklarasikan di inner block tidak boleh terlihat setelah
  block selesai (`if true { let A = 10 } print(A) // ERROR`).
- Variable outer tetap terbaca dari inner block selama belum di-shadow.
- Shadowing di scope berbeda diperbolehkan dan tidak saling mengganggu.

### F. Function Scope Assignment & Global Scope (§3, §4)

Ini bagian paling sensitif — baca §3 dan §4 berulang kali sebelum implementasi.

- §3 menyebutkan lookup chain: current scope → parent scope → global scope.
- §4 menyebutkan assignment terhadap nama yang sama di dalam function TIDAK
  mengubah variable outer — function scope assignment bersifat lokal.

Implementasikan sehingga:

- Function (bukan closure yang meng-capture variable tersebut) tetap bisa
  MEMBACA variable yang dideklarasikan di global/outer scope, mengikuti
  lookup chain §3.
- ASSIGNMENT terhadap nama tersebut di dalam function tidak boleh menembus
  ke luar function itu (§4), walaupun variable dengan nama sama ada di outer
  scope.
- Ini berbeda dari closure capture (Job 007) yang memang sengaja membagikan
  binding yang sama secara eksplisit lewat `LoadCaptured`/`StoreCaptured`.
  Job ini HANYA menyangkut fungsi biasa yang membaca/menulis nama yang
  kebetulan sama dengan variable di scope luar tanpa capture eksplisit.

Jika setelah membaca Spec berulang kali makna §3 dan §4 masih ambigu atau
tampak kontradiktif dengan mekanisme closure capture Job 007, JANGAN menebak.
Hentikan di titik itu, tulis analisis ambiguitasnya, cek "ISSUES.md", dan
laporkan di Required Final Report tanpa mengubah semantics secara sepihak.

Bytecode.md §127 secara eksplisit menyebutkan `LOAD_GLOBAL`/`STORE_GLOBAL`
sebagai instruksi variable ZeroVM. Implementasikan opcode ini (nama internal
boleh disesuaikan gaya kode existing) sebagai mekanisme baca/tulis global
scope bagian ini, bukan hanya workaround di level compiler. `STORE_GLOBAL`
hanya berlaku untuk binding yang memang mutable (bukan `const`), sesuai §127.
Validasi/versioning bytecode secara umum tetap di luar scope job ini (Job 018).

### G. Control Flow Validation (§17, §21, §22)

- `else if` chain harus mendukung kedalaman berapa pun (`compiler.rs:514-517`
  saat ini error di kedalaman > 2).
- `break` di luar loop → compile-time ERROR (§21).
- `continue` di luar loop → compile-time ERROR (§22).
- `break`/`continue` tidak boleh melewati function boundary (nested function
  di dalam loop body tidak mewarisi loop milik outer function).

### H. Boolean Operators (§16)

- Tambahkan token dan parsing untuk `&&` dan `||` sebagai alias `and`/`or`.
- Tambahkan parsing `not X` sebagai alias `!X` (token `Not` sudah ada di
  `token.rs` tapi `parser.rs:469` hanya menangani `Bang`).
- Semantics `&&`/`||`/`not` harus identik dengan `and`/`or`/`!` yang sudah ada
  (short-circuit, hanya menerima boolean, tidak ada truthy/falsy).

### I. Housekeeping: Naming `===` / `==`

`BinaryOp::StrictEqual` di `ast.rs` dipakai untuk `===`, padahal menurut §14
`===` adalah perbandingan raw value LONGGAR (mengabaikan type) dan `==` yang
sebenarnya strict (value + type). Perbaiki penamaan internal (enum variant,
opcode, fungsi terkait) supaya tidak menyesatkan pembaca kode berikutnya.
Pastikan PERILAKU runtime tetap sesuai §14 (tidak berubah, hanya nama yang
diluruskan) — verifikasi ulang dengan test bahwa `10 === "10"` tetap `true`
dan `10 == "10"` tetap `false` setelah rename.

---

Out of Scope

- Standard Library (string/list/object/color/file/http/json/system methods)
- Modules / Import-Export
- Async / Task
- Package & Dependency System
- Bytecode versioning/validation, native call convention
- Perubahan besar terhadap arsitektur Heap/GC di luar yang diperlukan
  untuk shared reference List/Object

---

Testing Requirements

Tambahkan test untuk setiap item di atas, minimal:

1. list assignment shares reference (`let B = A; list.add(B, ...); print(A)`)
2. object assignment shares reference
3. list/object passed to function mutated inside is visible to caller
4. function returning new list/object produces independent reference
5. `A[i] = v` mutates underlying heap object
6. `user.key = v` mutates underlying heap object, including new key
7. `const A = 10; A = 20` → ERROR
8. `const A` tanpa value → `null`, lalu assignment → ERROR
9. `const` list element mutation → ERROR
10. `const` object property mutation (existing key & new key) → ERROR
11. captured `const` mutation via closure → ERROR
12. duplicate `let`/`const` declaration di scope sama → ERROR (semua kombinasi §5)
13. shadowing di scope berbeda tetap valid
14. duplicate object literal key → ERROR
15. inner block variable tidak leak ke outer scope
16. outer variable tetap terbaca dari inner block
17. function scope assignment tidak mengubah outer variable dengan nama sama (§4 example persis)
18. function tetap bisa membaca global/outer variable tanpa capture eksplisit
19. `else if` chain kedalaman > 2 berfungsi
20. `break` di luar loop → ERROR
21. `continue` di luar loop → ERROR
22. `break`/`continue` tidak melewati function boundary
23. `&&`, `||`, `not` berperilaku identik dengan `and`, `or`, `!`
24. `10 and true` / `"hello" || false` tetap ERROR (no truthy/falsy)
25. regression seluruh test suite yang sudah ada (355 test sebelumnya)

Jalankan `cargo test` dan laporkan hasilnya secara lengkap.

---

Acceptance Criteria

- Semua item di Scope A–I terimplementasi dan diverifikasi dengan test.
- Tidak ada regresi pada 355 test yang sudah ada.
- Tidak ada semantics baru yang tidak tercantum di Spec.
- Ambiguitas (khususnya bagian F) dilaporkan secara eksplisit bila ditemukan,
  bukan ditebak.
- "CHECK.md" diperbarui: baris-baris yang relevan dengan job ini diubah
  statusnya dari "Not Implemented"/"Partially Implemented" menjadi
  "Implemented" HANYA jika benar-benar diverifikasi dengan test, disertai
  evidence baru (file:line, nama test).

---

Required Final Report

1. Files changed
2. Desain heap-backed List/Object (struct, opcode yang berubah bila ada)
3. Cara SetIndex/SetProperty menulis ke heap
4. Mekanisme const enforcement (compile-time vs runtime, dan alasannya)
5. Mekanisme duplicate declaration/key detection
6. Desain block-level scope (struct/algoritma)
7. Keputusan final untuk bagian F (function scope assignment vs global scope
   vs closure capture) dan alasannya
8. Perubahan token/parser untuk `&&`, `||`, `not`
9. Perubahan naming `===`/`==` dan file yang terdampak
10. Tests added, total test count, hasil `cargo test`
11. Baris "CHECK.md" yang diperbarui
12. Ambiguitas/konflik yang ditemukan dan bagaimana diputuskan
13. Known limitations / intentionally unimplemented items
