# ZeroScript — Language Specification

ZeroScript — Spesifikasi Bahasa

«ZeroScript adalah bahasa pemrograman eksperimental dengan aturan yang sederhana, eksplisit, dan sebisa mungkin tidak menebak maksud programmer.»

Status: Experimental
Source: ".zs"
Compiler: "zerosc"
Runtime / VM: "zerovm"

---

1. Filosofi

ZeroScript dirancang dengan beberapa prinsip:

- Sederhana.
- Eksplisit.
- Strict terhadap tipe.
- Tidak melakukan implicit coercion.
- Tidak menebak maksud programmer.
- Syntax familiar, tetapi aturan internal ditentukan oleh ZeroScript.

---

2. Variables

ZeroScript memiliki "let" dan "const".

"let"

Variable "let" dapat diubah.

let A = 10

A = 20

"const"

Variable "const" tidak dapat diubah.

const A = 10

A = 20 // ERROR

"const" dapat dideklarasikan tanpa value. Nilainya menjadi "null" dan tetap tidak dapat diubah.

const A

print(A) // null

A = 10 // ERROR

---

3. Scope

ZeroScript menggunakan nested lexical scope.

Lookup variable dilakukan:

current scope
    ↓
parent scope
    ↓
global scope

Scope tidak pernah mencari ke dalam child scope.

Variable dari outer scope dapat dibaca oleh inner scope.

let A = 10

if true {
    print(A)
}

Variable yang hanya dibuat di inner scope tidak dapat diakses dari outer scope.

if true {
    let A = 10
}

print(A) // ERROR

Shadowing diperbolehkan pada scope berbeda.

Deklarasi variable di bawah posisi penggunaan belum terlihat.

---

4. Function Scope Assignment

Assignment di dalam function bersifat lokal terhadap function tersebut.

let A = 10

function test() {
    A = 20
    print(A)
}

test()

print(A)

Output:

20
10

Assignment "A = 20" di dalam function tidak mengubah "A" milik outer scope.

---

5. Duplicate Declaration

Dalam scope yang sama, nama variable tidak boleh dideklarasikan dua kali.

Semua berikut adalah ERROR:

let A = 10
let A = 20

let A = 10
const A = 20

const A = 10
const A = 20

Nama yang sama diperbolehkan pada scope berbeda.

---

6. Types

Type dasar ZeroScript:

number
string
boolean
null
list
object
function

"number" mencakup integer dan decimal.

ZeroScript tidak melakukan automatic type coercion.

10 + "5" // ERROR

Sedangkan:

"10" + "5" // "105"

Amandemen — Division by Zero pada "number"

ZeroScript tidak memiliki nilai "NaN" atau "Infinity" sebagai Runtime Value.

Operasi pembagian dengan nilai "0" menghasilkan runtime error.

Contoh:

1 / 0       // ERROR
0 / 0       // ERROR
10 / null   // ERROR, karena null diperlakukan sebagai 0

Jika ZeroVM menggunakan representasi seperti IEEE-754 "f64" secara internal, nilai "NaN" atau "Infinity" yang dihasilkan oleh operasi internal tidak boleh diteruskan menjadi Runtime Value ZeroScript. Nilai tersebut harus menghasilkan runtime error.

Dengan demikian, "number" ZeroScript tetap hanya memiliki nilai numerik yang valid dan tidak mengekspos "NaN" atau "Infinity".

.2
- Operasi modulo dengan pembagi `0` juga menghasilkan runtime error.
- `null` tetap mengikuti coercion arithmetic menjadi `0`, sehingga `null ** null` dan `10 ** null` menghasilkan `1`.
- Operasi arithmetic yang secara internal menghasilkan `NaN` atau `Infinity` menghasilkan runtime error dan tidak menjadi Runtime Value ZeroScript.
---

7. null

"null" berarti variable tidak memiliki value.

"null" berbeda dari "0".

A == null // true
A == 0    // false

Dalam arithmetic, "null" diperlakukan sebagai "0".

10 + null // 10
10 - null // 10
10 * null // 0

null + 5  // 5
null * 5  // 0
null + null // 0

Division by "null" menghasilkan ERROR karena dianggap division by zero.

10 / null // ERROR

"null" merupakan value yang valid untuk function argument.

function test(A) {
    return A
}

test()       // null
test(null)   // null

---

8. Arithmetic Operators

ZeroScript menyediakan:

Operator| Fungsi
"+"| Penjumlahan
"-"| Pengurangan
"*"| Perkalian
"/"| Pembagian
"%"| Modulo
"**"| Pangkat

Contoh:

10 + 5  // 15
10 - 5  // 5
10 * 5  // 50
10 / 5  // 2
10 % 3  // 1
2 ** 3  // 8

---

