# ZeroScript — Asynchronous Tasks & Concurrency

49. Async / Await

ZeroScript menggunakan:

async function
await

Contoh:

async function ambilData() {
    let data = await file.read("data.txt")
    return data
}

Pemanggilan:

let data = await ambilData()

Rules:

- "await" digunakan di dalam "async function".
- "async function" menghasilkan asynchronous operation.
- "await" menunggu operation selesai.
- "async" tidak otomatis membuat seluruh code berjalan paralel.
- Async error dapat ditangani menggunakan "try/catch".
- Promise-like mechanism tidak diekspos sebagai type bahasa.

---

101. Asynchronous Tasks

ZeroScript menyediakan konsep asynchronous task untuk menjalankan operasi yang dapat berlangsung tanpa langsung menghentikan eksekusi program.

Fungsi yang dideklarasikan dengan "async" menghasilkan asynchronous operation.

Contoh:

async function ambilData() {
    let data = await file.read("data.txt")
    return data
}

Pemanggilan fungsi asynchronous dapat menghasilkan task yang kemudian diselesaikan menggunakan "await".

let task = ambilData()
let data = await task
print(data)

Aturan

- "async function" menghasilkan asynchronous task ketika dipanggil.
- Task tidak langsung memberikan hasil akhir sebelum operasi selesai.
- "await" digunakan untuk menunggu task dan memperoleh hasilnya.
- "await" hanya dapat digunakan di dalam "async function".
- Membuat beberapa task tidak secara otomatis menjamin eksekusi paralel.
- Task dapat berjalan secara asynchronous sesuai kemampuan runtime.
- Task yang belum selesai dapat ditunggu oleh "await".
- Hasil task hanya dapat diperoleh setelah task selesai.
- Jika task menghasilkan runtime error, error tersebut diteruskan melalui "await" dan dapat ditangani dengan "try/catch".
- Task yang telah selesai menyimpan hasilnya sehingga "await" berikutnya terhadap task yang sama tidak menjalankan operasi tersebut kembali.
- Promise tidak diekspos sebagai type terpisah pada bahasa ZeroScript.

Contoh:

async function ambilUser() {
    return await http.get("https://example.com/user")
}

async function main() {
    let task = ambilUser()

    let res = await task

    print(res.body)
}

Pemanggilan:

let task = ambilUser()

membuat task, sedangkan:

let res = await task

menunggu task tersebut selesai.

Concurrency lebih lanjut seperti menjalankan beberapa task secara bersamaan dan menunggu beberapa task akan ditentukan pada bagian berikutnya.

102. Concurrent Task Execution

ZeroScript menyediakan mekanisme untuk menjalankan beberapa asynchronous task secara concurrent.

Concurrency memungkinkan program memulai beberapa operasi asynchronous tanpa harus menunggu setiap operasi selesai sebelum memulai operasi berikutnya.

Contoh:

async function ambilUser() {
    return await http.get("https://example.com/user")
}

async function ambilData() {
    return await http.get("https://example.com/data")
}

async function main() {
    let userTask = ambilUser()
    let dataTask = ambilData()

    let user = await userTask
    let data = await dataTask

    print(user.body)
    print(data.body)
}

Pada contoh tersebut, "userTask" dan "dataTask" dibuat terlebih dahulu sebelum program menunggu hasilnya.

Aturan

- Setiap pemanggilan "async function" menghasilkan task.
- Task dapat dibuat tanpa langsung menggunakan "await".
- Beberapa task dapat aktif pada waktu yang sama.
- "await" hanya menunggu task yang sedang diminta.
- Membuat task secara berurutan tidak berarti program harus menunggu task pertama selesai sebelum membuat task berikutnya.
- Concurrency tidak menjamin bahwa task dijalankan pada thread yang berbeda.
- Runtime bebas menggunakan event loop, worker, thread, atau mekanisme internal lainnya.
- Urutan penyelesaian task tidak harus sama dengan urutan task dibuat.
- Hasil setiap task tetap terpisah.
- Error pada satu task tidak secara otomatis membatalkan task lainnya.
- Jika task gagal dan hasilnya di-"await", error tersebut diteruskan ke pemanggil.

Contoh:

