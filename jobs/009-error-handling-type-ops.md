Job 009 — Runtime Error Handling, Type Introspection, Explicit Conversion

Objective

Implementasikan dispatch runtime untuk "try/catch/finally" sesuai §43, serta
"type()" (§53) dan explicit conversion (§54). Job ini bergantung pada Job 008
(khususnya const enforcement dan block scope, karena `catch e` mendeklarasikan
binding baru per §43).

---

Required Reading — WAJIB

1. "AGENTS.md"
2. "Language.md" §43, §53, §54
3. "Runtime.md" §150 (Execution Limits), §154 (Runtime Error Reporting)
4. Hasil Job 008 (heap-backed List/Object, block scope)
5. source: compiler.rs, vm.rs, error.rs

---

Scope

### A. Try / Catch / Finally Runtime Dispatch (§43)

Saat ini `try`/`catch`/`finally` di-parse dan di-compile tapi dieksekusi
sekadar sequential tanpa exception handler nyata (`vm.rs:1179-1188`).
Implementasikan:

- Ketika runtime error terjadi di dalam blok `try`, eksekusi meloncat ke
  blok `catch` alih-alih menghentikan program / propagate sampai top level.
- `catch e` membuat local variable `e` berisi representasi error tersebut,
  ter-scope hanya pada blok `catch` (§3 block scope dari Job 008).
- Blok `finally` SELALU dijalankan — baik `try` sukses, `try` gagal dan
  ditangkap `catch`, maupun (jika ZeroScript punya mekanisme unwind lain
  seperti return di dalam try) saat terjadi early exit.
- Nested `try/catch/finally` bekerja dengan benar (catch terdekat yang
  menangani).
- Runtime error yang tidak berada di dalam `try` tetap propagate sampai
  program berhenti, sesuai perilaku sebelumnya.
- Syntax/parse error TIDAK tertangkap oleh `try/catch` (§43, baris terakhir)
  — pastikan ini tetap benar karena parse terjadi sebelum eksekusi.

Tentukan representasi runtime untuk "error" yang ditangkap (string message,
atau struktur dengan message/type). Spec tidak merinci struktur error secara
eksplisit di luar "e merupakan local variable yang berisi error" — jika
representasi ini ambigu, cek "ISSUES.md" dan laporkan keputusan yang diambil
tanpa mengarang field yang tidak ada dasarnya di Spec.

### B. `print.err()` (disebutkan di contoh §43, StandardLibrary §42)

Implementasikan `print.err(e)` minimal agar bisa menampilkan error yang
ditangkap `catch`, karena dipakai langsung di contoh §43. Jangan implementasikan
seluruh `print.*` API di sini — itu scope Job 010. Cukup builtin minimum yang
diperlukan supaya contoh §43 berjalan.

### C. Type Introspection — `type()` (§53)

Baca §53 penuh dan implementasikan `type(value)` mengembalikan representasi
type runtime value sesuai Spec (number/string/boolean/null/list/object/
function, dst — samakan persis dengan nama yang dipakai Spec, bukan nama
internal Rust). `runtime_type_name()` di `runtime_value.rs` sudah ada secara
internal — job ini membuatnya callable dari ZeroScript sebagai builtin,
bukan membuat sistem type baru.

### D. Explicit Type Conversion (§54)

Baca §54 penuh dan implementasikan builtin conversion yang disebutkan Spec
(kemungkinan `number()`, `string()`, `boolean()` — verifikasi nama persis dan
aturan konversi, termasuk kasus gagal konversi, dari isi §54, jangan menebak
dari nama fungsi bahasa lain).

---

Out of Scope

- Standard Library string/list/object/color/file/http/json/system (Job 010+)
- Modules, Async, Dependencies
- Perubahan besar terhadap RuntimeError/VmError architecture di luar yang
  diperlukan agar catchable dari ZeroScript

---

Testing Requirements

1. try/catch menangkap runtime error (index out of bounds, sesuai contoh §43)
2. `e` berisi informasi error yang bisa ditampilkan
3. `finally` dijalankan saat try sukses
4. `finally` dijalankan saat try gagal dan tertangkap catch
5. nested try/catch/finally
6. error di luar try tetap menghentikan program seperti sebelumnya (regression)
7. `print.err(e)` menampilkan error
8. `type()` untuk setiap runtime type yang ada
9. setiap builtin conversion di §54 untuk kasus sukses
10. setiap builtin conversion di §54 untuk kasus gagal (sesuai aturan Spec)
11. regression seluruh test suite sebelumnya (termasuk hasil Job 008)

---

Acceptance Criteria

- try/catch/finally berjalan sesuai contoh persis di §43
- type() dan conversion builtin sesuai §53-§54, tanpa field/behavior yang
  tidak disebutkan Spec
- Tidak ada regresi
- "CHECK.md" diperbarui untuk baris terkait Error Handling, Type Introspection,
  Explicit Type Conversion

---

Required Final Report

1. Files changed
2. Desain exception dispatch (bagaimana VM meloncat ke catch, struktur handler)
3. Representasi runtime value untuk error, dan alasan desainnya
4. Builtin yang ditambahkan (`print.err`, `type`, conversion functions) beserta
   signature-nya
5. Tests added, total test count, hasil `cargo test`
6. Ambiguitas dari Spec yang ditemukan dan keputusan yang diambil
7. Known limitations