9. Operator Precedence

Arithmetic mengikuti aturan matematika umum.

Urutan:

**
* / %
+ -
> < >= <=
== != ===
and / &&
or / ||

Parentheses memiliki prioritas tertinggi.

2 + 3 * 4      // 14
(2 + 3) * 4    // 20

---

10. Unary Operators

ZeroScript memiliki:

-A
+A
!A

"-" dan "+" unary hanya menerima "number".

let A = 10

-A // -10
+A // 10

"!" hanya menerima "boolean".

!true  // false
!false // true

Berikut adalah ERROR:

-"10"
!10

---

11. Increment / Decrement

ZeroScript mendukung postfix:

A++
A--

Contoh:

let A = 10

A++
A-- 

"A++" setara dengan:

A = A + 1

"A--" setara dengan:

A = A - 1

Hanya "let" dengan value "number" yang dapat digunakan.

"const" → ERROR.

"++" dan "--" menghasilkan value setelah perubahan.

let A = 10
let B = A++

print(A) // 11
print(B) // 11

Prefix "++A" dan "--A" tidak digunakan.

---

12. Compound Assignment

ZeroScript mendukung:

+=
-=
*=
/=
%=

Contoh:

let A = 10

A += 5
A -= 3
A *= 2
A /= 4
A %= 3

Compound assignment mengikuti aturan type dan mutability assignment biasa.

Contoh:

A += B

secara konsep setara dengan:

A = A + B

---

13. Assignment

Assignment menggunakan:

=

"=" bukan operator comparison.

let A = 10

A = 20

---

14. Equality

"==" membandingkan value dan type.

10 == 10      // true
10 == "10"    // false

"!=" adalah inverse dari "==".

10 != 20 // true

"===" menggunakan aturan khusus ZeroScript: membandingkan raw value/content tanpa memperhatikan type.

10 === 10     // true
10 === "10"   // true

---

15. Comparison

Operator:

>
<
>=
<=

Memiliki makna matematis dan menghasilkan "boolean".

Untuk saat ini comparison tersebut hanya berlaku pada "number".

10 > 5   // true
10 <= 5  // false

String tidak menggunakan operator mathematical comparison.

"apple" > "banana" // ERROR

---

16. Boolean

Boolean hanya memiliki:

true
false

Operator:

and / &&
or / ||
not / !

Hanya "boolean" yang dapat digunakan.

Tidak ada truthy/falsy.

true and false
true || false
!true

Contoh ERROR:

10 and true
"hello" || false

---

17. if

Syntax:

if condition {
    ...
} else if condition {
    ...
} else {
    ...
}

Condition wajib "boolean".

if A > 10 {
    print("Besar")
} else {
    print("Kecil")
}

---

18. Ternary Operator

Ternary digunakan untuk expression pendek.

let status = umur >= 17 ? "Dewasa" : "Belum"

Aturan:

- condition wajib "boolean".
- bagian setelah "?" digunakan ketika true.
- bagian setelah ":" digunakan ketika false.
- ternary menghasilkan sebuah value.

let A = 10
let B = A > 5 ? 100 : 0

"if" dan ternary dapat digunakan bersamaan sesuai kebutuhan.

---

19. while

while condition {
    ...
}

Condition wajib boolean.

while A < 10 {
    A++
}

---

20. loop

"loop" merupakan infinite/unconditional loop.

loop {
    print("Hello")
}

Runtime dapat memiliki resource/time limit pada level ZeroVM.

---

21. break

"break" menghentikan nearest active loop.

while true {
    break
}

"break" di luar loop → ERROR.

"break" tidak dapat melewati function boundary.

Pada nested loop, "break" hanya memengaruhi loop terdekat.

---

22. continue

"continue" melewati sisa iteration saat ini dan melanjutkan iteration berikutnya dari nearest loop.

while A < 10 {
    A++

    if A == 5 {
        continue
    }
}

"continue" di luar loop → ERROR.

"continue" tidak dapat melewati function boundary.

---

23. switch / case

ZeroScript mendukung "switch".

let A = 2

switch A {
    case 1 {
        print("Satu")
    }

    case 2 {
        print("Dua")
    }

    default {
        print("Lain")
    }
}

Rules:

- "switch" membandingkan value dengan setiap "case".
- Perbandingan case menggunakan "==".
- Hanya satu case yang dijalankan.
- Setelah case yang cocok selesai, "switch" selesai.
- Tidak ada fall-through.
- "default" bersifat optional.
- "default" dijalankan jika tidak ada case yang cocok.
- Tidak diperlukan "break" di dalam case.

---

24. Functions

Function menggunakan:

function nama(A, B) {
    return A + B
}

Pemanggilan:

let hasil = tambah(10, 20)