buat task A
buat task B
buat task C

      ↓

A ─────────────── selesai
B ─────── selesai
C ──────────────── selesai

Program tidak boleh mengasumsikan bahwa task yang dibuat lebih dahulu akan selesai lebih dahulu.

Concurrency digunakan untuk memungkinkan beberapa operasi asynchronous berjalan tanpa harus menunggu operasi sebelumnya selesai terlebih dahulu.

Detail mengenai penggabungan dan penungguan beberapa task akan ditentukan oleh API concurrency ZeroScript.

103. Task Group

ZeroScript menyediakan "task.all()" untuk menunggu beberapa asynchronous task secara bersamaan.

Contoh:

async function ambilUser() {
    return await http.get("https://example.com/user")
}

async function ambilData() {
    return await http.get("https://example.com/data")
}

async function main() {
    let tasks = [
        ambilUser(),
        ambilData()
    ]

    let results = await task.all(tasks)

    print(results[0].body)
    print(results[1].body)
}

"task.all()" menerima list berisi task dan menghasilkan list hasil dengan urutan yang sama seperti task yang diberikan.

Aturan

- "task.all(A)" menerima list task.
- Setiap elemen pada list harus berupa asynchronous task.
- "task.all()" menunggu seluruh task sampai selesai.
- Hasil dikembalikan sebagai list.
- Urutan hasil mengikuti urutan task pada input, bukan urutan task selesai.
- Task dapat selesai dalam urutan apa pun.
- Jika seluruh task berhasil, "task.all()" menghasilkan list hasil.
- Jika salah satu task menghasilkan error, "task.all()" menghasilkan error.
- Error tersebut dapat ditangani menggunakan "try/catch".
- Kegagalan satu task tidak mengubah hasil task lain yang sudah selesai.
- Task yang sudah dibuat tetap hanya dijalankan satu kali.
- "task.all([])" menghasilkan list kosong.

Contoh:

async function main() {
    let tasks = [
        ambilUser(),
        ambilData(),
        ambilConfig()
    ]

    try {
        let results = await task.all(tasks)

        print(results)
    } catch e {
        print.err(e)
    }
}

Secara konsep:

Task A ─────────────── selesai
Task B ─────── selesai
Task C ─────────────────── selesai
          ↓
      task.all()
          ↓
    [A, B, C]

"task.all()" menyediakan cara sederhana untuk menggabungkan beberapa asynchronous task tanpa mengekspos mekanisme internal concurrency ZeroVM.

104. Task Race

ZeroScript menyediakan "task.race()" untuk menunggu beberapa asynchronous task dan mengambil hasil dari task yang selesai terlebih dahulu.

Contoh:

async function serverA() {
    return await http.get("https://server-a.example")
}

async function serverB() {
    return await http.get("https://server-b.example")
}

async function main() {
    let result = await task.race([
        serverA(),
        serverB()
    ])

    print(result.body)
}

Jika "serverB" selesai lebih dahulu, "task.race()" menghasilkan hasil dari "serverB".

Aturan

- "task.race(A)" menerima list asynchronous task.
- Setiap elemen pada list harus berupa task.
- "task.race()" menunggu sampai salah satu task selesai.
- Hasil task yang selesai terlebih dahulu menjadi hasil "task.race()".
- Task lain yang masih berjalan tidak otomatis dianggap selesai.
- Task lain yang masih berjalan tidak otomatis dibatalkan.
- Jika task pertama yang selesai menghasilkan error, "task.race()" menghasilkan error tersebut.
- Error dapat ditangani menggunakan "try/catch".
- "task.race([])" → ERROR karena tidak memiliki task yang dapat ditunggu.
- Urutan task pada list tidak menentukan task mana yang menang.
- Jika beberapa task selesai pada waktu yang sama, runtime menggunakan urutan penyelesaian yang ditentukan oleh scheduler secara deterministik.

Contoh:

async function main() {
    try {
        let result = await task.race([
            http.get("https://server-a.example"),
            http.get("https://server-b.example")
        ])

        print(result.body)
    } catch e {
        print.err(e)
    }
}

"task.race()" berbeda dengan "task.all()":

task.all()
A ─────── selesai
B ───────────── selesai
C ──────── selesai
↓
tunggu semuanya
↓
[A, B, C]

