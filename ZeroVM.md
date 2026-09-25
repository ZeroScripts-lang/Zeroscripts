# ZeroScript — ZeroVM Architecture & Runtime Model

114. ZeroVM Architecture

ZeroVM adalah runtime virtual machine yang menjalankan program ZeroScript setelah source code diproses oleh compiler.

Alur eksekusi utama:

Source Code (.zs)
      ↓
Lexer
      ↓
Parser
      ↓
AST
      ↓
Compiler
      ↓
Bytecode
      ↓
ZeroVM
      ↓
Program Execution

Aturan

- ZeroVM bertanggung jawab menjalankan bytecode ZeroScript.
- ZeroVM tidak bergantung pada Node.js untuk menjalankan program ZeroScript.
- Compiler bertanggung jawab mengubah AST menjadi bytecode yang dapat dipahami ZeroVM.
- ZeroVM tidak perlu memahami syntax source code secara langsung.
- Bytecode menjadi bentuk instruksi utama yang dieksekusi ZeroVM.
- ZeroVM menyediakan runtime environment untuk nilai, fungsi, scope, task, error, dan standard library ZeroScript.
- Implementasi internal ZeroVM dapat menggunakan bahasa pemrograman apa pun selama perilakunya sesuai spesifikasi ZeroScript.
- Detail internal seperti struktur object VM, register, stack, heap, dan scheduler tidak menjadi bagian dari syntax ZeroScript.
- Program ZeroScript harus menghasilkan perilaku yang konsisten berdasarkan aturan bahasa, terlepas dari detail implementasi internal ZeroVM.

Contoh sederhana:

let A = 10
let B = 20
print(A + B)

Compiler mengubah program tersebut menjadi bytecode, kemudian ZeroVM menjalankan bytecode tersebut dan menghasilkan output:

30

ZeroVM menjadi lapisan runtime utama antara bytecode ZeroScript dan sistem operasi.

115. ZeroVM Execution Model

ZeroVM menjalankan bytecode menggunakan execution loop yang membaca dan menjalankan instruksi secara berurutan.

Reference implementation ZeroVM menggunakan Rust.

Komponen Dasar

ZeroVM memiliki beberapa komponen utama:

- Program Counter (PC) — menunjuk instruksi bytecode yang sedang dieksekusi.
- Value Stack — menyimpan nilai sementara selama eksekusi instruksi.
- Call Stack — menyimpan informasi pemanggilan fungsi dan execution frame.
- Heap — menyimpan data runtime yang memiliki lifetime lebih panjang, seperti object, list, dan closure.
- Runtime State — menyimpan informasi global runtime, task, module, dan resource yang sedang digunakan.

Execution Loop

Secara konseptual, ZeroVM menjalankan proses:

fetch instruction
      ↓
decode instruction
      ↓
execute instruction
      ↓
update runtime state
      ↓
next instruction

Program Counter berpindah ke instruksi berikutnya setelah instruksi berhasil dijalankan, kecuali instruksi tersebut mengubah alur eksekusi seperti jump, function call, return, atau error.

Contoh konseptual bytecode:

LOAD_CONST 10
LOAD_CONST 20
ADD
PRINT
HALT

ZeroVM mengeksekusi instruksi tersebut secara berurutan:

10
20
↓
ADD
↓
30
↓
PRINT
↓
HALT

Aturan

- ZeroVM hanya mengeksekusi instruksi bytecode yang valid.
- Instruksi yang tidak dikenal → VM Error.
- Akses stack yang tidak valid → VM Error.
- Program Counter yang keluar dari bytecode yang valid → VM Error.
- Error runtime harus menghentikan execution frame yang terdampak dan mengikuti aturan error handling ZeroScript.
- "HALT" mengakhiri eksekusi program secara normal.
- Detail struktur internal stack dan heap merupakan implementasi ZeroVM dan dapat berubah tanpa mengubah syntax ZeroScript.
- Compiler dan ZeroVM harus menggunakan spesifikasi bytecode yang kompatibel.

Execution model ini menjadi dasar untuk pembahasan bytecode instruction set pada bagian berikutnya.

116. ZeroVM Value Stack

ZeroVM menggunakan Value Stack untuk menyimpan nilai sementara selama eksekusi bytecode.

Value Stack bersifat last-in, first-out (LIFO).

Operasi Dasar

