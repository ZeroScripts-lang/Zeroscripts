Job 014 — Async/Await Core: async function, Task, Single-Task Lifecycle

Objective

Implementasikan fondasi asynchronous ZeroScript: `async function`, `await`,
representasi runtime Task, dan lifecycle satu task (pending → running →
completed/failed/cancelled). Concurrency lanjutan (`task.all`, `task.race`,
scheduler multi-task) ada di Job 015.

Ini adalah subsystem besar yang menyentuh compiler, VM execution model, dan
kemungkinan memerlukan event loop/scheduler internal. Baca "Async.md"
SELURUHNYA sebelum mulai.

---

Required Reading — WAJIB

1. "AGENTS.md"
2. "Async.md" §49, §101, §106, §107, §113 (baca juga §102-§105, §108-§112
   untuk konteks penuh walau detail concurrency adalah scope Job 015)
3. "ZeroVM.md" §115 (Execution Model), §117 (Call Stack & Execution Frame)
4. "Runtime.md" §150 (Execution Limits)
5. Hasil Job 007 (function/closure runtime), Job 009 (try/catch runtime
   dispatch — async error harus bisa ditangani try/catch, §49)

---

Scope

### A. `async function` & `await` Syntax (§49)

- Token/parsing untuk `async function name(...) { ... }`.
- Token/parsing untuk `await expr`.
- `await` hanya valid di dalam `async function` — di luar itu → compile-time
  ERROR.
- `async function` dapat dipanggil seperti function biasa tapi menghasilkan
  Task, bukan langsung menjalankan body sampai selesai (§101).

### B. Task Runtime Representation (§101, §106)

- Task adalah runtime value baru (bukan Number, bukan Function biasa).
- Task memiliki state: `pending`, `running`, `completed`, `failed`,
  `cancelled` (§106). State dapat diperiksa lewat `task.state(A)`
  (implementasikan builtin ini di job ini karena erat dengan lifecycle).
- Task yang `completed` menyimpan hasilnya; yang `failed` menyimpan error;
  yang `cancelled` tidak punya hasil normal.
- Task tidak bisa kembali ke state sebelumnya setelah terminal state.

### C. Task Execution Model

Tentukan mekanisme eksekusi task di ZeroVM. Spec sengaja tidak memaksa
implementasi tertentu (§101: "Task dapat berjalan secara asynchronous sesuai
kemampuan runtime", §112: "Scheduler bebas menggunakan event loop, worker,
thread, atau mekanisme internal lain"). Pilihan yang wajar untuk interpreter
single-threaded seperti ZeroVM saat ini: cooperative scheduling di dalam satu
thread (bukan OS thread paralel sungguhan), selama observable behavior sesuai
Spec. Jelaskan dan justifikasi pilihan arsitektur di Required Final Report.

Jangan menjanjikan true parallel execution jika arsitektur ZeroVM saat ini
tidak mendukungnya — §101 eksplisit menyatakan concurrency tidak menjamin
paralelisme thread sungguhan.

### D. `await` Semantics (§101, §113)

- `await task` menunggu task sampai terminal state, mengembalikan hasil jika
  `completed`.
- `await` pada task `failed` → error task tersebut diteruskan (catchable
  via try/catch, Job 009).
- `await` pada task `cancelled` → cancellation error.
- `await` pada task yang sudah `completed` sebelumnya TIDAK menjalankan
  ulang operasi — mengembalikan hasil yang sudah tersimpan (§101, §113).
- `await` terhadap non-async function call (memanggil function biasa secara
  langsung, bukan via task) — tentukan perilaku dari Spec; §49 hanya
  mencontohkan `await ambilData()` di mana `ambilData` adalah async function.
  Jika Spec tidak menyebutkan `await` terhadap non-task value, jangan
  mengarang perilaku — cek ISSUES.md dan laporkan.

### E. Task Result & Error (§107)

Baca §107 penuh (Task Result) dan implementasikan aturan yang disebutkan di
sana untuk bagaimana hasil/errror task diakses dan disimpan.

---

Out of Scope

- `task.all()`, `task.race()` (Job 015)
- `task.cancel()`, `task.wait()`, `task.timeout()`, `task.sleep()` (Job 015)
- `http.*` (Job 016, bergantung pada job ini)
- Multi-task scheduler / concurrent execution lintas banyak task sekaligus
  (Job 015) — job ini cukup untuk SATU task pada satu waktu (create → await)

---

Testing Requirements

1. `async function` dapat dideklarasikan dan dipanggil, menghasilkan Task
2. `await` di luar `async function` → compile-time ERROR
3. Task memiliki state awal yang benar setelah dibuat
4. `await task` mengembalikan hasil setelah task `completed`
5. `await` pada task `failed` meneruskan error, catchable via try/catch
6. `await` pada task `cancelled` menghasilkan cancellation error (task ini
   perlu cara untuk dibatalkan minimal secara internal untuk test — atau
   ditunda jika `task.cancel()` memang baru ada di Job 015; jika demikian,
   test ini boleh dipindah ke Job 015 dan disebutkan di final report)
7. `task.state(A)` mengembalikan string state yang benar di tiap fase
8. task yang sudah `completed`, `await` kedua tidak menjalankan ulang operasi
   (verifikasi dengan side-effect seperti counter/print)
9. nested async function calls
10. recursion di dalam async function
11. regression seluruh test suite sebelumnya

---

Acceptance Criteria

- `async function`/`await`/Task dasar berjalan sesuai contoh §49 dan §101.
- Task state lifecycle sesuai §106.
- Error async tertangkap try/catch (§49).
- Tidak ada regresi.
- "CHECK.md" diperbarui untuk baris Async/Task yang relevan — HANYA yang
  benar-benar diverifikasi di job ini (single task lifecycle), bukan seluruh
  Async.md.

---

Required Final Report

1. Files changed
2. Representasi runtime Task (struct, bagaimana terhubung ke Heap/GC)
3. Desain eksekusi task (cooperative scheduling / mekanisme lain) dan alasannya
4. Bagaimana `await` berinteraksi dengan call stack ZeroVM (apakah perlu
   mekanisme suspend/resume execution frame, atau pendekatan lain — jelaskan)
5. Integrasi dengan try/catch (Job 009)
6. Tests added, total test count, hasil `cargo test`
7. Ambiguitas dan keputusan yang diambil (khususnya bagian D soal await
   terhadap non-task)
8. Known limitations, termasuk item yang ditunda ke Job 015
