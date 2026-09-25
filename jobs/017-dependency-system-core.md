Job 017 — Package & Dependency System Core: Manifest, Lockfile, Install, Scripts

Objective

Implementasikan fondasi Package & Dependency System ZeroScript: format
manifest, lockfile, resolusi package saat import, dan `zpm` sebagai CLI dasar
untuk install/scripts. Registry jarak jauh, publishing, dan git source
adalah scope Job 018.

Ini subsystem paling besar di seluruh Spec (20 section, §81-§100). Baca
"Dependencies.md" SELURUHNYA sebelum mulai — jangan hanya section yang
disebut di bawah.

---

Required Reading — WAJIB

1. "AGENTS.md"
2. "Dependencies.md" §81, §82, §83, §84, §85, §86, §87, §88, §89
3. Hasil Job 012-013 (Module System — package import memakai mekanisme
   module yang sama, hanya path resolution-nya beda)

---

Scope

### A. Package vs Module vs Dependency (§81)

- Path import dengan `./` atau `../` → module lokal (sudah ada, Job 012).
- Path import TANPA prefix tersebut → package dependency, harus tersedia
  di `.zsc_m/`. Tidak ditemukan → ERROR.
- Package tetap menggunakan mekanisme module yang sama (bukan sistem loading
  kedua) — reuse hasil Job 012.
- Package tidak menjadi global hanya karena terpasang.

### B. Project Manifest — `project.zson` (§82)

- Format ZSON (superset/varian JSON — periksa apakah ZSON sudah punya
  parser dari `json.*`, Job 011, atau perlu parser konfigurasi terpisah;
  jangan mengarang syntax ZSON di luar yang dicontohkan Spec).
- Field wajib: `name` (string), `version` (string), `dependencies` (object,
  key = nama package, value = version selector string).
- `project.zson` tidak dieksekusi sebagai source code, tidak dapat di-import.

### C. Dependency Lockfile — `project.lock.zson` (§84)

- Mencatat versi konkret yang dipilih resolver.
- Jika lockfile valid, versi yang tercatat dipakai (tidak auto-upgrade).
- Lockfile dibuat/diperbarui setelah instalasi berhasil.
- Lockfile bukan source code, tidak dapat di-import.

### D. Package Versioning / SemVer (§83)

- Format `MAJOR.MINOR.PATCH`.
- Implementasikan version selector matching sesuai aturan yang dicontohkan
  Spec (baca §83 penuh untuk notasi selector yang didukung — jangan
  mengasumsikan seluruh sintaks SemVer range dari npm/cargo kalau tidak
  disebutkan Spec).

### E. `zpm install` (§85)

- Baca `project.zson` → cek lockfile → resolve versi → "download" package
  → simpan ke `.zsc_m/` → update lockfile.
- Untuk job ini, sumber package boleh dibatasi ke filesystem lokal/registry
  sederhana yang bisa diuji secara deterministik (registry jarak jauh
  sungguhan ada di Job 018) — tetap ikuti alur dan struktur direktori yang
  dicontohkan Spec.
- Package tidak ditemukan / tidak ada versi yang memenuhi selector → ERROR.
- Package yang sudah ada dengan versi sesuai tidak di-download ulang.

### F. Project Scripts (§86)

- `scripts` di `project.zson`, dijalankan lewat `zpm run <nama>` (verifikasi
  nama command persis dari §86).

### G. Package Resolution During Execution & Dependency Tree (§87, §88)

- Saat program dijalankan, import package di-resolve terhadap `.zsc_m/`
  sesuai versi yang terkunci di lockfile.
- Dependency tree: package dapat memiliki dependency sendiri — tentukan
  aturan resolusi nested dependency dari §88 (flat vs nested `.zsc_m/`,
  ikuti persis yang dicontohkan Spec).

### H. Dependency Scope (§89)

- Baca §89 penuh untuk aturan scope dependency (dev vs runtime, atau bentuk
  lain sesuai Spec — jangan mengarang kategori scope yang tidak disebutkan).

---

Out of Scope

- Package registry jarak jauh, publishing, git package source, integrity
  verification, package cache lintas project, source priority (Job 018)
- Dependency conflict resolution lanjutan, removal, update command (Job 018,
  karena bergantung pada resolver penuh yang mungkin butuh registry)

---

Testing Requirements

1. parsing `project.zson` (field wajib, error jika field wajib hilang/salah
   tipe)
2. `zpm install` alur penuh: baca manifest → resolve → simpan ke `.zsc_m/` →
   tulis lockfile
3. lockfile valid → versi yang dipakai konsisten dengan lockfile, bukan versi
   terbaru
4. lockfile dihapus → resolver memilih versi terbaru yang memenuhi selector
5. version selector matching untuk beberapa kasus dari §83
6. package tidak ditemukan / versi tidak memenuhi selector → ERROR
7. import package (tanpa `./`/`../`) berhasil resolve ke `.zsc_m/`
8. import package yang belum di-install → ERROR
9. `zpm run <script>` menjalankan script sesuai `project.zson`
10. dependency tree nested sesuai aturan §88
11. regression seluruh test suite sebelumnya

---

Acceptance Criteria

- Seluruh item Scope A-H berjalan sesuai contoh Spec.
- Package import reuse mekanisme Module System (Job 012), tidak membuat
  loader kedua.
- Tidak ada regresi.
- "CHECK.md" diperbarui untuk section Dependencies yang relevan — jangan
  tandai seluruh §81-§100 selesai, hanya yang benar-benar dikerjakan job ini.

---

Required Final Report

1. Files changed
2. Desain parser ZSON (reuse json.* atau parser terpisah, dan alasannya)
3. Desain resolver versi (algoritma matching selector)
4. Struktur `.zsc_m/` yang dihasilkan
5. Desain CLI `zpm` (subcommand yang diimplementasikan)
6. Tests added, total test count, hasil `cargo test`
7. Ambiguitas dari Spec (terutama notasi version selector §83 dan dependency
   scope §89) dan keputusan yang diambil
8. Known limitations, item yang ditunda ke Job 018
