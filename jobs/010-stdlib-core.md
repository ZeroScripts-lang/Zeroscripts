Job 010 — Standard Library Core: String, List, Object, Console

Objective

Implementasikan Standard Library inti ZeroScript: `string.*`, `list.*`,
`object.*`, dan `print`/`print.err`. Job ini bergantung pada Job 008
(shared reference semantics List/Object, const enforcement) dan Job 009
(runtime error yang catchable).

---

Required Reading — WAJIB

1. "AGENTS.md"
2. "StandardLibrary.md" §42, §44, §58
3. "Language.md" §33-§40 (List/Object semantics — sudah diperbaiki di Job 008)
4. Hasil Job 008 dan Job 009
5. source: vm.rs, compiler.rs, heap.rs

---

Scope

### A. `string.*` (§42, §58)

- `string.length(A)`
- `string.contains(A, sub)`
- `string.trim(A)`
- `string.split(A, sep)` → list of string
- `string.replace(A, target, replacement)`
- `string.startsWith(A, prefix)` → boolean
- `string.endsWith(A, suffix)` → boolean
- `string.lower(A)` / `string.upper(A)`
- `string.indexOf(A, sub)` → index pertama, `-1` jika tidak ditemukan

Aturan wajib:

- Semua operasi string TIDAK mengubah string asal (string tetap immutable,
  §36).
- Argumen dengan tipe salah → ERROR (§58, berlaku untuk seluruh `string.*`,
  bukan hanya API tambahan).

### B. `list.*` (§34)

- `list.add(A, value)` — menambah element ke akhir list, memerlukan shared
  reference (Job 008 bagian A) supaya mutation terlihat oleh semua binding
  yang menunjuk list yang sama.
- `list.remove(A, index)` — menghapus berdasarkan index. Invalid index → ERROR.
- `list.length(A)`
- `const` list → seluruh operasi mutasi (`add`, `remove`) → ERROR (§34).

### C. `object.*` (§39)

- `object.remove(A, key)` — key wajib string. Missing key → tentukan
  perilaku dari Spec (§38 menyebut "Missing key → ERROR" untuk access; jika
  §39 tidak eksplisit soal remove terhadap key yang tidak ada, jangan
  mengarang — cek ISSUES.md, laporkan keputusan).
- `const` object → mengubah value, menambah key, atau menghapus key → ERROR
  (§39).

### D. Console Output (§44)

- `print(...)` — pastikan mendukung value apa pun (number, string, boolean,
  null, list, object, function) dengan representasi yang konsisten dengan
  representasi yang sudah dipakai VM saat ini.
- `print.err(...)` — hanya menulis ke error output stream, TIDAK melempar
  runtime error secara otomatis (§44, ditegaskan eksplisit di Spec). Jika
  Job 009 sudah mengimplementasikan versi minimal `print.err`, job ini
  menyempurnakannya agar konsisten dengan seluruh call site termasuk yang
  bukan dari `catch`.

---

Out of Scope

- `color.*`, `file.*`, `system.*`, `json.*`, `http.*` (Job 011+)
- Modules, Async, Dependencies
- Perubahan semantics List/Object di luar yang sudah dikunci Job 008

---

Testing Requirements

Tambahkan test untuk setiap fungsi di atas, minimal untuk tiap fungsi:
1 kasus normal, 1 kasus error (bila Spec menyatakan ada kondisi ERROR).
Tambahkan juga:

- `list.add`/`list.remove` pada `const` list → ERROR
- `object.remove` pada `const` object → ERROR
- mutation lewat `list.*`/`object.*` di dalam function terlihat oleh caller
  (memverifikasi shared reference dari Job 008 tetap berlaku lewat stdlib)
- string operation tidak mengubah string asal
- regression seluruh test suite sebelumnya

---

Acceptance Criteria

- Seluruh fungsi di §42, §44, §58 (string), §34 (list), §39 (object) berjalan
  sesuai contoh Spec persis.
- const enforcement konsisten dengan Job 008.
- Tidak ada regresi.
- "CHECK.md" diperbarui untuk section Standard Library terkait.

---

Required Final Report

1. Files changed
2. Daftar builtin yang ditambahkan beserta signature
3. Bagaimana builtin ini terhubung ke VM (opcode baru? native call mechanism?
   jelaskan desainnya)
4. Tests added, total test count, hasil `cargo test`
5. Ambiguitas dari Spec (contoh: `object.remove` missing key) dan keputusan
   yang diambil
6. Known limitations
