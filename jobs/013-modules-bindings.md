Job 013 — Module System Advanced: Live Binding, Alias, Re-export, Conflict

Objective

Melengkapi Module System dengan binding semantics lanjutan di atas fondasi
loader yang sudah dibangun Job 012: live binding, export mutability, import
alias, multiple bindings, re-export, dan export conflict detection. Setelah
job ini, Module System dianggap "LOCKED" sesuai §80.

---

Required Reading — WAJIB

1. "AGENTS.md"
2. "Modules.md" §66, §67, §68, §69, §70, §74, §80
3. Hasil Job 012 (module loader, scope isolation)
4. Hasil Job 008 (const enforcement, shared reference — live binding
   memerlukan mekanisme yang serupa dengan closure captured variable)

---

Scope

### A. Live Binding & Export Mutability (§66, §74)

- `export let X` mempertahankan binding ke variable ASLI di module sumber
  — bukan snapshot nilai saat export dilakukan.
- Perubahan pada `X` di module sumber terlihat oleh importer setelah
  perubahan tersebut terjadi (contoh §66: counter pattern).
- `export const` tetap immutable.
- Importer TIDAK BOLEH melakukan assignment langsung terhadap imported
  binding (`count = 10` pada binding yang di-import → ERROR), baik untuk
  binding dari `export let` maupun `export const`.
- Mekanisme yang tepat untuk live binding kemungkinan mirip dengan
  captured environment closure (HeapRef ke slot module) — evaluasi apakah
  bisa reuse mekanisme Heap yang sama tanpa membuat sistem reference kedua.

### B. Re-export (§67)

- `export { name } from "./other"` — me-re-export named export dari module
  lain tanpa membuat salinan; binding tetap mengacu ke export asli (live
  binding tetap berlaku transitif).
- Source module atau export yang di-re-export tidak ditemukan → ERROR.
- Duplicate export dalam module yang sama (termasuk kombinasi re-export +
  export lokal) → ERROR (lihat juga bagian D).

### C. Import Alias & Multiple Bindings (§69, §70)

- `import { name as localName } from "./path"` — alias hanya mengubah nama
  lokal, nama export asli tidak berubah.
- Alias bentrok dengan deklarasi lokal pada scope yang sama → ERROR.
- Satu statement import boleh mengambil banyak named export sekaligus,
  masing-masing dapat memiliki alias sendiri.
- Duplicate LOCAL BINDING (bukan duplicate nama export sumber) → ERROR.

### D. Export Conflict Detection (§68)

- Satu module tidak boleh menyediakan dua binding untuk nama export yang
  sama, baik dari kombinasi:
  - `export { X } from "./a"` + `export { X } from "./b"`
  - `export const X = ...` + `export { X } from "./other"`
- Konflik terdeteksi saat module dianalisis/diinisialisasi (sebelum atau
  selama load, bukan lazy saat dipakai).
- Case-sensitive.

---

Out of Scope

- Module loader dasar, path resolution, circular dependency (sudah Job 012)
- Package/dependency system (Job 016-017)
- Default export (eksplisit tidak menjadi bagian Spec, §47, §80)

---

Testing Requirements

1. live binding: perubahan `export let` di module sumber terlihat importer
   (contoh persis §66)
2. assignment terhadap imported binding → ERROR, untuk `export let` maupun
   `export const`
3. re-export dasar (§67 contoh barrel/facade `index.zs`)
4. re-export dari source module yang tidak ditemukan → ERROR
5. re-export export yang tidak ada di source → ERROR
6. duplicate export (const lokal vs re-export) → ERROR
7. duplicate export (re-export vs re-export nama sama dari 2 sumber) → ERROR
8. import alias dasar (§69 contoh `tambah as add`)
9. alias bentrok dengan deklarasi lokal → ERROR
10. multiple named import dalam satu statement, sebagian pakai alias
11. duplicate local binding tanpa alias (import nama sama dua kali) → ERROR
    (melengkapi kasus dasar yang sudah ditest di Job 012)
12. duplicate local binding DIHINDARI dengan alias → valid
13. regression seluruh test suite sebelumnya (termasuk seluruh test Job 012)

---

Acceptance Criteria

- Seluruh item Scope A-D berjalan sesuai contoh Spec persis.
- Live binding tidak diimplementasikan sebagai snapshot/copy nilai.
- Tidak ada regresi terhadap Job 012.
- "CHECK.md" diperbarui: Module System dapat ditandai selesai (§46-§80) HANYA
  jika seluruh item Job 012 dan Job 013 benar-benar terverifikasi test.

---

Required Final Report

1. Files changed
2. Desain live binding (bagaimana referensi ke variable asli dipertahankan)
3. Desain re-export (bagaimana binding chain ditelusuri lintas module)
4. Mekanisme deteksi export conflict
5. Tests added, total test count, hasil `cargo test`
6. Ambiguitas dan keputusan yang diambil
7. Known limitations
8. Konfirmasi eksplisit: apakah seluruh §46-§80 (Module System) sekarang
   "Implemented" penuh, atau masih ada gap — sebutkan gap yang tersisa jika ada
