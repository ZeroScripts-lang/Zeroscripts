Job 003 — Compiler & AST → Bytecode

Objective

Implement ZeroScript Compiler yang mengubah AST hasil parser menjadi bytecode ZeroScript sesuai spesifikasi yang sudah ada.

Pipeline yang harus diwujudkan:

Source
  ↓
Lexer
  ↓
Tokens
  ↓
Parser
  ↓
AST
  ↓
Compiler
  ↓
Bytecode

Compiler harus menggunakan AST yang sudah dibuat pada Job 002 dan menghasilkan representasi bytecode yang sesuai dengan "Bytecode.md".

Spec adalah source of truth. Jangan membuat syntax, semantics, opcode, atau behavior baru yang tidak didefinisikan oleh spec.

---

Required Reading

Sebelum implementasi:

1. "AGENTS.md"
2. "INDEX.md"
3. "Language.md"
4. "Bytecode.md"
5. "ZeroVM.md" — hanya bagian yang mendefinisikan format/instruksi yang diperlukan compiler
6. "ISSUES.md"
7. "src/ast.rs"
8. "src/parser.rs"
9. "src/token.rs"
10. "src/error.rs"
11. "src/main.rs"

Baca implementasi yang sudah ada sebelum mengubah struktur AST atau parser.

---

Scope

Job ini hanya mencakup:

- Compiler
- Bytecode representation yang dibutuhkan compiler
- Constant Pool yang dibutuhkan compiler
- AST → bytecode
- Label/jump target resolution
- Compiler error
- Unit tests compiler
- Integrasi compiler dengan AST

Out of Scope

Jangan mengimplementasikan:

- ZeroVM execution loop
- Runtime evaluation
- Heap
- Garbage collector
- Task scheduler
- Async runtime
- Module loader
- Dependency manager
- Standard library
- Native functions
- Runtime I/O
- Optimization lanjutan
- JIT
- Machine code generation

Compiler hanya menghasilkan bytecode.

---

Compiler Architecture

Buat struktur compiler yang jelas dan terpisah dari parser.

Contoh struktur yang dapat digunakan:

src/
├── ast.rs
├── compiler.rs
├── error.rs
├── lexer.rs
├── main.rs
├── parser.rs
└── token.rs

Nama dan struktur internal boleh disesuaikan selama tetap konsisten dengan project dan spec.

Compiler menerima AST:

Program

dan menghasilkan bytecode program.

Konsep umum:

AST
 ↓
Compiler
 ├── Constant Pool
 ├── Instructions
 ├── Jump Patching
 └── Metadata
 ↓
Bytecode

---

Constant Pool

Implementasi compiler harus menyediakan mekanisme untuk memasukkan constant yang diperlukan bytecode ke Constant Pool.

Constant Pool mengikuti aturan pada "Bytecode.md", termasuk:

- setiap entry memiliki index
- instruction dapat mereferensikan constant berdasarkan index
- duplicate constant dapat menggunakan index yang sama jika spec mengizinkan
- invalid index harus menjadi tanggung jawab validasi bytecode/VM, bukan compiler runtime
- Constant Pool immutable setelah bytecode selesai dibangun

Minimal dukung constant yang memang digunakan oleh AST dan didefinisikan oleh spec:

- number
- string
- boolean
- null

Jangan memasukkan runtime object/function/closure sebagai runtime value Constant Pool kecuali "Bytecode.md" secara eksplisit mendefinisikannya demikian.

---

Instruction Generation

Compiler harus menerjemahkan AST menjadi opcode yang didefinisikan dalam "Bytecode.md".

Jangan membuat opcode baru hanya karena opcode tersebut terasa dibutuhkan.

Compiler harus menangani seluruh construct yang memang sudah didukung parser dan memiliki representasi bytecode dalam spec.

Minimal mencakup construct yang relevan:

Literals

number
string
boolean
null

Variable

let
const
identifier read
assignment
compound assignment

Expressions

unary
binary
comparison
equality
boolean
postfix ++
postfix --
function call
member access
index access

Collections

list
object
list indexing
object property access

Functions

function declaration
anonymous function
arrow function
function call
return
closure/captured variable

Control Flow

if
else
ternary
loop
while
break
continue
switch
case

Error Handling

Jika bytecode spec menyediakan instruction/representation untuk:

try
catch
finally

compiler harus menghasilkan representasi yang sesuai.

Jangan mengimplementasikan runtime error handling di compiler.

---

Expression Compilation

Setiap expression harus menghasilkan bytecode dengan stack behavior yang konsisten dengan VM specification.

Contoh konseptual:

number literal
    ↓
LOAD_CONST <index>

Binary expression:

A + B

secara konseptual:

compile A
compile B
ADD

Gunakan opcode aktual dari "Bytecode.md", bukan nama konseptual di atas jika berbeda.

Operator precedence tidak perlu dihitung ulang oleh compiler karena sudah direpresentasikan oleh struktur AST.

Compiler harus mengikuti struktur AST.

---

Variable Compilation

Compiler harus menghormati semantics variable dari "Language.md".

Perhatikan:

- "let" mutable
- "const" immutable
- scope lexical
- shadowing
- declaration visibility
- assignment rules
- local/global binding sesuai model bytecode
- closure capture sesuai spec

Jangan mengganti semantics variable hanya karena implementasi compiler lebih mudah.

---

Control Flow

Compiler harus menangani jump instruction melalui mekanisme label/patching jika diperlukan.

Contoh konseptual:

if condition {
    A
} else {
    B
}

menjadi konsep:

compile condition
JUMP_IF_FALSE <else>
compile A
JUMP <end>
<else>:
compile B
<end>:

Gunakan opcode aktual yang didefinisikan pada "Bytecode.md".

Jump target tidak boleh dibiarkan sebagai placeholder pada final bytecode.

---

Loop

Untuk:

while condition {
    body
}

compiler harus menghasilkan struktur jump yang memungkinkan:

condition
 ↓
conditional jump
 ↓
body
 ↓
jump back

"break" dan "continue" harus menggunakan target loop yang benar.

Nested loop harus memiliki target masing-masing sehingga:

break

hanya keluar dari loop yang sesuai dengan semantics ZeroScript.

---

Switch

Compiler harus mengikuti semantics "switch/case" pada "Language.md" dan representation bytecode pada "Bytecode.md".

Tidak boleh menambahkan fallthrough jika language specification menyatakan case tidak fallthrough.

---

Function Compilation

Compiler harus menangani:

- parameter
- local scope
- function body
- return
- nested function
- anonymous function
- arrow function
- recursion
- closure

Function value harus dikompilasi menggunakan representation yang didefinisikan oleh bytecode specification.

Closure capture tidak boleh diimplementasikan dengan global variable sebagai shortcut jika spec mendefinisikan lexical capture.

---

Compiler Errors

Tambahkan compiler error type jika diperlukan.

Compiler error minimal harus dapat membedakan masalah seperti:

- invalid AST state
- unresolved binding
- invalid compilation state
- invalid control-flow context
- unsupported AST construct
- invalid bytecode construction state

Error harus membawa informasi yang berguna untuk debugging.

Jangan mengubah syntax error menjadi compiler error.

---

Bytecode Validation

Compiler harus menghasilkan bytecode yang structurally valid menurut "Bytecode.md".

Minimal pastikan:

- constant index valid
- jump target valid setelah patching
- instruction operands valid
- function metadata konsisten
- bytecode tidak meninggalkan unresolved placeholder
- stack behavior tidak menghasilkan struktur yang jelas invalid menurut format bytecode

Runtime semantic validation tetap menjadi tanggung jawab ZeroVM.

---

Tests

Tambahkan unit tests untuk compiler.

Minimal test:

Basic

- empty program
- number literal
- string literal
- boolean literal
- null literal

Variables

- let declaration
- const declaration
- identifier access
- assignment
- compound assignment
- shadowing

Expressions

- unary
- arithmetic
- comparison
- equality
- boolean
- postfix
- function call
- member access
- index access

Collections

- list
- object
- indexing
- property access

Functions

- function declaration
- parameters
- return
- anonymous function
- arrow function
- recursion
- nested function
- closure

Control Flow

- if
- if/else
- ternary
- while
- loop
- break
- continue
- nested loops
- switch/case

Errors

- invalid compiler state
- invalid control-flow context
- unresolved binding if applicable
- unsupported/invalid AST state

Constant Pool

Test bahwa constants yang digunakan compiler masuk ke Constant Pool dengan index yang benar.

Jika deduplication constant merupakan bagian dari implementation design/spec, test bahwa constant yang sama tidak dibuat berulang.

---

Existing Tests

Semua test dari Job 001 dan Job 002 harus tetap lulus.

Jalankan:

cargo test

Pastikan:

- lexer tests pass
- parser tests pass
- compiler tests pass
- tidak ada warning Rust yang baru
- tidak ada panic dari test normal
- tidak ada TODO implementasi yang ditinggalkan dalam scope job

---

Important Constraints

1. Jangan mengubah semantics ZeroScript.
2. Jangan mengarang opcode.
3. Jangan mengimplementasikan ZeroVM pada job ini.
4. Jangan memindahkan tanggung jawab runtime ke compiler.
5. Jangan melakukan optimasi yang mengubah semantics.
6. Jangan menghapus parser/lexer test hanya supaya build pass.
7. Jangan mengubah AST secara besar-besaran tanpa alasan yang berasal dari spec.
8. Jika AST dari Job 002 ternyata tidak cukup untuk merepresentasikan semantics yang diwajibkan spec, evaluasi masalahnya terlebih dahulu dan lakukan perubahan minimal yang diperlukan.
9. Jika spec ambigu, baca "ISSUES.md" dan jangan menebak.
10. Jangan mengubah file spec kecuali job ini secara eksplisit membutuhkan koreksi spec.

---

Completion Criteria

Job dianggap selesai jika:

- [ ] "src/compiler.rs" atau struktur compiler equivalent dibuat
- [ ] AST dapat dikompilasi menjadi bytecode
- [ ] Constant Pool bekerja
- [ ] instruction generation bekerja sesuai "Bytecode.md"
- [ ] jump/branch patching bekerja
- [ ] function compilation bekerja sesuai representation bytecode
- [ ] closure representation mengikuti spec
- [ ] compiler errors tersedia
- [ ] compiler tests ditambahkan
- [ ] seluruh test sebelumnya tetap pass
- [ ] "cargo test" berhasil
- [ ] tidak ada warning baru
- [ ] tidak ada opcode/spec semantics yang diinvent
- [ ] final bytecode tidak mengandung unresolved placeholder

---

Final Report

Setelah selesai, laporkan:

1. Files created/modified
2. Compiler architecture yang digunakan
3. AST nodes yang berhasil dikompilasi
4. Constant Pool implementation
5. Control-flow/jump implementation
6. Function/closure implementation
7. Jumlah test
8. Hasil "cargo test"
9. Warning/error yang ditemukan dan diperbaiki
10. Bagian spec yang belum dapat diimplementasikan jika ada
11. Ambiguitas spec yang ditemukan jika ada

Jangan menyatakan job selesai jika acceptance criteria belum terpenuhi.
