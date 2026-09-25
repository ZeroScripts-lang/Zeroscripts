# ZeroScript — Bytecode & Instruction Set

124. ZeroVM Bytecode Validation

Sebelum menjalankan bytecode, ZeroVM harus memvalidasi struktur dasar bytecode tersebut.

Validasi dilakukan untuk memastikan bytecode dapat diproses dengan aman oleh runtime.

Validasi

ZeroVM memeriksa setidaknya:

- bytecode memiliki header atau format yang valid
- versi bytecode didukung oleh ZeroVM
- seluruh opcode dikenali
- operand setiap opcode memiliki format yang benar
- referensi constant pool valid
- target jump berada pada lokasi instruksi yang valid
- metadata fungsi memiliki struktur yang valid
- entry point program valid
- struktur bytecode tidak melewati batas data yang tersedia

Aturan

- Bytecode yang gagal validasi tidak boleh dieksekusi.
- Bytecode yang tidak memiliki versi yang didukung menghasilkan VM Error.
- Opcode yang tidak dikenal menghasilkan VM Error.
- Referensi data di luar batas bytecode menghasilkan VM Error.
- Jump ke lokasi yang tidak valid menghasilkan VM Error.
- Validasi harus dilakukan sebelum instruksi yang tidak tervalidasi dieksekusi.
- ZeroVM tidak boleh mengasumsikan bytecode selalu dibuat oleh compiler ZeroScript.
- Bytecode yang telah lolos validasi dapat dieksekusi oleh ZeroVM.
- Detail format binary bytecode dapat berkembang antar versi selama compatibility rule tetap dipatuhi.

Validasi bytecode menjadi lapisan pertama sebelum execution loop ZeroVM dimulai.

125. ZeroVM Bytecode Versioning

Bytecode ZeroScript memiliki version identifier yang menunjukkan format bytecode yang digunakan.

Version identifier digunakan oleh ZeroVM untuk menentukan apakah bytecode dapat dieksekusi.

Aturan

- Setiap bytecode executable harus memiliki informasi versi.
- ZeroVM memeriksa versi bytecode sebelum execution dimulai.
- ZeroVM wajib mendukung versi bytecode yang dinyatakan kompatibel.
- Bytecode dengan versi yang tidak didukung → VM Error.
- Versi bytecode berbeda dari versi bahasa ZeroScript.
- Perubahan format internal bytecode dapat menghasilkan versi bytecode baru.
- Compiler harus menghasilkan bytecode sesuai versi yang didukung compiler tersebut.
- ZeroVM tidak boleh mengasumsikan semua bytecode menggunakan format terbaru.
- Compatibility antar versi harus ditentukan secara eksplisit oleh ZeroVM.
- Bytecode lama tidak otomatis dianggap kompatibel hanya karena syntax ZeroScript masih kompatibel.
- Versi bytecode tidak menentukan versi package atau module.

Contoh header konseptual:

ZERO
BYTECODE_VERSION: 1

Jika ZeroVM hanya mendukung bytecode version "1", maka bytecode version "2" akan ditolak sebelum eksekusi.

Dengan versioning ini, format bytecode dapat berkembang tanpa membuat ZeroVM mengeksekusi struktur binary yang tidak dipahaminya.

126. ZeroVM Constant Pool

Bytecode ZeroScript menggunakan Constant Pool untuk menyimpan nilai konstan yang diperlukan selama eksekusi.

Constant Pool memungkinkan instruksi bytecode mereferensikan nilai melalui index daripada menyimpan seluruh nilai pada setiap instruksi.

Nilai yang Dapat Disimpan

Constant Pool dapat berisi:

- "number"
- "string"
- "boolean"
- "null"
- metadata konstan lain yang diperlukan bytecode

Object, list, function, dan closure runtime tidak disimpan sebagai nilai runtime langsung di Constant Pool kecuali diperlukan sebagai metadata atau representasi khusus bytecode.

Aturan