Sedangkan:

task.race()
A ─────── selesai
B ──────────────── selesai
C ───────── selesai
↓
ambil hasil A

"task.race()" digunakan ketika program hanya membutuhkan task pertama yang selesai.

105. Task Cancellation

ZeroScript menyediakan mekanisme untuk membatalkan asynchronous task yang masih berjalan.

Task dapat dibatalkan menggunakan:

task.cancel(A)

"A" harus berupa asynchronous task yang masih aktif.

Contoh:

async function ambilData() {
    return await http.get("https://example.com/data")
}

async function main() {
    let dataTask = ambilData()

    task.cancel(dataTask)
}

Aturan

- "task.cancel(A)" menerima satu asynchronous task.
- Task yang masih berjalan dapat dibatalkan.
- Task yang sudah selesai tidak dapat dibatalkan kembali dan operasi tersebut tidak menghasilkan error.
- Task yang sudah dibatalkan tidak menghasilkan nilai normal.
- "await" terhadap task yang dibatalkan menghasilkan ERROR.
- Error cancellation dapat ditangani menggunakan "try/catch".
- Pembatalan task tidak otomatis membatalkan task lain.
- Pembatalan task tidak menjamin operasi eksternal langsung berhenti apabila operasi tersebut tidak mendukung cancellation.
- Runtime bertanggung jawab menghentikan atau membersihkan task sesuai kemampuan operasi yang sedang dijalankan.
- Task yang sudah dibatalkan tidak boleh dijalankan kembali.
- Task hanya dapat dibatalkan oleh kode yang memiliki reference terhadap task tersebut.

Contoh penggunaan bersama "task.race()":

async function main() {
    let taskA = serverA()
    let taskB = serverB()

    let winner = await task.race([
        taskA,
        taskB
    ])

    task.cancel(taskA)
    task.cancel(taskB)

    print(winner)
}

Pada contoh tersebut, task yang sudah selesai tidak bermasalah ketika "task.cancel()" dipanggil, sedangkan task yang masih berjalan dapat dibatalkan.

Cancellation merupakan mekanisme cooperative pada level runtime. ZeroScript tidak menjamin bahwa setiap operasi asynchronous dapat dihentikan secara langsung pada saat cancellation dilakukan.

106. Task State

Setiap asynchronous task memiliki state yang menunjukkan kondisi task selama lifecycle-nya.

State dasar task:

pending
running
completed
failed
cancelled

Task State

- "pending" — task telah dibuat tetapi belum mulai menjalankan operasi asynchronous.
- "running" — task sedang menjalankan operasi asynchronous.
- "completed" — task telah selesai dan menghasilkan nilai.
- "failed" — task telah selesai dengan runtime error.
- "cancelled" — task telah dibatalkan sebelum berhasil selesai.

Status task dapat diperiksa menggunakan:

task.state(A)

Contoh:

let dataTask = ambilData()

print(task.state(dataTask))

"task.state()" menghasilkan string:

"pending"
"running"
"completed"
"failed"
"cancelled"

Aturan

- "task.state(A)" menerima asynchronous task.
- Jika argument bukan task → ERROR.
- State dapat berubah selama lifecycle task.
- Task yang telah "completed", "failed", atau "cancelled" merupakan terminal state.
- Task tidak dapat kembali ke state sebelumnya setelah mencapai terminal state.
- Task yang "completed" menyimpan hasilnya.
- Task yang "failed" menyimpan error yang menyebabkan kegagalan.
- Task yang "cancelled" tidak memiliki hasil normal.
- Membaca state task tidak mengubah task.
- "task.state()" tidak menunggu task selesai.

Lifecycle umum:

pending
   ↓
running
   ├──→ completed
   ├──→ failed
   └──→ cancelled

Runtime tidak wajib mengekspos detail internal scheduler kepada programmer. "task.state()" hanya menyediakan status lifecycle yang telah didefinisikan oleh ZeroScript.

107. Task Result

Asynchronous task menyimpan hasil atau error setelah operasinya selesai.

Hasil task diperoleh menggunakan "await".

Contoh:

async function tambah(A, B) {
    return A + B
}