Bytecode dapat melakukan operasi:

- "PUSH" — memasukkan nilai ke stack.
- "POP" — mengambil dan menghapus nilai teratas.
- "DUP" — menduplikasi nilai teratas.
- "SWAP" — menukar dua nilai teratas.

Contoh:

PUSH 10
PUSH 20
ADD

Keadaan stack:

PUSH 10

[10]

PUSH 20

[10, 20]

ADD

[30]

Instruksi "ADD" mengambil dua nilai teratas, melakukan operasi sesuai aturan tipe ZeroScript, kemudian memasukkan hasilnya kembali ke stack.

Aturan

- Nilai pada Value Stack merupakan nilai runtime ZeroScript.
- Operasi aritmatika mengambil operand dari Value Stack.
- Operand yang tidak mencukupi → VM Error.
- Operasi dengan tipe yang tidak valid mengikuti aturan type checking ZeroScript.
- Value Stack bersifat lokal terhadap execution frame.
- Stack tidak dapat diakses langsung oleh program ZeroScript.
- Compiler bertanggung jawab menghasilkan urutan instruksi yang menjaga stack tetap valid.
- ZeroVM harus mendeteksi stack underflow.
- Batas maksimum stack dapat ditentukan oleh implementasi runtime.
- Melebihi batas stack → VM Error.

Value Stack digunakan terutama untuk operasi sementara. Data yang harus bertahan melewati pemanggilan fungsi atau memiliki lifetime runtime yang lebih panjang dikelola melalui execution frame dan heap.

117. ZeroVM Call Stack & Execution Frame

ZeroVM menggunakan Call Stack untuk mengelola pemanggilan fungsi. Setiap pemanggilan fungsi membuat sebuah Execution Frame baru.

Execution Frame menyimpan konteks eksekusi fungsi yang sedang berjalan.

Execution Frame

Sebuah Execution Frame dapat berisi:

- Program Counter untuk fungsi tersebut.
- Referensi ke bytecode fungsi.
- Local variables.
- Parameter fungsi.
- Referensi ke lexical environment yang diperlukan oleh closure.
- Informasi return value.
- Informasi runtime lain yang diperlukan untuk melanjutkan eksekusi.

Call Stack

Call Stack menyimpan Execution Frame secara berurutan.

Contoh:

main()
  ↓
foo()
  ↓
bar()

Call Stack:

┌─────────────┐
│ bar frame   │ ← aktif
├─────────────┤
│ foo frame   │
├─────────────┤
│ main frame  │
└─────────────┘

Ketika "bar()" melakukan "return", frame "bar" dihapus dan eksekusi dilanjutkan pada frame "foo".

Aturan

- Setiap pemanggilan fungsi membuat Execution Frame baru.
- Parameter fungsi disimpan sebagai local binding pada frame tersebut.
- Local variable fungsi tidak dapat diakses langsung oleh frame lain.
- "return" mengakhiri frame fungsi yang sedang aktif.
- Nilai "return" dikembalikan ke pemanggil.
- Fungsi tanpa "return" menghasilkan "null".
- Rekursi membuat frame baru untuk setiap pemanggilan.
- Frame yang sudah selesai tidak dapat digunakan kembali untuk melanjutkan fungsi tersebut.
- Closure dapat mempertahankan lexical environment yang dibutuhkan setelah frame pembuatnya selesai.
- Pemanggilan fungsi non-function menghasilkan runtime error.
- Call Stack memiliki batas implementasi untuk mencegah penggunaan stack yang tidak terbatas.
- Melebihi batas Call Stack menghasilkan VM Error.

Contoh rekursi:

function hitung(A) {
    if A <= 0 {
        return 0
    }

    return A + hitung(A - 1)
}

print(hitung(3))

Secara konseptual:

hitung(3)
  → hitung(2)
      → hitung(1)
          → hitung(0)
          ← return
      ← return
  ← return

Call Stack memastikan setiap pemanggilan memiliki konteks lokalnya sendiri dan dapat kembali ke titik pemanggilan yang benar.

118. ZeroVM Heap

ZeroVM menggunakan Heap untuk menyimpan data runtime yang membutuhkan lifetime lebih panjang atau tidak cocok disimpan langsung sebagai nilai sederhana pada Value Stack.

Heap digunakan terutama untuk:

