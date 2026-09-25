Job 015 — Async Concurrency: Task Group, Race, Cancellation, Scheduler

Objective

Melengkapi Async/Task system dengan operasi multi-task dan scheduler yang
memungkinkan beberapa task berjalan concurrent. Bergantung penuh pada Job 014
(single task lifecycle harus sudah benar sebelum menambah multi-task).

---

Required Reading — WAJIB

1. "AGENTS.md"
2. "Async.md" §102, §103, §104, §105, §108, §109, §110, §111, §112
3. Hasil Job 014 (Task runtime representation, execution model)

---

Scope

### A. Concurrent Task Creation (§102)

- Memanggil beberapa `async function` secara berurutan TANPA `await`
  langsung membuat beberapa task aktif bersamaan (contoh §102: userTask,
  dataTask dibuat dulu, baru di-await belakangan).
- Urutan penyelesaian task tidak harus sama dengan urutan pembuatan.

### B. `task.all()` (§103)

- Menerima list of task, menunggu SEMUA selesai.
- Hasil dikembalikan sebagai list, urutan mengikuti urutan task di INPUT
  (bukan urutan selesai).
- Jika satu task manapun error → `task.all()` menghasilkan error tersebut
  (tidak menunggu representasi hasil untuk task yang belum tuntas).
- `task.all([])` → list kosong.
- Setiap elemen list wajib berupa Task; bukan Task → ERROR.

### C. `task.race()` (§104)

- Menerima list of task, mengembalikan hasil dari task PERTAMA yang selesai
  (baik sukses maupun gagal — jika yang pertama selesai adalah error, error
  itu yang diteruskan).
- Task lain yang masih berjalan TIDAK otomatis dianggap selesai maupun
  otomatis dibatalkan.
- `task.race([])` → ERROR (tidak ada task untuk ditunggu).

### D. `task.cancel()` (§105)

- Membatalkan task yang masih berjalan (state → `cancelled`).
- Task yang sudah selesai (`completed`/`failed`) → cancel tidak menghasilkan
  error, hanya no-op.
- Cancellation bersifat cooperative — tidak menjamin operasi eksternal
  langsung berhenti; jelaskan batasan ini secara jujur di implementasi
  (misal untuk operasi filesystem/http yang mungkin belum bisa diinterupsi
  di tengah jalan, sesuai kemampuan runtime saat ini).

### E. `task.wait()` (§109)

- Menunggu task sampai terminal state TANPA mengambil result.
- `completed` → selesai tanpa nilai; `failed` → meneruskan error task;
  `cancelled` → cancellation error.
- Tidak menjalankan ulang task, tidak mengubah state task.

### F. `task.timeout()` (§110)

- `task.timeout(A, time)` — `time` dalam milidetik, integer non-negatif.
- Task selesai sebelum timeout → hasil dikembalikan.
- Task belum selesai saat timeout tercapai → ERROR, task TIDAK otomatis
  dibatalkan (harus manual via `task.cancel()`, lihat contoh §110).

### G. `task.sleep()` (§111)

- `task.sleep(time)` — menunda task pemanggil tanpa memblokir task lain.
- `task.sleep(0)` memberi kesempatan scheduler memproses task lain.
- TIDAK boleh diimplementasikan sebagai blocking sleep pada thread utama
  runtime (§111 eksplisit).

### H. Task Scheduler (§112)

- Scheduler menjalankan task `pending`/`running` tanpa task yang sedang
  menunggu (`await`, `sleep`, dll) memblokir task lain yang siap jalan.
- Tidak ada jaminan urutan penyelesaian berdasarkan urutan pembuatan.
- State task harus tetap konsisten ketika banyak task aktif bersamaan.

---

Out of Scope

- `http.*` (Job 016 — tapi setelah job ini selesai, http.get/post dkk dapat
  menggunakan Task/await yang sudah lengkap)
- Real OS-level parallelism jika arsitektur ZeroVM saat ini single-threaded
  (cooperative scheduling tetap sesuai Spec selama observable behavior benar)

---

Testing Requirements

1. beberapa task dibuat sebelum di-await, keduanya progress concurrent
   (§102 contoh persis)
2. `task.all()` menunggu semua, hasil urutan sesuai input
3. `task.all([])` → list kosong
4. `task.all()` dengan salah satu task gagal → error diteruskan
5. `task.all()` elemen bukan Task → ERROR
6. `task.race()` mengembalikan hasil task tercepat
7. `task.race()` task tercepat gagal → error diteruskan
8. `task.race([])` → ERROR
9. `task.cancel()` pada task berjalan → state jadi `cancelled`
10. `task.cancel()` pada task selesai → no-op, tidak error
11. `await` pada task yang di-cancel → cancellation error
12. `task.wait()` pada masing-masing terminal state (completed/failed/cancelled)
13. `task.timeout()` task selesai sebelum batas waktu → hasil normal
14. `task.timeout()` task belum selesai saat timeout → ERROR, task tetap jalan
15. `task.sleep()` tidak memblokir task lain (test dengan beberapa task sleep
    berbeda durasi, verifikasi urutan output sesuai durasi, bukan urutan
    pembuatan — mirroring contoh §112)
16. `task.sleep(0)` tidak crash/hang
17. regression seluruh test suite sebelumnya (termasuk seluruh Job 014)

---

Acceptance Criteria

- Seluruh operasi Scope A-H berjalan sesuai contoh Spec.
- Scheduler tidak memblokir task lain saat satu task menunggu.
- Tidak ada regresi.
- "CHECK.md" diperbarui: Async/Task (§49, §101-§113) dapat ditandai selesai
  penuh HANYA jika Job 014 dan Job 015 keduanya lolos verifikasi.

---

Required Final Report

1. Files changed
2. Desain scheduler (algoritma, bagaimana banyak task di-manage dalam satu
   event loop / mekanisme yang dipilih)
3. Mekanisme cancellation dan batasannya yang jujur
4. Mekanisme timeout (bagaimana diukur, apakah pakai wall-clock time nyata)
5. Tests added, total test count, hasil `cargo test`
6. Ambiguitas dan keputusan yang diambil
7. Known limitations (terutama terkait real concurrency vs cooperative
   scheduling di single-threaded interpreter)
8. Konfirmasi eksplisit: apakah §49 & §101-§113 sekarang "Implemented" penuh
