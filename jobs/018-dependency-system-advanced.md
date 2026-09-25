Job 018 — Package & Dependency System Advanced: Registry, Publish, Git Source

Objective

Melengkapi Package & Dependency System dengan operasi lanjutan: conflict
resolution, removal, update, git package source, publishing, registry,
metadata, integrity verification, cache, dan source priority. Bergantung
penuh pada Job 017.

Baca "Dependencies.md" §90-§100 SELURUHNYA sebelum mulai.

---

Required Reading — WAJIB

1. "AGENTS.md"
2. "Dependencies.md" §90, §91, §92, §93, §94, §95, §96, §97, §98, §99, §100
3. Hasil Job 017 (manifest, lockfile, resolver dasar, `.zsc_m/`)

---

Scope

Baca tiap section penuh dan implementasikan aturan yang dinyatakan Spec
secara eksplisit. Jangan mengarang detail infrastruktur (format registry
HTTP API, autentikasi publish, dsb) yang tidak dirinci Spec — jika Spec
menyatakan aturan behavior tapi tidak merinci mekanisme transport/protokol,
implementasikan versi yang paling sederhana dan dapat diuji secara lokal
(misal: registry sebagai direktori/file lokal untuk keperluan test, dengan
catatan jelas di final report bahwa ini adalah implementasi minimum yang
memenuhi kontrak Spec, bukan layanan registry publik sungguhan).

### A. Dependency Conflict Resolution (§90)
### B. Dependency Removal (§91) — kemungkinan `zpm remove <package>`
### C. Dependency Update (§92) — kemungkinan `zpm update <package>`
### D. Git Package Source (§93)
### E. Package Publishing (§94) — kemungkinan `zpm publish`
### F. Package Registry (§95)
### G. Package Metadata — `package.zson` (§96)
### H. Package Integrity Verification (§97)
### I. Package Cache (§98)
### J. Package Source Priority (§99) — urutan pencarian antara local cache,
   registry, git source, dsb bila lebih dari satu sumber dikonfigurasi
### K. Package System Completion (§100) — section penutup, gunakan sebagai
   checklist akhir bahwa seluruh Dependencies.md (§81-§100) benar-benar
   selesai setelah Job 017 + Job 018.

Untuk setiap sub-bagian di atas, baca section terkait di Spec secara penuh,
catat aturan eksplisit yang disebutkan, dan implementasikan HANYA itu.

---

Out of Scope

- Layanan registry publik sungguhan yang di-hosting online — job ini cukup
  menyediakan mekanisme yang benar secara kontrak/behavior dan dapat diuji
  lokal
- Modules, Async, Standard Library, Bytecode

---

Testing Requirements

Untuk setiap sub-bagian A-J, minimal 1 test kasus normal dan 1 test kasus
error/edge case sesuai aturan eksplisit di Spec. Tambahkan:

- regression seluruh test suite sebelumnya (termasuk seluruh Job 017)
- test end-to-end: manifest → install → publish (ke registry lokal test) →
  install ulang dari package lain → berhasil

---

Acceptance Criteria

- Seluruh aturan eksplisit di §90-§100 terimplementasi dan diuji.
- Tidak ada mekanisme yang diarang di luar yang Spec sebutkan.
- Tidak ada regresi.
- "CHECK.md" diperbarui: §81-§100 (seluruh Dependencies.md) dapat ditandai
  selesai penuh HANYA jika Job 017 dan Job 018 keduanya lolos verifikasi.

---

Required Final Report

1. Files changed
2. Desain masing-masing sub-bagian A-J (ringkas per bagian)
3. Penjelasan eksplisit bagian mana yang diimplementasikan sebagai versi
   lokal/minimum karena Spec tidak merinci infrastruktur transport
4. Tests added, total test count, hasil `cargo test`
5. Ambiguitas dan keputusan yang diambil
6. Known limitations
7. Konfirmasi eksplisit: apakah seluruh §81-§100 sekarang "Implemented" penuh