- list
- object
- function value
- closure
- string dengan representasi runtime yang membutuhkan storage khusus
- data runtime lain yang memiliki lifetime dinamis

Aturan

- Setiap nilai yang membutuhkan storage heap memiliki referensi runtime yang dikelola oleh ZeroVM.
- Value Stack dapat menyimpan referensi menuju data yang berada di Heap.
- Object dan list yang diberikan sebagai argument fungsi mempertahankan shared reference semantics sesuai aturan ZeroScript.
- Dua variable dapat merujuk pada object atau list yang sama.
- Memodifikasi collection melalui salah satu reference dapat terlihat melalui reference lainnya.
- "const" mengontrol mutability binding dan collection sesuai aturan ZeroScript, bukan lokasi penyimpanan pada Heap.
- Heap tidak dapat diakses secara langsung oleh program ZeroScript.
- ZeroVM bertanggung jawab menjaga object yang masih digunakan agar tetap tersedia.
- Data yang sudah tidak dapat digunakan oleh program dapat diklaim kembali oleh mekanisme memory management ZeroVM.
- Implementasi Heap tidak menentukan syntax atau semantics ZeroScript.

Contoh:

let A = [10, 20]
let B = A

list.add(B, 30)

print(A)

"A" dan "B" merujuk pada list yang sama sehingga perubahan melalui "B" terlihat ketika "A" digunakan.

Heap menjadi dasar untuk pembahasan memory management dan garbage collection pada bagian berikutnya.

119. ZeroVM Runtime Values

ZeroVM merepresentasikan seluruh nilai ZeroScript menggunakan Runtime Value.

Runtime Value menjadi representasi internal untuk nilai yang digunakan selama eksekusi program.

Jenis utama Runtime Value mengikuti tipe ZeroScript:

number
string
boolean
null
list
object
function

Aturan

- "number", "boolean", dan "null" dapat direpresentasikan sebagai nilai langsung.
- "string", "list", "object", dan "function" dapat menggunakan data yang dikelola oleh Heap.
- Runtime Value dapat disimpan pada Value Stack, Execution Frame, atau struktur runtime lainnya.
- Reference menuju data Heap tidak dianggap sebagai tipe ZeroScript tambahan.
- Program ZeroScript tidak dapat membedakan apakah suatu nilai disimpan langsung atau melalui Heap.
- Operasi terhadap Runtime Value harus mengikuti aturan tipe ZeroScript.
- Reference yang menunjuk data yang sama mempertahankan shared reference semantics.
- Runtime harus membedakan nilai "null" dari reference yang valid.
- Reference yang sudah tidak valid tidak boleh digunakan oleh program.
- Implementasi internal representasi Runtime Value dapat berubah tanpa mengubah semantics ZeroScript.

Contoh:

let A = {
    nama: "Kael"
}

let B = A

B.nama = "Zero"
print(A.nama)

"A" dan "B" memiliki reference menuju object yang sama. Karena itu perubahan melalui "B" dapat terlihat melalui "A".

Runtime Value menjadi lapisan penghubung antara bytecode, Value Stack, Execution Frame, dan Heap.

120. ZeroVM Memory Management

ZeroVM menggunakan automatic memory management untuk mengelola data runtime yang berada pada Heap.

Program ZeroScript tidak mengelola alokasi dan pelepasan memory Heap secara manual.

Aturan

- ZeroVM bertanggung jawab mengalokasikan storage Heap ketika diperlukan.
- ZeroVM bertanggung jawab melepaskan storage Heap yang sudah tidak dapat digunakan.
- Programmer tidak perlu melakukan "free", "delete", atau operasi memory manual lainnya.
- Reference yang masih dapat dijangkau oleh program harus tetap valid.
- Data yang sudah tidak dapat dijangkau dapat direklamasi oleh runtime.
- Memory management tidak mengubah semantics nilai ZeroScript.
- Runtime dapat menjalankan garbage collection ketika diperlukan.
- Garbage collection dapat berjalan secara otomatis tanpa instruksi khusus dari programmer.
- Waktu eksekusi garbage collection tidak menjadi bagian dari semantics ZeroScript.
- Kegagalan alokasi memory menghasilkan runtime/VM error.
- ZeroVM harus mencegah penggunaan kembali memory yang masih direferensikan oleh program.

Contoh:

function buatData() {
    let data = {
        nama: "ZeroScript"
    }

    return data
}

let A = buatData()

Object yang dibuat oleh "buatData()" tetap tersedia setelah fungsi selesai karena masih direferensikan oleh "A".

Sebaliknya, ketika suatu object tidak lagi dapat dijangkau oleh program, object tersebut tidak lagi diperlukan dan dapat direklamasi oleh garbage collector.

Detail algoritma garbage collection, seperti tracing, generational collection, atau strategi lainnya, merupakan detail implementasi ZeroVM dan dapat berubah tanpa mengubah bahasa ZeroScript.

121. ZeroVM Garbage Collection

ZeroVM menggunakan garbage collector (GC) untuk menemukan dan mereklamasi data Heap yang sudah tidak dapat dijangkau oleh program.

Garbage collection dilakukan secara otomatis oleh runtime.

Root Reference

GC menganggap beberapa sumber runtime sebagai root, termasuk:

- local variables pada Execution Frame yang aktif
- parameter fungsi
- nilai pada Value Stack
- global bindings
- module bindings yang masih aktif
- task yang masih berjalan
- reference yang sedang digunakan oleh runtime

GC menelusuri reference dari root tersebut untuk menentukan object yang masih dapat dijangkau.

Aturan

- Object yang masih dapat dijangkau tidak boleh direklamasi.
- Object yang tidak dapat dijangkau dapat direklamasi.
- Circular reference diperbolehkan.
- Circular reference yang tidak lagi dapat dijangkau tetap dapat direklamasi.
- Programmer tidak perlu mengetahui kapan GC dijalankan.
- GC tidak mengubah nilai atau semantics program.
- GC tidak boleh menghapus data yang masih digunakan oleh program.
- Runtime dapat menjalankan GC secara berkala atau ketika penggunaan memory mencapai kondisi tertentu.
- Algoritma GC merupakan detail implementasi ZeroVM.
- ZeroVM dapat mengganti algoritma GC tanpa mengubah compatibility bahasa.

Contoh circular reference:

let A = {}
let B = {}

A.other = B
B.other = A

Jika "A" dan "B" sudah tidak dapat dijangkau dari root runtime mana pun, keduanya dapat direklamasi meskipun masih memiliki reference satu sama lain.

Dengan demikian, ZeroVM tidak bergantung pada reference counting sederhana sebagai satu-satunya mekanisme memory management.

122. ZeroVM Memory Safety

ZeroVM harus menjaga agar program ZeroScript tidak dapat mengakses memory runtime secara tidak valid.

Memory safety merupakan tanggung jawab ZeroVM dan tidak memerlukan pengelolaan memory manual dari programmer.

Aturan

- Program ZeroScript tidak dapat mengakses alamat memory mentah.
- Program tidak dapat melakukan pointer arithmetic.
- Program tidak dapat membaca atau menulis memory di luar object yang direferensikan.
- Akses list dengan index di luar batas menghasilkan runtime error.
- Akses object dengan key yang tidak tersedia menghasilkan runtime error.
- Reference yang tidak lagi valid tidak boleh digunakan.
- Garbage collector tidak boleh mereklamasi object yang masih reachable.
- Perubahan pada object atau list hanya dapat dilakukan melalui operasi yang disediakan ZeroScript.
- Memory error internal pada ZeroVM harus menghasilkan VM Error dan tidak boleh menyebabkan akses memory yang tidak valid diteruskan ke program.
- Reference antar object dapat membentuk cycle tanpa menyebabkan akses memory yang tidak valid.
- Detail implementasi memory safety mengikuti implementasi ZeroVM dalam Rust dan runtime internals lainnya.

ZeroScript tidak menyediakan primitive memory seperti raw pointer, manual allocation, "free", atau akses alamat memory.

Dengan aturan ini, memory management dan memory safety sepenuhnya berada di bawah kendali ZeroVM.

123. ZeroVM Error Boundary

ZeroVM membedakan antara runtime error ZeroScript dan VM Error.

Runtime error berasal dari operasi program yang melanggar aturan ZeroScript, sedangkan VM Error menunjukkan kondisi internal atau instruksi bytecode yang tidak dapat diproses dengan benar.

Runtime Error

Contoh runtime error:

let A = [10]
print(A[5])

Akses index yang berada di luar batas menghasilkan runtime error yang dapat ditangani oleh "try/catch".