Parameter otomatis menjadi local variable dan dapat diubah.

function test(A) {
    A = 20
    return A
}

Parameter tidak membutuhkan type declaration.

Function dapat menerima value dengan type apa pun. Type error baru terjadi ketika operation yang tidak valid dilakukan.

---

25. Function Arguments

Missing argument menjadi "null".

function test(A, B) {
    return [A, B]
}

test(10)
// [10, null]

Extra argument → ERROR.

---

26. Return

Function dapat mengembalikan value:

function tambah(A, B) {
    return A + B
}

Tanpa "return", function menghasilkan "null".

function test() {
}

test() // null

Bare return juga menghasilkan "null".

return

Code setelah "return" valid tetapi tidak pernah dieksekusi.

Batas statement untuk "return": nilai "return" hanya diparsing jika ekspresinya dimulai pada baris yang sama dengan kata kunci "return". Jika token pertama setelah "return" berada di baris berikutnya, itu adalah bare return — statement berikutnya adalah statement baru dan tidak menjadi nilai "return".

"return" hanya menghentikan function tempat ia berada.

"return" di top level program (di luar semua function) mengakhiri program.

Di top level, "return" berada di frame utama program, sehingga ia menghentikan program itu sendiri. Nilai "return" top level diabaikan karena program tidak memiliki pemanggil. Code setelah "return" top level tidak dieksekusi, dan program selesai normal.

---

27. Recursive Functions

Function dapat memanggil dirinya sendiri.

function count(A) {
    if A <= 0 {
        return
    }

    print(A)
    return count(A - 1)
}

Resource/recursion limit akan ditentukan pada level runtime.

---

28. Nested Functions

Function dapat dideklarasikan di dalam function lain.

function outer() {
    function inner() {
        return 10
    }

    return inner()
}

---

29. Anonymous Functions

Function dapat digunakan sebagai value.

let tambah = function(A, B) {
    return A + B
}

tambah(10, 20)

Function dapat:

- disimpan dalam variable,
- dikirim sebagai argument,
- dikembalikan dari function,
- dipanggil sebagai value.

Memanggil value yang bukan function → ERROR.

---

30. Default Parameters

Parameter dapat memiliki default value.

function sapa(nama = "Dunia") {
    return "Halo " + nama
}

Rules:

- Missing argument menggunakan default jika tersedia.
- Missing argument tanpa default menjadi "null".
- Explicit value selalu mengoverride default.
- Default dievaluasi saat function dipanggil.
- Default dapat menggunakan outer variable dan parameter sebelumnya.
- Default tidak dapat menggunakan parameter yang dideklarasikan setelahnya.

---

31. Closure

ZeroScript menggunakan lexical closure.

Function dapat menangkap scope tempat function dibuat.

function buat() {
    let nama = "Kael"

    return function() {
        return nama
    }
}

let f = buat()

f() // "Kael"

Closure juga dapat memodifikasi captured variable.

function buatCounter() {
    let A = 0

    return function() {
        A++
        return A
    }
}

let counter = buatCounter()

counter() // 1
counter() // 2
counter() // 3

Captured variable tetap hidup selama masih dibutuhkan oleh closure.

"const" tetap immutable.

---

32. Arrow Functions

Arrow function adalah shorthand untuk anonymous function.

let tambah = (A, B) => A + B

Satu parameter:

let kali = A => A * 2

Tanpa parameter:

let halo = () => "Hello"

Block body:

let cek = (A) => {
    return A > 10
}

Expression body otomatis menjadi return value.

Arrow function memiliki behavior function biasa dan mendukung closure.

---

33. Lists

List menggunakan:

[]

Contoh:

let A = [10, 20, 30]

Index dimulai dari "0".

A[0] // 10
A[1] // 20
A[2] // 30

Out-of-bounds → ERROR.

Empty list valid:

let A = []

A[0] // ERROR

List dapat berisi berbagai type:

let A = [10, "Hello", true, null]

---

34. List Mutability

"let" list dapat dimodifikasi.

"const" list: binding-nya immutable, tetapi element mutation tetap diperbolehkan.

const A = [10, 20]

A[0] = 30
A = [40] // ERROR

Standard library:

list.add(A, 30)
list.remove(A, 1)
list.length(A)

"list.remove()" menggunakan index.

Invalid index → ERROR.

---

35. List / Object References

List dan object menggunakan shared reference semantics.

function tambah(A) {
    list.add(A, 40)
}

let X = [10, 20, 30]

tambah(X)

print(X)
// [10, 20, 30, 40]

"const" hanya membuat binding immutable; mutation melalui function terhadap isi list/object tetap diperbolehkan.

Function yang mengembalikan list/object menghasilkan value yang dapat dimodifikasi jika disimpan ke "let".

---