- Setiap entry Constant Pool memiliki index.
- Instruksi bytecode dapat mereferensikan entry menggunakan index.
- Index yang tidak valid menghasilkan VM Error.
- Constant Pool tidak dapat diubah oleh program ZeroScript selama eksekusi.
- Nilai yang diambil dari Constant Pool menjadi Runtime Value ketika digunakan oleh instruksi.
- Compiler bertanggung jawab membangun Constant Pool.
- ZeroVM bertanggung jawab memvalidasi dan membaca Constant Pool.
- Nilai yang sama dapat digunakan oleh beberapa instruksi melalui index yang sama.
- Urutan dan struktur internal Constant Pool merupakan bagian dari format bytecode, bukan syntax ZeroScript.

Contoh konseptual:

Constant Pool:
0 → 10
1 → 20
2 → "Hello"

Bytecode:
LOAD_CONST 0
LOAD_CONST 1
ADD
LOAD_CONST 2
PRINT

Constant Pool menjadi sumber nilai literal yang digunakan oleh bytecode tanpa harus menduplikasi data tersebut pada setiap instruksi.

127. ZeroVM Variable Instructions

ZeroVM menyediakan instruksi bytecode untuk membaca, membuat, dan mengubah binding variable.

Instruksi variable beroperasi terhadap Execution Frame dan lexical environment sesuai scope ZeroScript.

Instruksi Dasar

Secara konseptual, ZeroVM menyediakan:

LOAD_LOCAL
STORE_LOCAL
DECLARE_LOCAL
LOAD_GLOBAL
STORE_GLOBAL

Aturan

- "DECLARE_LOCAL" membuat binding pada scope yang sesuai.
- "LOAD_LOCAL" mengambil nilai dari local binding dan memasukkannya ke Value Stack.
- "STORE_LOCAL" mengubah nilai binding yang dapat dimutasi.
- "LOAD_GLOBAL" membaca binding global yang valid.
- "STORE_GLOBAL" hanya dapat digunakan untuk binding yang memang dapat dimutasi.
- Akses binding yang tidak ditemukan menghasilkan runtime error.
- Percobaan mengubah "const" menghasilkan runtime error.
- Instruksi bytecode tidak boleh melewati aturan scope ZeroScript.
- Compiler bertanggung jawab menentukan jenis akses binding yang diperlukan.
- ZeroVM bertanggung jawab memastikan instruksi tersebut valid ketika dieksekusi.
- Detail apakah local binding disimpan langsung pada frame, environment object, atau struktur internal lain merupakan detail implementasi.

Contoh:

let A = 10
A = 20
print(A)

Secara konseptual dapat menghasilkan:

DECLARE_LOCAL A
LOAD_CONST 10
STORE_LOCAL A

LOAD_CONST 20
STORE_LOCAL A

LOAD_LOCAL A
PRINT

Instruksi variable menjadi dasar bagi compiler untuk menerjemahkan deklarasi, pembacaan, dan assignment ZeroScript ke Bytecode

128. ZeroVM Control Flow Instructions

ZeroVM menggunakan instruksi control flow untuk mengubah urutan eksekusi bytecode.

Instruksi control flow bekerja dengan mengubah Program Counter (PC) ke lokasi instruksi tertentu.

Instruksi Dasar

Secara konseptual, ZeroVM menyediakan:

JUMP
JUMP_IF_TRUE
JUMP_IF_FALSE

"JUMP" selalu berpindah ke target tertentu.

"JUMP_IF_TRUE" dan "JUMP_IF_FALSE" mengambil nilai boolean dari Value Stack dan menentukan apakah jump dilakukan.

Aturan

- Target jump harus menunjuk ke lokasi instruksi bytecode yang valid.
- Target yang tidak valid menghasilkan VM Error.
- Conditional jump hanya menerima nilai "boolean".
- Nilai selain "boolean" tidak otomatis dianggap truthy atau falsy.
- "JUMP" tidak memerlukan operand pada Value Stack.
- Conditional jump mengonsumsi kondisi dari Value Stack.
- Compiler bertanggung jawab menghasilkan target jump yang benar.
- ZeroVM bertanggung jawab memvalidasi target sebelum digunakan.
- Control flow tidak dapat melewati batas bytecode yang tidak valid.

Contoh konseptual untuk:

if A > 10 {
    print("besar")
} else {
    print("kecil")
}

Dapat diterjemahkan menjadi:

