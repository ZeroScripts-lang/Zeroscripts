Job 023 — ZeroVM Execution Limits

Objective

Mengimplementasikan batas execution ZeroVM sesuai Runtime.md §150 dan ZeroVM.md §133:

§150 (Runtime.md:105):
- "ZeroVM dapat menerapkan batas terhadap resource yang digunakan oleh program selama execution."
- "Batas dapat mencakup: execution time, memory usage, call stack depth, task count."
- "Execution limit ditentukan oleh runtime environment."
- "Program yang melewati execution limit dapat dihentikan dengan runtime/VM error sesuai jenis batas yang dilanggar."

§133 (ZeroVM.md:638):
- "ZeroVM dapat memiliki batas maksimum Heap memory."
- "Program yang membutuhkan memory melebihi batas yang tersedia menghasilkan VM Error atau memory error."
- "Garbage collection dapat dijalankan sebelum runtime menyatakan allocation gagal."
- "Tidak ada batas memory tetap yang diwajibkan oleh bahasa ZeroScript."

Saat ini: CHECK.md:127 mencatat "Execution limits (instruction count, time, call depth) — Not Implemented". Verifikasi source: tidak ada guard sama sekali — `ZeroVM` (src/vm/mod.rs:33) menyimpan `call_stack` (:37), `tasks` (:53), `now_ms` (:63) tanpa satupun dibatasi; `Heap::allocate` (src/heap.rs:144) tidak pernah menolak; loop instruction (src/vm/mod.rs:490) tidak menghitung apa pun. Program tak terbatas (rekursi tanpa dasar, loop tanpa await, tugas tak terhenti) hanya bisa dihentikan oleh host dari luar proses.

Tujuan: memberi runtime environment cara membatasi keempat resource di §150, dengan default TANPA batas (§150 "dapat", §133 "tidak ada batas tetap yang diwajibkan") sehingga perilaku default tidak berubah dan seluruh suite tetap hijau tanpa penyesuaian.

Depends on: Job 015 (scheduler + keputusan jam virtual ISSUES §64d — penting untuk membaca §150 "execution time"), Job 019 (validasi bytecode tidak berubah — job ini tidak menyentuh bytecode), Job 020 (GC roots — memory limit bergantung pada pengukuran setelah GC), Job 022 + modularisasi vm/ (layout source saat ini).

---

Required Reading -- WAJIB

1. "AGENTS.md" (terutama §3 spec rules, §12 spec citation discipline)
2. "Runtime.md":
   - §147 (Runtime Context, :23) — "resource limits" bagian dari context; "Detail struktur Runtime Context merupakan detail internal ZeroVM"
   - §149 (Runtime Configuration, :77) — config berlaku pada execution environment bukan syntax; "Program tidak boleh mengubah batas keamanan atau resource runtime secara langsung"; "Configuration yang tidak valid sebelum program dijalankan menghasilkan startup/runtime configuration error"; "Detail format configuration file bukan bagian wajib dari syntax ZeroScript dan dapat ditentukan oleh implementasi runtime"
   - §150 (Execution Limits, :105) — empat jenis batas, ditentukan runtime environment, runtime/VM error sesuai jenis batas
   - §153 (Program Termination, :173) — "Resource limit yang tercapai dapat menyebabkan termination sesuai aturan runtime"; "Program tidak boleh terus mengeksekusi instruction setelah termination"
3. "Runtime.md" §154 (Runtime Error Reporting, :201) — kategori error "dapat disesuaikan selama jenis kegagalan dapat dibedakan secara konsisten"; VM Error dibedakan dari runtime error
4. "ZeroVM.md" §133 (Memory Limits, :638) — aturan memory limit penuh di atas
5. "ISSUES.md" §64d (Job 015) — jam virtual monotonik; keputusan bahwa sleep/timeout program-observable memakai jam virtual. Bandingkan dengan §150 "execution time" yang merupakan resource runtime, BUKAN waktu yang diamati program.
6. "ISSUES.md" §59a — `classify_runtime_error` (src/vm/error.rs) dan kategori yang ada; pembacaan baru untuk batas dicatat sebagai §71.
7. "CHECK.md": §5 baris "Execution limits" (:127), §16 Coverage Summary
8. Source (line number saat penulisan — verifikasi ulang):
   - src/vm/mod.rs: `ZeroVM` struct (:33), `run` (:246), `run_modules` (:302), `execute_instruction` (:490), `execute_call` (:921), `raise_runtime_error` (:2150), `set_program_args` (:101, contoh host API), trigger GC `self.gc.on_allocation` (:214, :1954, :2024, :2388, :2611)
   - src/vm/task.rs: `create_task` (:12), `run_scheduler` (:508), `wake_due_timers` (:396), `count_terminal` (:426)
   - src/heap.rs: `Heap` (:129, `slots`/`free_list`), `allocate` (:144)
   - src/gc.rs: `on_allocation` (:36), `collect` (:40)
   - src/vm/error.rs: `enum VmError` (:5)
   - src/vm/mod.rs loop idle park (`http_inflight` → `thread::park_timeout` di `run_scheduler`)

