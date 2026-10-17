Job 011 — Standard Library Extended: Color, File System, System, JSON

Objective

Implementasikan `color.*`, `file.*`, `system.*`, dan `json.*`. Job ini
bergantung pada Job 009 (try/catch runtime dispatch, karena §48 eksplisit
menyebut error filesystem ditangani lewat try/catch) dan Job 010 (pola
integrasi builtin ke VM yang sudah ditetapkan).

`http.*` TIDAK termasuk job ini karena bergantung pada `await`/async yang
belum ada (lihat Job 014-015).

---

Required Reading — WAJIB

1. "AGENTS.md"
2. "StandardLibrary.md" §45, §48, §51, §52
3. Hasil Job 009 dan Job 010
4. source: vm.rs, compiler.rs

---

Scope

### A. Color (§45)

- `color.red(A)`, `color.green(A)`, `color.yellow(A)`, `color.blue(A)`
- `color.bold(A)` — dapat dikombinasikan dengan fungsi color lain
  (`color.bold(color.red("Error"))`)
- Implementasi TIDAK boleh bergantung pada Chalk atau library Node.js
  manapun (§45 eksplisit). Gunakan ANSI escape code langsung di Rust, atau
  crate Rust minimal jika benar-benar diperlukan (evaluasi dulu apakah bisa
  tanpa dependency tambahan, sesuai prinsip AGENTS.md "Jangan menambahkan
  dependency yang tidak perlu").

### B. File System (§48)

- `file.read(path)`
- `file.write(path, content)`
- `file.exists(path)` → boolean
- `file.remove(path)`
- `file.mkdir(path)`

Aturan:

- Path relatif mengacu pada working directory saat `zerosc`/`zerovm`
  dijalankan (bukan lokasi source file — beda dengan module path resolution
  di §64).
- Error filesystem (file tidak ada, permission, dsb) menghasilkan runtime
  error yang dapat ditangkap `try/catch` (bergantung Job 009).

### C. System (§52)

- `system.args()` → list of string, TIDAK termasuk nama file program
  (§52 contoh: `zerovm app.zs hello world` → `["hello", "world"]`)
- `system.env(name)` → string, atau `null` jika tidak ada
- `system.os()` → string (contoh: `"linux"`)
- `system.arch()` → string (contoh: `"arm64"`)

### D. JSON (§51)

- `json.parse(A)` — A adalah string, parsing eksplisit (tidak ada automatic
  JSON detection, §51).
- `json.stringify(A)` — A adalah object/list, hasil string.
- Tentukan perilaku error untuk `json.parse` terhadap string yang bukan JSON
  valid (Spec tidak merinci pesan error spesifik — gunakan mekanisme runtime
  error yang sudah ada, catchable via try/catch, dan laporkan bila ada detail
  yang perlu diasumsikan).

---

Out of Scope

- `http.*` (Job 015, setelah Async selesai)
- Modules, Async, Dependencies
- Package manager (`zpm`, Job 016-017)

---

Testing Requirements

1. setiap fungsi `color.*` menghasilkan output dengan ANSI code yang benar
2. `color.bold` dapat mengombinasikan hasil fungsi color lain
3. `file.write` lalu `file.read` roundtrip
4. `file.exists` true/false
5. `file.remove` menghapus file, lalu `file.exists` → false
6. `file.mkdir` membuat direktori
7. error filesystem (path tidak ada, dsb) dapat ditangkap `try/catch`
8. `system.args()` tidak menyertakan nama file program
9. `system.env()` untuk variable yang ada dan yang tidak ada (→ null)
10. `system.os()` dan `system.arch()` mengembalikan string valid
11. `json.stringify` lalu `json.parse` roundtrip untuk object dan list
12. `json.parse` terhadap string tidak valid → error yang catchable
13. regression seluruh test suite sebelumnya

---

Acceptance Criteria

- Seluruh fungsi §45, §48, §51, §52 berjalan sesuai Spec.
- File system error terintegrasi dengan try/catch dari Job 009.
- Tidak ada dependency baru yang tidak perlu untuk color output.
- Tidak ada regresi.
- "CHECK.md" diperbarui untuk section terkait.

---

Required Final Report

1. Files changed
2. Dependency baru (jika ada) dan alasannya
3. Desain integrasi filesystem error dengan try/catch
4. Representasi JSON parse error
5. Tests added, total test count, hasil `cargo test`
6. Ambiguitas dan keputusan yang diambil
7. Known limitations (misal: batasan file system sandboxing jika ada)