VM Error

VM Error dapat terjadi karena kondisi seperti:

- bytecode tidak valid
- opcode tidak dikenal
- stack corruption
- stack underflow
- Program Counter tidak valid
- format runtime internal tidak konsisten
- kegagalan memory allocation
- kondisi internal VM yang tidak dapat dipulihkan

Aturan

- Runtime error ZeroScript mengikuti sistem error handling ZeroScript.
- Runtime error dapat ditangkap oleh "try/catch" jika terjadi pada konteks yang dapat ditangkap.
- VM Error tidak dianggap sebagai error program biasa.
- VM Error tidak boleh diam-diam diubah menjadi "null" atau nilai lain.
- VM Error harus menghentikan eksekusi yang terdampak.
- Kondisi internal yang tidak dapat dipulihkan dapat menghentikan seluruh program.
- ZeroVM harus mencegah kondisi internal VM menyebabkan akses memory yang tidak valid.
- Pesan VM Error harus memberikan informasi yang cukup untuk debugging ketika memungkinkan.
- Detail internal seperti alamat memory, struktur Rust, atau implementasi GC tidak menjadi bagian dari semantics ZeroScript.

Dengan boundary ini, kesalahan programmer pada program ZeroScript dapat dibedakan dari kerusakan atau kondisi tidak valid pada runtime ZeroVM.

130. ZeroVM Closure Environment

ZeroVM menyediakan mekanisme runtime untuk mempertahankan lexical environment yang digunakan oleh closure.

Closure menyimpan function value bersama environment tempat fungsi tersebut dibuat.

Aturan

- Setiap closure mempertahankan akses terhadap binding yang diperlukan dari lexical scope tempat closure dibuat.
- Closure dapat membaca captured variable.
- Closure dapat mengubah captured "let" variable.
- Captured "const" tetap immutable.
- Environment yang masih digunakan oleh closure harus tetap tersedia pada Heap.
- Execution Frame pembuat closure dapat selesai tanpa menghapus environment yang masih digunakan.
- Beberapa closure dapat berbagi environment yang sama.
- Perubahan captured variable dapat terlihat oleh closure lain yang berbagi environment tersebut.
- Closure tidak dapat mengakses variable yang berada di scope yang tidak termasuk lexical environment-nya.
- Runtime harus mempertahankan aturan scope ZeroScript ketika melakukan variable lookup.

Contoh:

function buatCounter() {
    let A = 0

    return function() {
        A++
        return A
    }
}

let counter = buatCounter()

print(counter())
print(counter())

Secara konseptual:

buatCounter()
    │
    ├── Environment
    │     A = 0
    │
    └── Closure ──────┐
                      ↓
                 Environment
                      A

Setelah "buatCounter()" selesai, Execution Frame fungsi tersebut dapat dihapus. Namun environment yang berisi "A" tetap hidup karena masih direferensikan oleh closure.

Pemanggilan "counter()" menggunakan environment tersebut sehingga nilai "A" tetap dipertahankan antar pemanggilan.

Detail apakah environment direpresentasikan sebagai object, heap structure, atau mekanisme internal lainnya merupakan detail implementasi ZeroVM.

131. ZeroVM Reference Tracking

ZeroVM harus dapat menentukan apakah suatu data Heap masih digunakan oleh program.

Runtime menggunakan mekanisme reference tracking untuk membantu garbage collector menentukan object yang masih dapat dijangkau.

Reference Sources

Reference dapat berasal dari:

- Value Stack
- local variables
- function parameters
- global bindings
- module bindings
- active Execution Frame
- closure environment
- active asynchronous task
- object atau list lain yang masih reachable

Aturan

- Reference yang masih dapat dijangkau dari runtime root harus dipertahankan.
- Reference yang sudah tidak dapat dijangkau dapat direklamasi oleh garbage collector.
- Object dapat memiliki reference menuju object lain.
- List dapat berisi reference menuju object atau list lain.
- Circular reference diperbolehkan.
- Circular reference yang sudah tidak dapat dijangkau dari root runtime wajib dapat direklamasi.
- Runtime tidak boleh mempertahankan data hanya karena data tersebut memiliki reference dari object lain yang juga sudah unreachable.
- Reference tracking tidak mengubah shared reference semantics ZeroScript.
- Program ZeroScript tidak dapat mengakses mekanisme reference tracking secara langsung.
- Runtime harus memperbarui reference yang relevan ketika binding, frame, closure, atau task dibuat dan dihentikan.
- Implementasi reference tracking dan garbage collection merupakan detail internal ZeroVM.
- ZeroVM dapat menggunakan tracing GC, generational GC, hybrid mechanism, atau mekanisme internal lainnya selama seluruh aturan memory management pada spesifikasi tetap dipenuhi.