Spec adalah source of truth. Jangan mengubah aturan §150/§133/§149/§153. Setiap detail yang TIDAK dinyatakan spec (lihat Daftar Ambiguitas di bawah) WAJIB dicatat sebagai entri ISSUES.md §71 beserta reading yang diadopsi — jangan mengarang diam-diam.

---

Daftar Ambiguitas (WAJIB dianalisis & dicatat di ISSUES.md §71 sebelum/during implementasi)

Spec menetapkan batasnya, bukan mekanismenya. Butir-butir berikut mekanis/diam-diam dan wajib dapat ditelusuri ke reading §71:

1. **Kategori error per jenis batas.** §150: "runtime/VM error sesuai jenis batas yang dilanggar" — spec tidak menentukan mana yang mana. §133 eksplisit memory → "VM Error atau memory error". Tentukan (dengan reading): mana yang catchable (runtime error, §154) vs VM Error (tidak catchable), dan kategori `classify_runtime_error` baru jika perlu — §154 mengizinkan penyesuaian kategori.
2. **"Execution time" vs jam virtual.** §64d memutuskan jam VIRTUAL untuk sleep/timeout program-observable. §150 berbicara tentang resource runtime ("resource yang digunakan oleh program"). Loop `while true {}` tanpa await tidak pernah maju `now_ms`. Reading yang harus diteliti: execution time limit = waktu riil yang disample di instruction boundary (bukan jam virtual), sehingga tetap bekerja pada busy loop dan saat scheduler park menunggu HTTP (ISSUES §65). Jangan mencampur jam program-observable dengan limit resource tanpa catatan reading.
3. **Apakah "instruction count" disetujui spec.** CHECK.md:127 menyebut instruction count, tetapi §150 menyebut "execution time, memory usage, call stack depth, task count" — tidak instruction count. §150 memakai "dapat mencakup" (daftar ilustratif?) dan §149 "Execution limit dapat digunakan untuk mencegah program berjalan tanpa batas". Analisis dua reading; jika memakai instruction count (satu-satunya metrik progres deterministik murni-deterministik untuk test), catat alasannya. Jika reading menolaknya, gunakan sampling riil untuk time dan jangan menambah limit kelima.
4. **Metrik memory.** §133 tidak menyebut satuan. "Detail strategi allocation dan reclamation tetap menjadi tanggung jawab ZeroVM" → pilihan metrik (jumlah slot terpakai / estimasi byte / high-water) adalah detail implementasi — tetapi WAJIB dibuka-muka di laporan, diuji, dan dikaitkan ke §133.
5. **Ukuran "task count".** Total entri `self.tasks` (tabel monotonik, §152) vs jumlah task non-terminal (aktif). Resource mana yang dimaksud §150 tidak eksplisit. Pilih satu dengan reading.
6. **Konfigurasi invalid.** §149: "Configuration yang tidak valid sebelum program dijalankan menghasilkan startup/runtime configuration error" — tentukan apa yang invalid (mis. nilai 0/negatif) dan di mana error itu muncul (sebelum eksekusi program).
7. **Permukaan konfigurasi.** §149: format config bukan bagian syntax; §147: struktur context internal. Host API pada `ZeroVM` (analog `set_program_args`, src/vm/mod.rs:101) adalah implementasi wajar; flag CLI `zsc` adalah UX baru yang tidak disebut spec → lihat Out of Scope.

---

Scope

### A. Representasi konfigurasi limit

Tambahkan struktur limit internal pada `ZeroVM` (nama bebas, detail internal per §147) berisi keempat batas §150 + memory §133, default "unlimited". Host mengaturnya lewat host API publik baru (analog `set_program_args`), berlaku untuk `run` DAN `run_modules` (kedua entry point src/vm/mod.rs:246/:302). Tidak ada permukaan in-language (tidak ada binding/builtin baru — §149: program tidak boleh mengubah batas resource).

