Job 012 — Module System Core: Import/Export Loading & Resolution

Objective

Implementasikan mekanisme dasar module ZeroScript: syntax import/export,
path resolution, module execution/initialization, circular dependency
detection, dan isolasi scope antar module. Job ini adalah fondasi — binding
semantics lanjutan (live binding, alias, re-export, conflict) ada di Job 013.

Ini adalah subsystem besar. Baca "Modules.md" SELURUHNYA sebelum mulai,
jangan hanya section yang disebut di bawah.

---

Required Reading — WAJIB

1. "AGENTS.md"
2. "Modules.md" §46, §47, §61, §62, §63, §64, §65, §71, §72, §73, §75, §76,
   §77, §78, §79 (baca juga §66-70, §74, §80 untuk konteks, walau detail
   binding semantics-nya adalah scope Job 013)
3. Hasil Job 008 (scope/global), Job 009 (try/catch, karena §73 menyebut
   evaluation failure dapat ditangkap try/catch)
4. source: lexer.rs, token.rs, parser.rs, ast.rs, compiler.rs, vm.rs, main.rs

---

Scope

### A. Syntax (§46, §47)

- `import { name, name2 } from "./path"`
- `export function ...`, `export const ...`
- `export { name, name2 }` (export existing declaration)
- Tidak ada `require()`, tidak ada default export (§47 eksplisit — jangan
  diimplementasikan).
- Ekstensi `.zs` tidak ditulis di import path, ditambahkan otomatis (§46, §64).

### B. Module Path Resolution (§64)

- Path relatif terhadap file yang melakukan import (BUKAN cwd).
- `./` = direktori file saat ini, `../` = satu tingkat ke atas.
- Subdirectory resolution.
- Directory import (tanpa nama file valid) → ERROR.
- Path yang tidak ditemukan setelah resolusi → ERROR.

### C. Module Initialization & Execution (§61, §71, §72, §78)

- Module dieksekusi saat pertama kali di-import; import berikutnya
  menggunakan hasil yang sudah diinisialisasi (tidak dieksekusi ulang).
- Semua `import` diproses SEBELUM kode module lain dijalankan, walaupun
  posisi `import` ada di tengah file (§78) — ini adalah static declaration,
  bukan runtime statement biasa.
- `import` hanya boleh berada di level module (bukan di dalam function,
  if, while, block lain) — validasi ini di compile-time.
- Urutan inisialisasi mengikuti dependency graph: dependency diinisialisasi
  sebelum module yang mengimpornya; dependency yang tidak saling bergantung
  mengikuti urutan import di source (§71).

### D. Circular Dependency & Self-Import (§62, §77)

- Cycle langsung (A→B→A) maupun tidak langsung → ERROR:
  "ERROR: Circular module dependency detected" (atau pesan setara yang jelas
  menyebutkan module yang terlibat).
- Self-import (module mengimpor dirinya sendiri) → ERROR, dideteksi sebagai
  kasus khusus circular dependency, terjadi saat dependency graph dianalisis
  (sebelum eksekusi).
- Tidak boleh ada module yang menerima nilai dari module lain yang setengah
  terinisialisasi.

### E. Module Not Found / Export Not Found (§63)

- Module path tidak ditemukan → ERROR: "Module not found" (atau setara).
- Named export yang diminta tidak ada di module sumber → ERROR: "Export
  'X' not found" (atau setara), case-sensitive.
- Import tidak boleh diam-diam menghasilkan `null`.

### F. Module Scope & Namespace Isolation (§65, §75, §76)

- Setiap module memiliki scope sendiri; variable/function yang tidak
  di-export bersifat private.
- Dua module boleh memakai nama variable yang sama tanpa konflik.
- `import` hanya berlaku pada scope module tempat ia dideklarasikan — TIDAK
  otomatis tersedia untuk module lain, dan tidak menjadi global.
- Nama import yang konflik dengan deklarasi lokal pada scope yang sama →
  ERROR.

### G. Module Evaluation Failure (§73)

- Jika runtime error terjadi saat module diinisialisasi, module dianggap
  gagal — export dari module tersebut tidak tersedia.
- Module yang gagal TIDAK dianggap sudah di-cache; import berikutnya
  mencoba inisialisasi ulang jika program masih berjalan.
- Error ini dapat ditangkap `try/catch` jika import terjadi pada konteks
  yang dieksekusi runtime (verifikasi bagaimana ini berinteraksi dengan
  aturan §78 bahwa import adalah static declaration — jika tampak
  kontradiktif, jangan menebak, laporkan analisisnya).

### H. Duplicate Import (§79)

- Meng-import module yang sama berkali-kali diperbolehkan (module tetap
  hanya diinisialisasi sekali).
- Local binding dengan nama yang sama dari import yang sama → ERROR
  (kecuali menggunakan alias — alias formal ada di Job 013, tapi validasi
  duplicate binding tanpa alias adalah scope job ini).

---

Out of Scope

- Live binding semantics untuk `export let`/`export const` (Job 013)
- Import alias (`as`), multiple bindings dengan alias, re-export,
  export conflict detection (Job 013)
- Package/dependency resolution (`import` tanpa `./`/`../`) — itu scope
  Dependency System (Job 016-017), bukan job ini
- Async/Task, Standard Library baru

---

Testing Requirements

1. import/export dasar (contoh persis §46, §47)
2. module hanya diinisialisasi sekali walau di-import berkali-kali (§61, §72)
3. private variable tidak dapat diakses dari luar module (§65)
4. dua module dengan variable nama sama tidak konflik (§65, §76)
5. path resolution: sibling file, subdirectory, `../`
6. directory import tanpa file valid → ERROR
7. module tidak ditemukan → ERROR
8. export tidak ditemukan → ERROR, case-sensitive
9. circular dependency (langsung A→B→A) → ERROR
10. circular dependency tidak langsung (A→B→C→A) → ERROR
11. self-import → ERROR
12. urutan inisialisasi dependency sesuai §71 (test dengan output print yang
    urutannya dapat diverifikasi)
13. import di tengah file tetap diproses sebelum kode lain (§78)
14. import di dalam function/if/while → ERROR compile-time
15. module evaluation failure → export tidak tersedia, error dapat ditangani
16. duplicate import module yang sama (tanpa alias, binding sama) → ERROR
17. duplicate import module yang sama dengan binding berbeda → valid
18. regression seluruh test suite sebelumnya

---

Acceptance Criteria

- Seluruh item Scope A-H berjalan sesuai contoh Spec.
- Tidak ada regresi.
- Ambiguitas (terutama bagian G) dilaporkan eksplisit.
- "CHECK.md" diperbarui untuk section Modules yang relevan (jangan tandai
  seluruh Modules "Implemented" — hanya yang benar-benar dikerjakan job ini;
  binding semantics lanjutan tetap "Not Implemented" sampai Job 013 selesai).

---

Required Final Report

1. Files changed
2. Desain module loader (resolusi path, cache module, dependency graph)
3. Mekanisme deteksi circular dependency
4. Bagaimana module scope diisolasi dari scope program utama
5. Interaksi dengan compiler existing (bytecode per-module? satu bytecode
   gabungan? jelaskan pilihan arsitektur)
6. Tests added, total test count, hasil `cargo test`
7. Ambiguitas dan keputusan yang diambil
8. Known limitations / bagian yang sengaja ditunda ke Job 013
