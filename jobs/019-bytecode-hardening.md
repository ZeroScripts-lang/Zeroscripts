Job 019 — Bytecode & VM Hardening: Versioning, Validation, Native Call Convention

Objective

Menutup gap spesifikasi level bytecode yang tercatat sejak audit awal
("CHECK.md"): bytecode header/versioning, validation pass sebelum eksekusi,
formalisasi `CALL_NATIVE` sebagai mekanisme resmi pemanggilan standard
library, serta opcode collection (`LIST_ADD`/`LIST_REMOVE`/`OBJECT_REMOVE`)
yang saat ini didelegasikan ke stdlib tanpa representasi bytecode formal.

Job ini adalah housekeeping arsitektur — TIDAK mengubah semantics bahasa
ZeroScript yang sudah benar setelah Job 008-018. Tujuannya membuat bytecode
ZeroVM sesuai kontrak Bytecode.md secara penuh.

---

Required Reading — WAJIB

1. "AGENTS.md"
2. "Bytecode.md" §124, §125, §126, §141, §142, §143, §144, §145, §146
3. Seluruh hasil Job 008-018 (stdlib, modules, async yang sudah dibangun di
   atas mekanisme call/opcode lama — harus tetap kompatibel setelah job ini)
4. source: compiler.rs, vm.rs (fokus pada definisi enum Opcode dan execution
   loop)

---

Scope

### A. Bytecode Header & Versioning (§124, §125)

- Tambahkan version identifier eksplisit pada bytecode yang dihasilkan
  compiler (§125 contoh konseptual: header berisi `BYTECODE_VERSION`).
- ZeroVM memeriksa versi sebelum eksekusi dimulai; versi tidak didukung →
  VM Error.
- Versi bytecode terpisah dari versi bahasa ZeroScript (§125).

### B. Bytecode Validation Pass (§124)

Sebelum execution loop dimulai, validasi:

- seluruh opcode dikenali
- operand tiap opcode berformat benar
- referensi constant pool valid (index dalam batas)
- target jump berada pada lokasi instruksi valid
- metadata function valid
- entry point valid
- struktur bytecode tidak melewati batas data yang tersedia

Bytecode yang gagal validasi TIDAK BOLEH dieksekusi sama sekali (§124).

### C. Formalisasi `CALL_NATIVE` (§145, §146)

- Refactor pemanggilan builtin/stdlib (hasil Job 010, 011, 016) yang saat
  ini mungkin memakai special-case per fungsi di VM, menjadi satu mekanisme
  `CALL_NATIVE <operation>` yang konsisten, sesuai §145 ("opcode dasar
  ZeroVM tetap relatif kecil, fitur standard library berkembang tanpa
  menambah opcode baru untuk setiap fungsi").
- Pastikan seluruh builtin dari Job 010/011/016 (string.*, list.*, object.*,
  color.*, file.*, system.*, json.*, http.*, type(), conversion functions,
  print/print.err) dipanggil lewat mekanisme ini setelah refactor, TANPA
  mengubah signature atau behavior yang sudah ditest sebelumnya.
- Regression test seluruh stdlib WAJIB tetap pass persis seperti sebelumnya.

### D. Collection Opcode Formal (§144)

- Formalkan `MAKE_OBJECT`/`MAKE_LIST` (kemungkinan sudah ada sebagai
  `MakeObject`/`MakeList`) dan tambahkan representasi bytecode eksplisit
  untuk `LIST_ADD`/`LIST_REMOVE`/`OBJECT_REMOVE` jika saat ini murni
  didelegasikan ke stdlib call (§144 vs §145 — tentukan batas mana yang
  jadi opcode inti vs `CALL_NATIVE`; dokumentasikan keputusan ini karena
  Spec menyebutkan LIST_ADD dkk di §144 sebagai instruksi konseptual
  terpisah dari native operation boundary di §145).

### E. Function Metadata & Call Convention Consistency (§141, §142, §143)

- Audit bahwa `LOAD_FUNCTION`/`CALL`/`RETURN` yang sudah ada (hasil Job 004,
  007) sesuai persis dengan §141-§143 — termasuk apakah representasi
  function saat ini (`LoadConst(Function(idx))`, dicatat sebagai gap di
  CHECK.md) perlu diformalkan menjadi `LOAD_FUNCTION` standalone sesuai
  contoh konseptual §141.

---

Out of Scope

- Perubahan semantics bahasa (const, scope, reference, dsb — sudah selesai
  di Job 008)
- Fitur stdlib/module/async baru — job ini hanya me-refactor MEKANISME
  pemanggilan yang sudah ada, bukan menambah fungsi baru

---

Testing Requirements

1. bytecode dengan version yang tidak didukung → VM Error, tidak dieksekusi
2. bytecode dengan opcode tidak dikenal (uji dengan bytecode yang dibuat
   manual/dimodifikasi) → VM Error sebelum eksekusi
3. bytecode dengan jump target invalid → VM Error sebelum eksekusi
4. bytecode dengan constant pool index invalid → VM Error sebelum eksekusi
5. seluruh call builtin (representative sample dari setiap stdlib job)
   tetap bekerja identik setelah refactor ke `CALL_NATIVE`
6. `LIST_ADD`/`LIST_REMOVE`/`OBJECT_REMOVE` (atau keputusan final desainnya)
   tetap menghasilkan behavior yang sama seperti sebelum refactor
7. regression PENUH seluruh test suite dari Job 001 sampai Job 018 — ini
   adalah syarat paling penting job ini karena sifatnya refactor lintas
   subsystem

---

Acceptance Criteria

- Bytecode versioning & validation pass berjalan sesuai §124-§125.
- `CALL_NATIVE` menjadi mekanisme konsisten untuk seluruh builtin.
- TIDAK ADA regresi behavior apa pun dari Job 001-018 — job ini murni
  hardening/refactor arsitektur bytecode.
- "CHECK.md" diperbarui untuk baris Bytecode Validation, Versioning, Native
  Operation Calling, Object/List Instructions.

---

Required Final Report

1. Files changed
2. Desain header/versioning bytecode
3. Desain validator (tahapan apa saja yang diperiksa, di mana ditempatkan
   dalam pipeline sebelum execution loop)
4. Desain `CALL_NATIVE` dan bagaimana seluruh builtin existing dimigrasikan
5. Keputusan LIST_ADD/LIST_REMOVE/OBJECT_REMOVE sebagai opcode inti vs
   native call
6. Tests added, total test count, hasil `cargo test` (harus mencakup SELURUH
   test dari job-job sebelumnya, bukan hanya test baru)
7. Ambiguitas dan keputusan yang diambil
8. Konfirmasi eksplisit: nol regresi dari Job 001-018