async function main() {
    let taskA = tambah(10, 20)

    let result = await taskA

    print(result) // 30
}

Aturan

- Task yang "completed" memiliki satu result.
- Result dapat berupa nilai ZeroScript apa pun yang valid.
- "await" terhadap task yang "completed" langsung menghasilkan result yang tersimpan.
- "await" terhadap task yang masih berjalan akan menunggu sampai task mencapai terminal state.
- Task yang "failed" menyimpan error.
- "await" terhadap task yang "failed" menghasilkan runtime error.
- Task yang "cancelled" menghasilkan cancellation error ketika di-"await".
- Result task tidak dapat diubah oleh programmer setelah task selesai.
- Menggunakan "await" terhadap task yang sama lebih dari sekali tidak menjalankan task kembali.
- Task yang sama dapat di-"await" oleh beberapa bagian program.
- Jika task sudah "completed", setiap "await" berikutnya menghasilkan result yang sama.

Contoh:

async function ambilData() {
    return "Hello"
}

async function main() {
    let dataTask = ambilData()

    let A = await dataTask
    let B = await dataTask

    print(A) // "Hello"
    print(B) // "Hello"
}

Secara konsep:

Task
  ↓
running
  ↓
completed
  ↓
result disimpan
  ↓
await → result
await → result

Task result menggunakan value semantics yang sama dengan nilai ZeroScript lainnya. Jika result berupa "list" atau "object", aturan reference semantics ZeroScript tetap berlaku.

108. Task Error Handling

Error yang terjadi di dalam asynchronous task disimpan pada task dan tidak langsung menghentikan seluruh program.

Contoh:

async function gagal() {
    return 10 / 0
}

async function main() {
    let taskA = gagal()

    try {
        let result = await taskA
        print(result)
    } catch e {
        print.err(e)
    }
}

Aturan

- Runtime error di dalam task menyebabkan task masuk ke state "failed".
- Error disimpan sebagai bagian dari task.
- Program tidak otomatis berhenti hanya karena sebuah task gagal.
- Error diteruskan ketika task tersebut di-"await".
- Error dapat ditangani menggunakan "try/catch".
- Jika task gagal tetapi tidak pernah di-"await", error tersebut tidak secara otomatis dilempar ke scope lain.
- Task lain yang sedang berjalan tidak otomatis dibatalkan ketika satu task gagal.
- "task.all()" menghasilkan error jika salah satu task yang ditunggunya gagal.
- "task.race()" menghasilkan error apabila task pertama yang mencapai terminal state adalah task yang gagal.
- Error yang telah ditangani melalui "try/catch" tidak menyebabkan error tersebut dilempar kembali kecuali programmer melakukan operasi yang menghasilkan error baru.

Contoh beberapa task:

async function main() {
    let taskA = ambilUser()
    let taskB = gagal()
    let taskC = ambilConfig()

    try {
        let result = await task.all([
            taskA,
            taskB,
            taskC
        ])

        print(result)
    } catch e {
        print.err(e)
    }
}

Secara konsep:

Task A ───────────→ completed
Task B ───────→ failed
Task C ───────────────→ completed

              ↓
          task.all()
              ↓
             ERROR

Kegagalan task merupakan bagian dari lifecycle task dan tidak mengubah aturan error handling ZeroScript yang sudah berlaku untuk operasi synchronous.

Un-awaited Task Failure

Jika sebuah asynchronous task gagal dan tidak pernah di-"await", runtime mencetak satu warning ke stderr ketika program selesai.

- Warning tidak melempar error ke scope lain dan tidak secara otomatis menghentikan program.
- Setiap "failed" task yang tidak pernah di-"await" menghasilkan satu warning.
- Task yang telah di-"await" tidak menghasilkan warning tambahan untuk failure yang sama.
- Error yang diteruskan ke "task.all()", "task.race()", "task.wait()", atau "task.timeout()" mengikuti aturan pada combinator tersebut: bila error tersebut (atau error lanjutannya) pada akhirnya sampai ke suatu "await", task sumbernya dianggap sudah di-"await" dan tidak menghasilkan warning.

109. Task Waiting

ZeroScript menyediakan operasi "task.wait()" untuk menunggu asynchronous task sampai mencapai terminal state tanpa mengambil result task tersebut.