Contoh circular reference yang menjadi unreachable:

function buatCycle() {
    let A = {}
    let B = {}

    A.other = B
    B.other = A
}

buatCycle()

Setelah "buatCycle()" selesai:

A → B
↑   ↓
└───┘

"A" dan "B" masih saling mereferensikan, tetapi tidak ada lagi reference dari root runtime menuju keduanya.

Karena itu, "A" dan "B" dianggap unreachable dan harus dapat direklamasi oleh garbage collector.

Reference tracking menjadi dasar bagi garbage collector untuk mempertahankan object yang masih digunakan dan melepaskan object yang sudah tidak diperlukan.

132. ZeroVM Heap Allocation

ZeroVM mengalokasikan data runtime ke Heap ketika data tersebut membutuhkan penyimpanan dengan lifetime dinamis.

Heap allocation dikelola sepenuhnya oleh runtime.

Allocation

Data yang dapat membutuhkan Heap allocation meliputi:

- object
- list
- closure environment
- function value
- string atau representasi string tertentu
- struktur internal runtime yang memiliki lifetime dinamis

Aturan

- Program ZeroScript tidak melakukan Heap allocation secara manual.
- Allocation dilakukan ketika runtime membutuhkan storage untuk suatu Runtime Value.
- Object dan list yang dibuat oleh program memiliki lifetime yang mengikuti reachability-nya.
- Closure environment harus tetap tersedia selama masih digunakan oleh closure.
- Data Heap yang masih reachable tidak boleh direklamasi.
- Data Heap yang sudah unreachable dapat direklamasi.
- Runtime dapat menggunakan allocation strategy apa pun selama semantics ZeroScript tetap dipenuhi.
- Kegagalan allocation menghasilkan VM Error atau runtime error yang sesuai.
- Allocation tidak menjamin setiap Runtime Value selalu memiliki object Heap tersendiri.
- Runtime dapat mengoptimalkan representasi internal selama perubahan tersebut tidak terlihat oleh program ZeroScript.

Contoh:

function buatUser() {
    return {
        nama: "Kael"
    }
}

let user = buatUser()

Object yang dibuat oleh "buatUser()" harus tetap tersedia selama "user" masih mereferensikannya.

Jika binding "user" kemudian tidak lagi menjadi bagian dari root runtime dan tidak ada reference lain menuju object tersebut, object dapat direklamasi.

Heap allocation merupakan mekanisme internal ZeroVM dan tidak menjadi bagian dari syntax atau API memory manual ZeroScript.

133. ZeroVM Memory Limits

ZeroVM dapat menerapkan batas penggunaan memory untuk menjaga kestabilan runtime.

Batas memory merupakan runtime constraint, bukan bagian dari semantics nilai ZeroScript.

Aturan

- ZeroVM dapat memiliki batas maksimum Heap memory.
- Batas dapat ditentukan oleh runtime, konfigurasi, atau environment tempat program dijalankan.
- Program yang membutuhkan memory melebihi batas yang tersedia menghasilkan VM Error atau memory error.
- Garbage collection dapat dijalankan sebelum runtime menyatakan allocation gagal.
- Memory limit tidak mengubah nilai atau reference semantics ZeroScript.
- Tidak ada batas memory tetap yang diwajibkan oleh bahasa ZeroScript.
- Runtime dapat menyediakan konfigurasi untuk mengatur memory limit.
- Jika tidak ada limit eksplisit, runtime dapat menggunakan batas yang ditentukan oleh environment.
- Memory yang sudah direklamasi dapat digunakan kembali oleh runtime.
- Detail strategi allocation dan reclamation tetap menjadi tanggung jawab ZeroVM.

Tujuan memory limit adalah mencegah satu program menggunakan resource secara tidak terkendali, tanpa menjadikan batas hardware tertentu sebagai bagian dari bahasa ZeroScript.