LOAD_LOCAL A
LOAD_CONST 10
GREATER_THAN
JUMP_IF_FALSE else_block

LOAD_CONST "besar"
PRINT
JUMP end_if

else_block:
LOAD_CONST "kecil"
PRINT

end_if:

Dengan model ini, struktur seperti "if", "else", "while", "break", "continue", dan "switch" dapat diterjemahkan compiler menjadi kombinasi instruksi control flow.

ZeroVM tidak perlu memahami syntax "if" atau "while"; ZeroVM hanya menjalankan instruksi bytecode yang dihasilkan compiler.

129. ZeroVM Function Instructions

ZeroVM menyediakan instruksi bytecode untuk membuat pemanggilan fungsi, meneruskan argument, dan mengembalikan nilai.

Instruksi Dasar

Secara konseptual:

CALL
RETURN

"CALL" membuat Execution Frame baru untuk fungsi yang dipanggil.

"RETURN" mengakhiri Execution Frame aktif dan mengembalikan nilai kepada pemanggil.

Aturan

- "CALL" hanya dapat memanggil Runtime Value bertipe "function".
- Jumlah argument harus sesuai dengan aturan function ZeroScript.
- Argument diteruskan ke Execution Frame baru.
- Parameter yang tidak diberikan memperoleh "null" atau default value sesuai aturan ZeroScript.
- Extra argument menghasilkan runtime error.
- "RETURN" dapat mengembalikan Runtime Value.
- "return" tanpa nilai menghasilkan "null".
- Jika fungsi selesai tanpa "RETURN", hasilnya adalah "null".
- Setelah "RETURN", eksekusi dilanjutkan pada instruksi setelah pemanggilan fungsi.
- "CALL" membuat frame baru sehingga local variable fungsi tetap terisolasi.
- Recursive call membuat Execution Frame baru untuk setiap pemanggilan.
- Function value dapat disimpan dan diteruskan seperti nilai ZeroScript lainnya.

Contoh:

function tambah(A, B) {
    return A + B
}

let hasil = tambah(10, 20)
print(hasil)

Secara konseptual:

LOAD_FUNCTION tambah
LOAD_CONST 10
LOAD_CONST 20
CALL 2
STORE_LOCAL hasil

LOAD_LOCAL hasil
PRINT

Di dalam frame "tambah":

LOAD_LOCAL A
LOAD_LOCAL B
ADD
RETURN

Instruksi function menjadi penghubung antara Function Value, Execution Frame, Call Stack, dan Value Stack.

134. ZeroVM Literal Instructions

ZeroVM menyediakan instruksi untuk memasukkan nilai literal dari Constant Pool ke Value Stack.

Instruksi konseptual:

LOAD_CONST <index>

Aturan

- "LOAD_CONST" mengambil entry berdasarkan index dari Constant Pool.
- Nilai yang diambil dimasukkan ke bagian atas Value Stack.
- Index yang tidak valid menghasilkan VM Error.
- "LOAD_CONST" tidak mengubah isi Constant Pool.
- Nilai yang dihasilkan mengikuti tipe Runtime Value dari entry tersebut.
- Compiler bertanggung jawab menentukan index Constant Pool yang digunakan.
- ZeroVM bertanggung jawab memastikan index dan entry tersebut valid.

Contoh konseptual:

Constant Pool:
0 → 10
1 → "Hello"

Bytecode:
LOAD_CONST 0
LOAD_CONST 1

Setelah kedua instruksi dijalankan:

[10, "Hello"]

Instruksi literal menjadi dasar untuk menghasilkan nilai awal yang kemudian digunakan oleh operasi arithmetic, comparison, assignment, function call, dan instruksi lainnya.

135. ZeroVM Stack Instructions

ZeroVM menyediakan instruksi untuk mengelola nilai sementara pada Value Stack.

Instruksi dasar:

POP
DUP
SWAP

Aturan