Syntax:

await task.wait(A)

"A" harus berupa asynchronous task.

Aturan

- "task.wait(A)" menunggu task sampai mencapai salah satu terminal state:
  - "completed"
  - "failed"
  - "cancelled"
- "task.wait()" harus digunakan dengan "await".
- Jika task "completed", "task.wait()" selesai tanpa menghasilkan nilai result.
- Jika task "failed", "task.wait()" menghasilkan error task tersebut.
- Jika task "cancelled", "task.wait()" menghasilkan cancellation error.
- Jika task sudah berada pada terminal state ketika "task.wait()" dipanggil, operasi langsung selesai.
- "task.wait()" tidak menjalankan task kembali.
- "task.wait()" tidak mengubah state task.

Contoh:

async function proses() {
    await file.write("data.txt", "Hello")
}

async function main() {
    let processTask = proses()

    await task.wait(processTask)

    print("Proses selesai")
}

Berbeda dengan:

let result = await processTask

"await task.wait(processTask)" digunakan ketika programmer hanya membutuhkan informasi bahwa task telah selesai, sedangkan result task tidak diperlukan.

Secara konsep:

Task
  ↓
running
  ↓
terminal state
  ↓
task.wait()
  ↓
lanjut

"task.wait()" tidak berarti menunggu selama waktu tertentu. Operasi tersebut khusus untuk menunggu lifecycle asynchronous task.

110. Task Timeout

ZeroScript menyediakan "task.timeout()" untuk membatasi waktu tunggu terhadap asynchronous task.

Syntax:

let result = await task.timeout(A, 5000)

Parameter kedua merupakan batas waktu dalam milidetik.

Aturan

- "task.timeout(A, time)" menerima asynchronous task dan batas waktu.
- "time" harus berupa number integer yang tidak negatif.
- Batas waktu menggunakan satuan milidetik.
- Jika task selesai sebelum batas waktu, "task.timeout()" menghasilkan result task.
- Jika task belum selesai ketika batas waktu tercapai, "task.timeout()" menghasilkan ERROR.
- Timeout tidak otomatis membatalkan task yang sedang berjalan.
- Task yang mengalami timeout dapat tetap berjalan di background.
- Programmer dapat membatalkan task secara terpisah menggunakan "task.cancel()".
- Jika task sudah selesai ketika "task.timeout()" dipanggil, result langsung dikembalikan.
- Jika task gagal sebelum timeout, error task diteruskan.
- Jika task dibatalkan sebelum timeout, cancellation error diteruskan.
- "task.timeout()" tidak mengubah timeout menjadi bagian dari state task.

Contoh:

async function ambilData() {
    return await http.get("https://example.com")
}

async function main() {
    let dataTask = ambilData()

    try {
        let result = await task.timeout(dataTask, 5000)
        print(result.body)
    } catch e {
        print.err(e)
        task.cancel(dataTask)
    }
}

Secara konsep:

Task
  │
  ├── selesai sebelum 5000 ms → result
  │
  └── belum selesai setelah 5000 ms → timeout error

"task.timeout()" hanya membatasi waktu penungguan. Timeout tidak menjamin operasi asynchronous yang sedang berjalan langsung dihentikan.

111. Task Sleep

ZeroScript menyediakan "task.sleep()" untuk menunggu selama waktu tertentu tanpa menghentikan eksekusi asynchronous task lainnya.

Syntax:

await task.sleep(1000)

Parameter merupakan durasi dalam milidetik.

Aturan

- "task.sleep(time)" menerima durasi dalam milidetik.
- "time" harus berupa number integer yang tidak negatif.
- "task.sleep()" harus digunakan dengan "await".
- Selama sleep berlangsung, runtime tetap dapat menjalankan task asynchronous lainnya.
- Setelah durasi selesai, task yang melakukan sleep dapat melanjutkan eksekusinya.
- "task.sleep()" tidak menghentikan task lain.
- "task.sleep(0)" memberikan kesempatan scheduler untuk memproses task lain sebelum melanjutkan.
- "task.sleep()" tidak menggunakan blocking sleep pada thread runtime utama.
- Jika task yang sedang melakukan sleep dibatalkan, sleep dapat dihentikan sesuai kemampuan runtime.

