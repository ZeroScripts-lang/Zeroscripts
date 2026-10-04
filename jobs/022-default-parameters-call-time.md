Job 022 — Default Parameters: Call-Time Evaluation

Objective

Mengimplementasikan evaluasi default parameter **saat pemanggilan fungsi** (call time) sesuai Language.md §30:

- "Default dievaluasi saat function dipanggil."
- "Default dapat menggunakan outer variable dan parameter sebelumnya."
- "Default tidak dapat menggunakan parameter yang dideklarasikan setelahnya."

Saat ini: default expressions dikompilasi ke konstanta pada **compile time** (compiler.rs:890-908, 1754-1770, 1876-1892) -> `FunctionInfo.param_defaults: Vec<Option<usize>>` = indeks konstanta. `build_function_scope` (vm.rs:1951) hanya membaca `constant_to_value` -> evaluasi terjadi sekali saat kompilasi, bukan tiap pemanggilan.

Tujuan: ubah agar ekspresi default dievaluasi ulang setiap pemanggilan, dengan akses ke:
- Outer variables (variabel dari scope pemanggil)
- Parameter sebelumnya (yang sudah di-bind di root_scope urutan kiri ke kanan)

Depends on: Job 008 (closure/scope stack), Job 020 (GC roots -- default expressions mungkin mengalokasikan heap), Job 021 (closure shared env -- default expressions mungkin mereferensikan closure di env).

---

Required Reading -- WAJIB

1. "AGENTS.md"
2. "Language.md" §30 (Default Parameters) -- aturan evaluasi call-time, outer vars, param sebelumnya, error param sesudah
3. "ZeroVM.md" §130 (Closure Environment) -- interaksi default dengan captured env
4. "ZeroVM.md" §101 (Async Function Calls) -- default di async task creation
5. "spec/Compiler.md" jika ada, atau "spec/Bytecode.md" -- encoding default expressions
6. Source:
   - src/compiler.rs: `compile_function` (890-908, 1754-1770, 1876-1892) -- compile default expressions
   - src/compiler.rs: `FunctionInfo` struct (line 201 `param_defaults`)
   - src/vm.rs: `build_function_scope` (1940-1970) -- tempat evaluasi default sekarang
   - src/vm.rs: `execute_call` (1737-1815) -- direct call & closure call
   - src/vm.rs: `prepare_task_start` (2053-2100) -- async task start
   - src/vm.rs: `execute_make_closure` (3989-3999) -- param_defaults closure capture
7. "CHECK.md": §14 row "Default parameters" (Partially Implemented), §18 bullet, §19 candidate gap
8. "ISSUES.md" -- catat ambiguitas jika ditemukan

Spec adalah source of truth. Jangan mengubah aturan §30.

---

Scope

### A. Compiler: representasi default expression

Ubah `FunctionInfo.param_defaults` dari `Vec<Option<usize>>` (indeks konstanta) menjadi representasi yang dapat dievaluasi di runtime. Pilihan desain (pilih satu, dokumentasikan di report):

- **Opsi 1 -- Default functions**: Setiap default expression dikompilasi sebagai function body terpisah (mirip closure tanpa env), disimpan di `program.function_defaults: Vec<Option<Vec<Opcode>>>`. Evaluasi: jalankan bytecode default di frame pemanggil sebelum call asli.
- **Opsi 2 -- Inline thunk bytecode**: Kompilator memasukkan bytecode evaluasi default langsung di call site (sebelum `Call` opcode). Memerlukan opcode baru (mis. `EVAL_DEFAULT`) atau memodifikasi `Call` untuk membawa default payload.
- **Opsi 3 -- Closure-style thunk**: Default expression dikompilasi sebagai `Value::Closure` dengan env = caller's frame (tapi caller's frame belum ada saat MakeClosure). Tidak praktis.

**Rekomendasi**: Opsi 1 (default functions) -- paling bersih, reuse VM execution yang ada, parameter sebelumnya dapat diakses via argumen atau scope, outer variables via captured env dari caller (perlu mekanisme capture caller scope -- mirip §130 tapi untuk default).

Aturan kompilator (§30):
- Default yang mereferensikan parameter **sebelumnya** -> boleh (indeks parameter < indeks default)
- Default yang mereferensikan parameter **sesudahnya** -> **compile error** (validasi di compiler.rs)
- Default yang mereferensikan outer variable -> diperbolehkan, dievaluasi di scope pemanggil

### B. VM: evaluasi default di call time

Modifikasi `build_function_scope` (atau buat helper `evaluate_defaults`) untuk:
1. Terima informasi caller scope (untuk outer variable lookup) -- untuk direct call: caller's frame scopes; untuk closure call: closure's captured env + caller's frame; untuk async: task creation context.
2. Evaluasi default expressions **urutan kiri ke kanan** (parameter sebelumnya sudah ter-bind).
3. Jika default expression error -> propagate error sebelum call body jalan.
4. Hasil evaluasi dimasukkan ke `root_scope` seperti argumen eksplisit.

**Call paths yang TERDAMPAK (wajib diperbaiki semua):**
- `execute_call` -> Value::Function arm (direct call, line ~1754)
- `execute_call` -> Value::Closure arm (closure call, line ~1817)
- `prepare_task_start` (async task, line 2053) -> juga memanggil `build_function_scope`