- "POP" mengambil dan menghapus nilai teratas dari Value Stack.
- "DUP" menyalin nilai teratas dan memasukkan salinannya kembali ke stack.
- "SWAP" menukar posisi dua nilai teratas.
- "POP" pada stack kosong menghasilkan VM Error.
- "DUP" pada stack kosong menghasilkan VM Error.
- "SWAP" dengan kurang dari dua nilai menghasilkan VM Error.
- Instruksi stack tidak mengubah nilai yang berada di Heap.
- Jika nilai yang di-stack merupakan reference menuju data Heap, operasi "DUP" hanya menduplikasi reference tersebut.
- Value Stack tetap mengikuti aturan LIFO.

Contoh:

PUSH 10
PUSH 20
SWAP

Hasil:

[20, 10]

Contoh reference:

A → Object X
DUP

Stack dapat berisi:

[A → X, A → X]

Keduanya menunjuk object yang sama, bukan membuat object baru.

Instruksi stack digunakan oleh compiler sebagai building block untuk operasi bytecode yang lebih kompleks.

136. ZeroVM Arithmetic Instructions

ZeroVM menyediakan instruksi arithmetic untuk menjalankan operasi matematika ZeroScript.

Instruksi dasar:

ADD
SUB
MUL
DIV
MOD
POW

Aturan

- Setiap operasi mengambil dua operand dari Value Stack.
- Operand yang diperlukan harus tersedia pada stack.
- Operand diproses sesuai aturan tipe ZeroScript.
- Hasil operasi dimasukkan kembali ke Value Stack.
- "ADD" melakukan penjumlahan atau string concatenation sesuai aturan ZeroScript.
- "SUB", "MUL", "DIV", "MOD", dan "POW" mengikuti aturan arithmetic ZeroScript.
- Operasi dengan tipe yang tidak valid menghasilkan runtime error.
- Pembagian dengan "null" menghasilkan error karena diperlakukan sebagai pembagian dengan zero.
- Arithmetic tidak melakukan implicit type coercion.
- Stack underflow menghasilkan VM Error.
- Error arithmetic tidak boleh menghasilkan nilai pengganti seperti "null".

Contoh:

LOAD_CONST 0
LOAD_CONST 1
ADD

Jika:

Constant Pool:
0 → 10
1 → 20

maka hasil pada Value Stack:

[30]

Untuk string:

LOAD_CONST "Hello "
LOAD_CONST "World"
ADD

menghasilkan:

["Hello World"]

ZeroVM menjalankan aturan arithmetic ZeroScript; VM tidak menentukan coercion tambahan di luar spesifikasi bahasa.

137. ZeroVM Unary Instructions

ZeroVM menyediakan instruksi untuk menjalankan operasi unary ZeroScript.

Instruksi dasar:

NEG
POS
NOT

Aturan

- Setiap instruksi unary mengambil satu operand dari Value Stack.
- Operand harus tersedia pada stack.
- "NEG" menerapkan unary "-" dan hanya menerima "number".
- "POS" menerapkan unary "+" dan hanya menerima "number".
- "NOT" menerapkan logical "!" dan hanya menerima "boolean".
- Operand dengan tipe yang tidak sesuai menghasilkan runtime error.
- Hasil operasi dimasukkan kembali ke Value Stack.
- Instruksi tidak melakukan implicit type coercion.
- Stack underflow menghasilkan VM Error.

Contoh konseptual:

LOAD_CONST 10
NEG

menghasilkan:

[-10]

Sedangkan:

LOAD_CONST true
NOT

menghasilkan:

[false]

Operasi unary pada bytecode harus mempertahankan aturan tipe yang telah ditentukan oleh ZeroScript.

138. ZeroVM Comparison Instructions

ZeroVM menyediakan instruksi untuk menjalankan operasi comparison dan equality ZeroScript.

Instruksi dasar:

EQ
NEQ
RAW_EQ
GT
LT
GTE
LTE

Aturan

- Setiap instruksi mengambil dua operand dari Value Stack.
- Hasil comparison berupa "boolean".
- "EQ" mengikuti aturan "==" ZeroScript, yaitu membandingkan value dan type.
- "NEQ" merupakan kebalikan dari "EQ".
- "RAW_EQ" mengikuti aturan "===" ZeroScript, yaitu membandingkan raw value/content tanpa mempertimbangkan type.
- "GT", "LT", "GTE", dan "LTE" hanya menerima operand "number".
- Operand dengan tipe yang tidak sesuai menghasilkan runtime error.
- Comparison tidak melakukan implicit type coercion.
- Stack underflow menghasilkan VM Error.