### B. Titik enforcement (empat limit §150 + memory §133)

1. **Call stack depth** — `execute_call` (src/vm/mod.rs:921): sebelum frame baru dibuat/push, cek panjang `call_stack` terhadap limit → error sesuai reading §71 butir 1. Harus mencegah pertumbuhan tak terbatas tanpa crash proses.
2. **Execution time** — sample sesuai reading §71 butir 2 (kemungkinan: sampling waktu riil murah di instruction boundary `execute_instruction` src/vm/mod.rs:490, dan di loop park scheduler task.rs:508 agar tetap bekerja saat menunggu HTTP/timer). Batas terlampaui → error sesuai reading.
3. **Instruction count** — hanya jika reading §71 butir 3 mengizinkan: hitung instruksi di boundary yang sama; deterministik untuk test.
4. **Task count** — `create_task` (src/vm/task.rs:12): cek kebijakan ukuran dari reading §71 butir 5 sebelum push → error.
5. **Memory** — pada jalur alokasi (titik `self.gc.on_allocation` src/vm/mod.rs:214 dst, atau `Heap::allocate` src/heap.rs:144 — pilih satu jalur konsisten): saat pemakaian mencapai limit, JALANKAN GC dulu (§133: "Garbage collection dapat dijalankan sebelum runtime menyatakan allocation gagal", helper `run_gc` src/vm/mod.rs:141 dengan roots `gc_roots`); ukur ulang; masih melebihi → VM Error (§133). Sampah yang dapat direklamasi tidak boleh dihitung (bukti: test 6).

Semua enforcement harus berukuran kecil (check per boundary, bukan algoritma baru), tanpa optimi prematur, dan tanpa perubahan bytecode/validator (Job 019 tidak berubah).

### C. Representasi error & termination

- Bentuk error mengikuti reading §71 butir 1 (runtime error via `raise_runtime_error` src/vm/mod.rs:2150 vs varian `VmError` baru di src/vm/error.rs:5 — jangan ubah kecatchability yang ada tanpa catatan reading §154).
- Kategori `classify_runtime_error` (src/vm/error.rs) jika ditambahkan harus memenuhi §154: "dapat disesuaikan selama jenis kegagalan dapat dibedakan secara konsisten".
- Setelah limit tercapai: termination §153 — tidak ada instruction berikutnya yang dieksekusi; tidak ada eksekusi yang "diteruskan setelah termination"; jalur error keluar ke host seperti error runtime tak-tertangani biasa (exit status host, §153 "Exit status ... bukan bagian dari semantics inti").

### D. Konfigurasi invalid

Sesuai reading §71 butir 6: nilai limit yang invalid menghasilkan startup/runtime configuration error SEBELUM program dijalankan (§149), bukan runtime error di tengah eksekusi.

---

Out of Scope

- **Flag CLI `zsc`/file config on-disk** — §149 memberi latitude format, tetapi menambah UX CLI baru tidak disebut spec; host API saja dulu. Kandidat job terpisah.
- **Permission/limit I/O** (filesystem/network/process, §149) — sistem izin terpisah, bukan execution limit; kandidat gap terpisah.
- **Perubahan bytecode, validator, compiler, opcode** — tidak ada yang berubah; job ini murni runtime.
- **`e.location`, debug info §155, task reclamation §152, value stack per frame §116** — kandidat terpisah.
- **Mengubah aturan §150/§133/§149/§153 atau keputusan jam virtual §64d** — spec adalah hukum; bedakan resource-time vs program-time dalam reading §71, jangan menulis ulang spec.
- **Mematikan limit secara diam-diam untuk membuat test lama lolos** — default unlimited harus karena defaultnya memang unlimited, bukan karena test dimatikan.

---

Testing Requirements

Semua test baru berprefix `test_job023_*`, deterministik seperlunya, diletakkan mengikuti lokasi test VM yang ada (src/vm/tests.rs — jangan ubah FQN `vm::tests::*`).