Contoh:

async function proses() {
    print("Mulai")

    await task.sleep(1000)

    print("Selesai")
}

Beberapa task dapat menggunakan sleep secara bersamaan:

async function A() {
    await task.sleep(1000)
    print("A")
}

async function B() {
    await task.sleep(500)
    print("B")
}

async function main() {
    let taskA = A()
    let taskB = B()

    await task.all([taskA, taskB])
}

Dalam contoh tersebut, kedua task dapat menunggu secara concurrent. "task.sleep()" hanya menunda task yang memanggilnya, bukan seluruh program atau semua task.

"task.sleep()" digunakan untuk kebutuhan seperti delay, retry interval, polling, dan scheduling sederhana.

112. Task Scheduling

ZeroVM menggunakan scheduler untuk mengatur eksekusi asynchronous task.

Aturan

- Scheduler bertanggung jawab menjalankan task yang berada pada state "pending" dan "running".
- Task yang sedang menunggu "await", "task.sleep()", atau operasi asynchronous lainnya tidak memblokir task lain yang dapat dijalankan.
- Scheduler bebas menggunakan event loop, worker, thread, atau mekanisme internal lain.
- ZeroScript tidak menjamin task dijalankan pada thread tertentu.
- ZeroScript tidak menjamin urutan penyelesaian task berdasarkan urutan pembuatannya.
- Jika beberapa task siap dijalankan, scheduler menentukan urutan eksekusinya.
- Programmer tidak boleh bergantung pada urutan eksekusi asynchronous yang tidak dinyatakan secara eksplisit.
- "await" hanya melanjutkan task setelah operasi yang ditunggu mencapai kondisi yang diperlukan.
- Scheduler tidak mengubah hasil task hanya karena urutan eksekusi berbeda.
- Runtime harus menjaga state task tetap konsisten ketika beberapa task aktif secara concurrent.

Contoh:

async function A() {
    await task.sleep(1000)
    print("A")
}

async function B() {
    await task.sleep(500)
    print("B")
}

async function main() {
    let taskA = A()
    let taskB = B()

    await task.all([taskA, taskB])
}
main()

Urutan pembuatan task tidak menentukan urutan output. "taskB" dapat selesai lebih dahulu karena durasi tunggunya lebih pendek.

Scheduler merupakan bagian internal ZeroVM. Detail implementasinya tidak menjadi bagian dari bahasa ZeroScript selama aturan perilaku asynchronous yang ditentukan oleh spesifikasi tetap dipenuhi

113. Asynchronous Task Completion

Task asynchronous dianggap selesai ketika mencapai salah satu kondisi terminal: "completed", "failed", atau "cancelled".

Aturan

- Task "completed" telah menghasilkan nilai dan menyimpan hasilnya.
- Task "failed" telah menghasilkan error dan menyimpan error tersebut.
- Task "cancelled" tidak menghasilkan nilai yang dapat diambil.
- Task yang telah mencapai kondisi terminal tidak dieksekusi kembali.
- "await" pada task "completed" mengembalikan hasil task.
- "await" pada task "failed" menghasilkan kembali error task.
- "await" pada task "cancelled" menghasilkan cancellation error.
- "task.wait()" hanya menunggu task mencapai kondisi terminal.
- "task.state()" dapat digunakan untuk mengetahui kondisi task tanpa menunggunya.
- Task yang telah selesai tetap dapat direferensikan dan di-await kembali tanpa menjalankan ulang task.
- Penyelesaian satu task tidak secara otomatis mengubah atau membatalkan task lainnya.
- Urutan penyelesaian task tidak harus sama dengan urutan pembuatan task.

Dengan aturan ini, lifecycle dasar asynchronous task ZeroScript terdiri dari:

pending
   ↓
running
   ↓
completed
   ├── failed
   └── cancelled

Transisi aktual menuju "failed" atau "cancelled" merupakan terminal dan task tidak dapat kembali ke state sebelumnya.

Bagian asynchronous task dan concurrency dianggap selesai pada level bahasa. Detail scheduler, thread, event loop, dan mekanisme eksekusi internal menjadi tanggung jawab ZeroVM.