Contoh:

LOAD_CONST 10
LOAD_CONST 10
EQ

menghasilkan:

[true]

Sedangkan:

LOAD_CONST 10
LOAD_CONST "10"
EQ

menghasilkan:

[false]

Untuk aturan "===":

LOAD_CONST 10
LOAD_CONST "10"
RAW_EQ

menghasilkan:

[true]

ZeroVM hanya menyediakan instruksi untuk menjalankan semantics equality dan comparison yang telah ditentukan ZeroScript.

139. ZeroVM Boolean Instructions

ZeroVM menyediakan instruksi untuk menjalankan operasi boolean ZeroScript.

Instruksi dasar:

AND
OR
NOT

Aturan

- "AND" mengambil dua operand "boolean" dari Value Stack.
- "OR" mengambil dua operand "boolean" dari Value Stack.
- "NOT" mengambil satu operand "boolean" dari Value Stack.
- Hasil setiap operasi berupa "boolean".
- Operand dengan tipe selain "boolean" menghasilkan runtime error.
- Tidak ada implicit truthy/falsy conversion.
- Stack underflow menghasilkan VM Error.
- Compiler dapat menggunakan conditional jump untuk melakukan short-circuit evaluation.
- "AND" dan "OR" harus mempertahankan semantics short-circuit ZeroScript.
- Operand kedua tidak boleh dievaluasi jika short-circuit menentukan hasil akhir.

Contoh:

let A = false
let B = A and sesuatu()

Compiler dapat menerjemahkannya secara konseptual menjadi:

LOAD_LOCAL A
JUMP_IF_FALSE end
CALL sesuatu
end:

Dengan demikian, "sesuatu()" tidak dipanggil ketika "A" bernilai "false".

ZeroVM dapat menggunakan kombinasi instruksi boolean dan control-flow instruction untuk menghasilkan short-circuit evaluation tanpa harus menyediakan opcode khusus untuk setiap bentuk expression.

140. ZeroVM Collection Access Instructions

ZeroVM menyediakan instruksi untuk membaca dan mengubah data pada list, string, dan object.

Instruksi konseptual:

GET_INDEX
SET_INDEX
GET_PROPERTY
SET_PROPERTY

Aturan

- "GET_INDEX" membaca nilai berdasarkan index atau key dari collection yang sesuai.
- "SET_INDEX" mengubah atau menambahkan nilai pada collection yang dapat dimutasi.
- "GET_PROPERTY" membaca property object.
- "SET_PROPERTY" mengubah atau menambahkan property object yang dapat dimutasi.
- Akses list menggunakan index integer.
- Index list di luar batas menghasilkan runtime error.
- Akses string menggunakan index integer dan menghasilkan string.
- String tidak dapat dimutasi.
- Akses object menggunakan key string.
- Key object yang tidak tersedia menghasilkan runtime error.
- "const" collection tidak dapat dimodifikasi.
- Tipe operand yang tidak sesuai menghasilkan runtime error.
- Stack underflow menghasilkan VM Error.
- Dynamic access seperti "user[key]" menggunakan Runtime Value sebagai key dan tetap mengikuti aturan tipe ZeroScript.

Contoh:

let user = {
    nama: "Kael"
}

user.nama = "Zero"

Secara konseptual:

LOAD_LOCAL user
LOAD_CONST "nama"
LOAD_CONST "Zero"
SET_PROPERTY

Untuk list:

let A = [10, 20]
A[0]

dapat diterjemahkan menjadi:

LOAD_LOCAL A
LOAD_CONST 0
GET_INDEX

Instruksi collection access menjadi lapisan bytecode yang menghubungkan operasi object, list, dan string ZeroScript dengan Runtime Value dan Heap.

141. ZeroVM Function Call Convention

ZeroVM menggunakan aturan yang konsisten untuk menempatkan function dan argument pada Value Stack sebelum instruksi "CALL" dijalankan.

Secara konseptual:

LOAD_FUNCTION
LOAD_ARGUMENT
LOAD_ARGUMENT
CALL

Aturan