1. **Default unlimited tidak berubah**: program loop/berat berjalan seperti biasa tanpa limit terpasang; seluruh suite baseline (897 passed saat commit a25c0a6) tetap hijau.
2. **Call depth limit**: fungsi rekursif tanpa dasar dengan limit N → error tepat di batas kedalaman (atau ≥N dan <N+k, sesuai desain frame), proses TIDAK crash, catchability sesuai reading §71 butir 1; tanpa limit → perilaku lama.
3. **Execution time limit**: busy loop `while true` dengan limit waktu kecil → error dalam waktu wajar; program cepat dengan limit longgar → selesai normal (margin longgar anti-flake). Jika reading butir 3 mengizinkan, uji instruction count dengan angka presisi (deterministik penuh).
4. **Task count limit**: spawn task melewati batas → error pada task melewati batas; kebijakan jumlah (total vs non-terminal) sesuai reading butir 5 dibuktikan eksplisit oleh test.
5. **Memory limit + GC-first**: (a) alokasi sampai limit → GC dijalankan dulu, masih penuh → VM error; (b) bangun sampah yang dapat direklamasi, jalankan sampai limit yang sama setelah GC → BERHASIL (sampah tidak dihitung); (c) default tanpa limit → alokasi tanpa penolakan.
6. **Konfigurasi invalid**: limit nilai 0/invalid → startup/runtime configuration error sebelum program berjalan (§149), bukan error di tengah eksekusi.
7. **Termination §153**: setelah limit tercapai, side-effect program (mis. counter yang dicetak sebelum error) tidak bertambah lagi — tidak ada instruksi yang dieksekusi setelah termination.
8. **Tidak ada permukaan in-language**: tidak ada binding/builtin baru yang mengubah limit dari dalam program (dibuktikan tidak adanya jalur baru pada native registry — src/native.rs tidak ditambah entry untuk limit).
9. **Interaksi HTTP park**: dengan limit waktu terpasang, program yang menunggu HTTP in-flight (park loop scheduler) tetap terhenti sesuai limit (pakai mock server Job 016, limit longgar agar tidak flake) — hanya jika reading butir 2 memilih sampling riil yang mencakup loop park.
10. **Regresi**: full suite 897 + test baru → 0 failed; warning tetap persis 2 (function_index, execute_load_variable).

---

Acceptance Criteria

- Keempat limit §150 (execution time, memory usage, call stack depth, task count) dapat diatur oleh host melalui API dan berlaku di `run` serta `run_modules`.
- Memory mengikuti §133 penuh: GC dijalankan sebelum kegagalan dinyatakan; kegagalan = VM Error; metrik dibuka-muka di laporan.
- Default = unlimited: tanpa konfigurasi, tidak ada perilaku baru — seluruh baseline 897 tetap hijau dengan warning tetap 2.
- Error per limit memenuhi reading §71 butir 1 dan §154 (jenis kegagalan dapat dibedakan secara konsisten); catchability tiap limit terdokumentasi dan diuji.
- Termination mengikuti §153: tidak ada instruksi setelah limit tercapai.
- Konfigurasi invalid → startup/runtime configuration error (§149), diuji sebelum eksekusi.
- Tidak ada perubahan bytecode/compiler/validator; tidak ada permukaan in-language; tidak ada flag CLI baru.
- ISSUES.md §71 berisi seluruh butir Daftar Ambiguitas dengan reading yang diadopsi — tidak ada keputusan mekanis yang diam-diam.
- Nol regresi Job 001-022 + modularisasi vm/.

---

Required Final Report

1. Files changed
2. Bentuk konfigurasi limit (struct, nama field, host API) + mengapa cukup untuk §150/§149
3. Titik enforcement tiap limit (file:fn) + biaya per-check
4. Metrik memory yang dipilih + bukti GC-first (test) + kutipan §133
5. ISSUES.md §71: setiap butir Daftar Ambiguitas → reading yang diadopsi + mengapa (khususnya: kategori error per limit, execution-time vs jam virtual §64d, status instruction count, metrik memory, ukuran task count, aturan config invalid)
6. Tests: daftar `test_job023_*`, jumlah, hasil full suite (angka passed/failed), warning count
7. Spec traceability: §150 empat limit → test mana; §133 aturan → test mana; §149 config invalid → test mana; §153 termination → test mana
8. Konfirmasi nol perubahan bytecode/compiler/validator dan nol permukaan in-language
9. Konfirmasi nol regresi Job 001-022 + modularisasi (baseline 897)
10. CHECK.md rows updated: §5 baris "Execution limits" → Implemented (bukti test), §16 Coverage Summary dihitung ulang sesuai jumlah baris sebenarnya, candidate gap terkait jika muncul — ikuti AGENTS §11 (jangan klaim Implemented tanpa test yang menjalankan perilaku sebenarnya)