Setiap path harus mengevaluasi default dengan **sama** (DRY: helper bersama).

### C. Closure interaction

`execute_make_closure` (vm.rs:3989) menyalin `param_defaults` dari `FunctionInfo` ke `Value::Closure`. Jika representasi default berubah (mis. indeks ke default function body), pastikan:
- `Value::Closure.param_defaults` membawa representasi yang benar
- `HeapObject::Closure.param_defaults` (heap.rs:57) menyimpan representasi yang sama
- `from_heap_ref` (vm.rs:254) round-trip dengan benar

### D. Validasi kompilator

Tambahkan validasi di `compile_function` / `compile_expression` untuk mendeteksi:
- Referensi ke parameter yang dideklarasikan **sesudah** default -> **CompileError** (bukan runtime error)
- Referensi ke nama yang tidak terdefinisi -> CompileError biasa

### E. GC & memory safety

Default expressions dievaluasi di call time -> mungkin mengalokasikan heap (string, list, object, closure). Pastikan:
- Evaluasi terjadi di **instruction boundary** (sebelum call body mulai) -> GC safe
- `Gc::on_allocation()` dipanggil saat evaluasi default mengalokasikan
- `gc_roots` sudah mencakup value stack & frame scopes -> default evaluation results aman

---

Out of Scope

- Default parameter yang merupakan **closure expression** (e.g., `f(x = () => x)`) -- jika spec §30 tidak eksplisit, catat di ISSUES.md
- `e.location` / source position di error default evaluation
- Task slot reclamation, export alias, bytecode file format
- Perubahan pada aturan §30 (spec adalah hukum)
- Penghapusan evaluasi compile-time untuk **konstanta non-default** (hanya default parameter)

---

Testing Requirements

1. **Basic call-time evaluation**: 
   ```
   let x = 1
   function f(a = x) { return a }
   x = 2
   print(f())  // harus 2 (outer var dievaluasi saat call)
   ```
2. **Outer variable mutation**:
   ```
   let counter = 0
   function inc() { counter += 1; return counter }
   function g(n = inc()) { return n }
   print(g())  // 1
   print(g())  // 2
   ```
3. **Previous parameter reference**:
   ```
   function h(a, b = a + 1) { return b }
   print(h(5))  // 6
   ```
4. **Cannot use later parameter** -> compile error:
   ```
   function bad(a = b, b) { }  // compile error: b not yet declared
   ```
5. **Explicit argument overrides default**:
   ```
   function f(a = 99) { return a }
   print(f(1))  // 1
   ```
6. **Missing arg without default -> null**:
   ```
   function f(a) { return a }
   print(f())  // null
   ```
7. **Closure call with defaults**:
   ```
   let x = 10
   function outer() {
       let x = 20
       return function(a = x) { return a }
   }
   let f = outer()
   print(f())  // 20 (closure's captured env)
   x = 30
   print(f())  // 30 (outer var di closure env)
   ```
8. **Async function with defaults**:
   ```
   async function af(a = 42) { return a }
   let t = af()
   print(await t)  // 42
   ```
9. **Default expression allocates heap** (list/object/string) under GC pressure (threshold 0).
10. **Nested defaults**: `function f(a = 1, b = a + 2, c = b + 3)` -- evaluasi berurutan.
11. **Regression**: seluruh suite (baseline 877) tetap hijau.

Semua test diberi prefix `test_job022_*`.

---

Acceptance Criteria

- Default expressions dievaluasi **di setiap pemanggilan** (bukan sekali saat kompilasi) -- dibuktikan test 1, 2, 7.
- Outer variables terlihat pada saat call (test 2, 7).
- Parameter sebelumnya bisa dipakai di default berikutnya (test 3, 10).
- Parameter sesudahnya -> **compile error** (test 4) -- tidak runtime error.
- Explicit argument override default (test 5).
- Missing arg tanpa default -> null (test 6).
- Closure call: default mereferensikan captured env & outer var (test 7).
- Async task creation: default dievaluasi di `prepare_task_start` (test 8).
- GC safe under threshold 0 (test 9).
- Representasi default di `FunctionInfo`, `Value::Closure`, `HeapObject::Closure` konsisten round-trip.
- Compile-time validation menolak default yang pakai later param (test 4).
- **Zero regresi**: 877 passed + test baru, 0 failed, warning tidak bertambah.

---

Required Final Report

1. Files changed
2. Desain representasi default expression (Opsi 1/2/3 + alasan) + perubahan `FunctionInfo.param_defaults`
3. Perubahan compiler: encoding default expressions (bytecode table, validasi later-param)
4. Perubahan VM: helper evaluasi default (`evaluate_defaults` atau modifikasi `build_function_scope`), integrasi ke 3 call paths (direct call, closure call, async task)
5. Closure & `Value::Closure` round-trip integrity
6. GC safety: bukti evaluasi default di boundary, `on_allocation`, root coverage
7. Tests added/changed, total count, `cargo test` full suite
8. Spec traceability: aturan §30 mana yang dibuktikan test mana
9. Ambiguitas (jika ada) -> entri ISSUES.md + reading yang diadopsi
10. Konfirmasi nol regresi Job 001-021
11. CHECK.md rows updated: §14 row Default parameters -> Fixed, §18 bullet resolved, §19 gap -> RESOLVED, §17 bullet + test count