- Function yang dipanggil dan seluruh argument harus tersedia pada Value Stack ketika "CALL" dieksekusi.
- "CALL" mengetahui jumlah argument yang diteruskan melalui operand bytecode.
- Argument dipindahkan ke parameter pada Execution Frame baru sesuai urutan pemanggilan.
- Argument pertama diberikan kepada parameter pertama, argument kedua kepada parameter kedua, dan seterusnya.
- Default parameter dan missing argument diproses sesuai aturan function ZeroScript.
- Extra argument menghasilkan runtime error.
- Setelah function selesai, hasil return tersedia bagi caller sebagai Runtime Value.
- "return" tanpa nilai menghasilkan "null".
- Function yang tidak memiliki "return" menghasilkan "null".
- "CALL" harus membuat Execution Frame baru sebelum function mulai dieksekusi.
- Setelah "RETURN", Execution Frame pemanggil dilanjutkan pada instruksi setelah "CALL".
- Detail layout internal Call Stack tidak menjadi bagian dari syntax ZeroScript.

Contoh:

function tambah(A, B) {
    return A + B
}

let hasil = tambah(10, 20)

Secara konseptual:

LOAD_FUNCTION tambah
LOAD_CONST 10
LOAD_CONST 20
CALL 2
STORE_LOCAL hasil

Ketika "CALL 2" dijalankan, ZeroVM membuat frame untuk "tambah" dan mengisi:

A = 10
B = 20

Setelah "RETURN", hasil "30" dikembalikan ke frame pemanggil dan dapat disimpan ke "hasil".

Calling convention ini menjadi kontrak antara compiler ZeroScript dan ZeroVM untuk pemanggilan function.

142. ZeroVM Return Convention

Instruksi "RETURN" digunakan untuk mengakhiri Execution Frame dan mengembalikan nilai kepada caller.

Secara konseptual:

LOAD_CONST 30
RETURN

Aturan

- "RETURN" mengambil nilai return dari Value Stack jika function mengembalikan nilai.
- Jika tidak ada nilai return, hasilnya adalah "null".
- "RETURN" mengakhiri Execution Frame saat ini.
- Setelah frame berakhir, execution dilanjutkan pada caller setelah instruksi "CALL".
- Nilai return ditempatkan pada Value Stack caller.
- Function yang tidak memiliki "return" eksplisit menghasilkan "null".
- Bare "return" juga menghasilkan "null".
- "RETURN" tidak dapat melewati Execution Frame lain.
- Recursive function menggunakan aturan yang sama; setiap invocation memiliki frame dan return value sendiri.
- Jika terjadi runtime error sebelum "RETURN", function tidak menghasilkan return value normal dan error diteruskan sesuai aturan error handling.

ZeroVM tidak menentukan bagaimana compiler menyusun seluruh bytecode function selama perilakunya mengikuti aturan return ZeroScript.

143. ZeroVM Function Metadata

Setiap function yang dapat dijalankan oleh ZeroVM memiliki metadata yang diperlukan untuk proses pemanggilan dan eksekusi.

Metadata dapat mencakup:

function reference
parameter count
entry point
local variable information
closure environment information

Aturan

- "CALL" menggunakan metadata function untuk membuat Execution Frame yang sesuai.
- Parameter count digunakan untuk memvalidasi jumlah argument.
- Entry point menentukan lokasi bytecode tempat function mulai dieksekusi.
- Informasi local variable digunakan oleh instruksi variable access.
- Function yang merupakan closure dapat memiliki referensi ke lexical environment.
- Metadata function bukan Runtime Value yang dapat dimodifikasi langsung oleh ZeroScript.
- Metadata yang tidak valid dapat menyebabkan VM Error saat bytecode validation atau execution.
- Recursive dan nested function menggunakan metadata masing-masing.
- Anonymous dan arrow function tetap menggunakan model metadata function yang sama.
- Detail struktur metadata merupakan bagian internal bytecode/ZeroVM dan dapat berubah selama implementasi tidak mengubah semantics ZeroScript.

Dengan metadata ini, ZeroVM dapat membedakan antara function value, Execution Frame, dan bytecode function body tanpa mengekspos detail internal tersebut kepada programmer.