36. String Indexing

String dapat di-index menggunakan "[]".

let A = "Hello"

A[0] // "H"
A[1] // "e"
A[4] // "o"

Rules:

- Index dimulai dari "0".
- Index harus integer "number".
- Out-of-bounds → ERROR.
- Hasil indexing adalah "string".
- String immutable.

A[0] = "X" // ERROR

---

37. Objects

Object menggunakan "{}".

let user = {
    nama: "Kael",
    umur: 18
}

Key object selalu berupa string.

Unquoted shorthand key diperbolehkan:

nama: "Kael"

Explicit string key juga diperbolehkan:

"nama": "Kael"

Duplicate key dalam object yang sama → ERROR.

---

38. Object Access

Dot access:

user.nama

Bracket access:

user["nama"]

Dynamic key:

let key = "nama"

user[key]

Missing key → ERROR.

Key dengan value "null" tetap merupakan key yang valid.

Nested object/list dapat digunakan:

let users = [
    { nama: "Kael" },
    { nama: "Budi" }
]

users[0].nama

---

39. Object Mutation

"let" object dapat dimodifikasi.

let user = {
    nama: "Kael"
}

user.umur = 18
user["kota"] = "Bandung"

Key baru dapat ditambahkan.

"const" object: binding-nya immutable, tetapi mengubah value, menambahkan key, dan menghapus key tetap diperbolehkan.

const user = {
    nama: "Kael"
}

user.nama = "Budi"
user = {} // ERROR

Menghapus key:

object.remove(user, "umur")

Key wajib berupa string.

---

40. Function Parameters with Objects/Lists

Parameter function tidak membutuhkan type declaration.

function test(A) {
    return A.nama
}

let A = {
    nama: "Kael"
}

test(A)

List dan object menggunakan shared reference semantics.

---

41. String Comparison

String dapat dibandingkan menggunakan:

==
!=

Contoh:

"A" == "A" // true
"A" == "B" // false

Mathematical comparison terhadap string tidak diperbolehkan.

"apple" > "banana" // ERROR

---

43. Error Handling

ZeroScript menggunakan:

try {
    ...
} catch e {
    ...
} finally {
    ...
}

"catch e" menangkap runtime error.

try {
    let A = [10]
    print(A[99])
} catch e {
    print.err(e)
}

"e" merupakan local variable berbentuk object yang berisi informasi error.

e.type          // string kategori error
e.message       // string pesan error
e.location      // object "{file, line, column}" atau "null"

"e.type" menggunakan kategori error seperti "TypeError", "ReferenceError",
"RuntimeError", "SyntaxError", "IndexError", "KeyError", "DivisionError",
"ArgumentError", dan "CancellationError" sesuai jenis kegagalan.

type(e)         // "object"
string(e)       // "e.message"

print.err(e) menampilkan pesan error sehingga output tetap mudah dibaca.

"finally" selalu dijalankan.

try {
    ...
} catch e {
    ...
} finally {
    ...
}

Syntax/parse error tidak ditangkap karena terjadi sebelum program dapat dieksekusi.

Kegagalan parse saat runtime, misalnya "json.parse" pada string JSON yang rusak,
dilaporkan sebagai "SyntaxError" yang dapat ditangkap oleh "catch".

---

53. Type Introspection

ZeroScript menyediakan:

type(value)

Contoh:

type(10)        // "number"
type("Hello")   // "string"
type(true)      // "boolean"
type(null)      // "null"
type([1, 2])    // "list"
type({})        // "object"
type(test)      // "function"

"type()" selalu menghasilkan "string".

---

54. Explicit Type Conversion

ZeroScript menggunakan explicit conversion:

number("123")
string(123)
boolean(true)

Contoh:

number("123")  // 123
number("3.14") // 3.14

string(123)    // "123"
string(true)   // "true"

Invalid conversion menghasilkan ERROR.

number("hello") // ERROR

"boolean()" tidak menggunakan truthy/falsy conversion.

boolean(1)       // ERROR
boolean("true")  // ERROR

Prinsip:

«ZeroScript tidak melakukan type conversion secara otomatis.»

---

56. Current Design Philosophy

ZeroScript mengutamakan:

Explicit
Strict
Predictable
Simple
Practical

Fitur yang terlihat familiar dari bahasa lain boleh digunakan selama behavior ZeroScript tetap memiliki aturan sendiri.

Contohnya:

- "import" menggunakan gaya ESM.
- "async/await" menggunakan konsep umum asynchronous programming.
- "[]" digunakan untuk indexing.
- "switch/case" tidak memiliki fall-through.
- Type conversion harus eksplisit.
- Tidak ada truthy/falsy.
- Tidak ada automatic JSON parsing.
- Tidak ada implicit type coercion.

---