144. ZeroVM Object and List Instructions

ZeroVM menyediakan instruksi untuk operasi dasar pada object dan list.

Instruksi konseptual:

MAKE_OBJECT
MAKE_LIST
LIST_ADD
LIST_REMOVE
OBJECT_REMOVE

Aturan

- "MAKE_OBJECT" membuat object baru.
- "MAKE_LIST" membuat list baru.
- Object dan list yang dibuat berada pada Heap.
- "LIST_ADD" menambahkan nilai ke akhir list yang dapat dimutasi.
- "LIST_REMOVE" menghapus elemen berdasarkan index.
- "OBJECT_REMOVE" menghapus property berdasarkan key.
- Operasi terhadap "const" collection menghasilkan runtime error.
- Index list harus berupa integer number.
- Index yang tidak valid menghasilkan runtime error.
- Key object harus berupa string.
- Property object yang tidak ditemukan saat dihapus menghasilkan runtime error.
- Instruksi yang membutuhkan collection harus menerima Runtime Value dengan tipe yang sesuai.
- Kegagalan internal Heap atau stack menghasilkan VM Error.

Instruksi ini merupakan representasi bytecode dari operasi collection ZeroScript. Implementasi internal dapat menggunakan struktur Heap yang berbeda selama semantics object dan list tetap konsisten.

145. ZeroVM Standard Operation Boundary

Operasi standard library ZeroScript dijalankan melalui runtime operation yang disediakan ZeroVM.

Standard library tidak harus menjadi bagian dari bytecode opcode dasar.

Secara konseptual, compiler dapat menghasilkan pemanggilan terhadap runtime function:

CALL_NATIVE list.length

atau menggunakan mekanisme internal lain yang setara.

Aturan

- Standard library harus mengikuti semantics yang telah ditentukan ZeroScript.
- Runtime function tidak boleh melakukan implicit coercion yang tidak diizinkan bahasa.
- Argument yang tidak sesuai menghasilkan runtime error.
- Standard library dapat membuat atau mengembalikan Runtime Value baru.
- Operasi terhadap collection tetap mengikuti aturan mutability dan reference semantics.
- Standard library dapat menggunakan fasilitas internal ZeroVM seperti Heap, I/O, filesystem, networking, dan scheduler.
- Detail mekanisme pemanggilan native operation tidak menjadi syntax ZeroScript.
- Native operation yang tidak tersedia pada runtime menghasilkan runtime error.
- Compiler dan ZeroVM boleh memiliki optimisasi khusus untuk standard library selama hasil observablenya tetap sama.

Dengan boundary ini, opcode dasar ZeroVM tetap relatif kecil, sementara fitur standard library dapat berkembang tanpa harus menambah opcode baru untuk setiap fungsi.

146. ZeroVM Native Operation Calling

ZeroVM dapat menyediakan native operation untuk fitur runtime dan standard library yang membutuhkan fasilitas di luar instruksi bytecode dasar.

Native operation dapat digunakan oleh standard library seperti operasi filesystem, networking, system information, dan fasilitas runtime lainnya.

Secara konseptual:

CALL_NATIVE <operation>

Aturan

- Native operation dipanggil melalui mekanisme runtime ZeroVM.
- Nama dan argument operation ditentukan oleh compiler dan standard library.
- Native operation menerima dan mengembalikan Runtime Value sesuai kontraknya.
- Validasi tipe argument tetap mengikuti aturan ZeroScript.
- Native operation tidak boleh melakukan implicit coercion yang bertentangan dengan semantics ZeroScript.
- Error yang berasal dari operasi normal dapat menjadi runtime error dan dapat ditangani oleh "try/catch".
- Error internal pada ZeroVM tetap menjadi VM Error.
- Native operation dapat menggunakan fasilitas sistem seperti filesystem, network, clock, atau scheduler sesuai API yang tersedia.
- Tidak semua native operation harus tersedia pada setiap runtime environment.
- Standard library tidak harus memiliki satu opcode khusus untuk setiap fungsi.

Mekanisme internal native operation merupakan detail implementasi ZeroVM. Compiler dan runtime hanya perlu mempertahankan behavior yang ditentukan oleh standard library dan ZeroScript.
