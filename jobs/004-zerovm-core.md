Job 004 — ZeroVM Core / Execution Loop

1. Objective

Implement core ZeroVM execution engine untuk menjalankan bytecode ZeroScript yang dihasilkan oleh Compiler.

Pipeline yang harus mulai dapat dijalankan:

Source
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
Execution

Gunakan spesifikasi ZeroScript sebagai source of truth.

Jangan mengubah semantics bahasa, bytecode, atau membuat opcode baru hanya agar implementasi lebih mudah.

---

2. Required Reading

Sebelum implementasi, baca:

1. "AGENTS.md"
2. "INDEX.md"
3. "Language.md" — hanya bagian yang relevan dengan execution semantics
4. "Bytecode.md"
5. "ZeroVM.md"
6. "Runtime.md"
7. "ISSUES.md"
8. "src/token.rs"
9. "src/ast.rs"
10. "src/parser.rs"
11. "src/compiler.rs"
12. "src/error.rs"
13. "src/main.rs"

Periksa juga seluruh test yang sudah ada sebelum mengubah architecture.

---

3. Scope

Job ini hanya menangani core VM execution.

Implementasikan:

- ZeroVM structure
- VM initialization
- bytecode loading
- instruction pointer / program counter
- instruction fetch
- instruction decode/dispatch
- operand handling
- value stack
- basic execution state
- VM errors
- bytecode validation required before execution
- execution termination
- basic execution tests
- integration dengan bytecode hasil compiler

Gunakan instruction set yang sudah didefinisikan di "Bytecode.md".

---

4. Out of Scope

Jangan implementasikan fitur berikut dalam job ini kecuali "ZeroVM.md" secara eksplisit mensyaratkan bagian minimalnya agar core VM dapat berjalan:

- garbage collector
- advanced heap management
- module loader
- dependency manager
- async scheduler
- task system
- standard library
- native functions
- JIT
- optimization
- debugger UI
- profiler
- networking
- filesystem runtime API
- process management
- advanced closure capture
- full exception-handler stack
- advanced function table/runtime linking

Jika sebuah opcode membutuhkan subsystem yang belum dibuat, jangan mengarang behavior baru.

Gunakan error/state yang sesuai dengan spesifikasi atau tandai sebagai dependency untuk job berikutnya.

---

5. ZeroVM Architecture

Buat struktur VM yang secara konsep memiliki:

ZeroVM
├── Bytecode
├── Program Counter
├── Value Stack
├── Call Stack / Execution Frames
├── Runtime State
└── Execution Status

Nama struct dan file boleh disesuaikan dengan codebase selama semantics tetap sama.

Pisahkan state VM dari compiler.

Compiler menghasilkan bytecode.

ZeroVM mengeksekusi bytecode.

Jangan membuat compiler melakukan runtime evaluation.

---

6. Bytecode Loading

VM harus dapat menerima bytecode hasil compiler.

Saat bytecode dimuat:

1. Validasi bytecode terlebih dahulu.
2. Pastikan constant pool valid.
3. Pastikan instruction stream valid.
4. Pastikan operand instruction valid.
5. Pastikan jump target valid.
6. Pastikan metadata yang diperlukan VM valid.
7. Tolak bytecode invalid sebelum execution dimulai.

Jangan menjalankan bytecode yang gagal validation.

Jika "Bytecode.md" mendefinisikan bytecode version atau format identifier, ikuti format tersebut.

Jangan membuat format baru.

---

7. Program Counter

VM harus memiliki program counter/instruction pointer.

Execution loop secara konseptual:

while VM masih berjalan:
    fetch instruction pada PC
    decode instruction
    execute instruction
    update PC

Perhatikan instruction yang mengubah control flow.

Jangan melakukan increment PC dua kali pada jump/branch instruction.

Pastikan:

- PC awal valid
- PC tidak membaca instruction di luar bytecode
- jump target valid
- execution berhenti pada instruction termination/halt sesuai spec

---

8. Instruction Dispatch

Implementasikan dispatch mechanism berdasarkan opcode yang benar-benar ada di "Bytecode.md".

Contoh berikut hanya konsep:

fetch
  ↓
decode opcode
  ↓
match opcode
  ↓
execute

Jangan menambahkan opcode seperti:

LOAD_CONST
ADD
RETURN

jika nama/format tersebut tidak sama dengan opcode yang didefinisikan di "Bytecode.md".

Gunakan opcode aktual dari specification.

Setiap opcode harus memiliki behavior yang sesuai dengan specification.

---

9. Value Stack

Implementasikan value stack untuk execution.

Minimal harus mendukung:

- push
- pop
- peek/top
- stack depth
- underflow detection

Stack underflow harus menghasilkan VM error.

Jangan menggunakan panic normal untuk kondisi runtime yang seharusnya menjadi VM Error.

Contoh konsep:

push(value)
pop()
peek()

Jika bytecode meminta operand tetapi stack tidak memiliki jumlah value yang diperlukan, VM harus menghasilkan error yang sesuai.

---

10. Runtime Values

Gunakan representation Runtime Value yang konsisten dengan ZeroScript.

Value yang sudah didefinisikan oleh bahasa antara lain:

number
string
boolean
null
list
object
function

Jangan menambahkan runtime type yang bertentangan dengan "Language.md" atau "ZeroVM.md".

Khusus:

- "NaN" bukan ZeroScript runtime value.
- "Infinity" bukan ZeroScript runtime value.
- Arithmetic yang menghasilkan nilai non-finite harus menghasilkan runtime error sesuai specification.
- "null" arithmetic behavior harus mengikuti "Language.md" dan "ISSUES.md".

Jika representation untuk list/object/function membutuhkan heap reference, gunakan abstraction yang sesuai dengan VM design tanpa mencoba menyelesaikan seluruh GC pada job ini.

---

11. Constant Pool

Gunakan Constant Pool yang dihasilkan Compiler.

VM harus dapat mengambil constant berdasarkan index.

Validasi:

index < constant_pool.length

Index invalid harus menghasilkan VM Error.

Constant Pool tidak boleh dimodifikasi oleh program selama execution.

Jika constant digunakan berkali-kali, VM harus dapat mengambilnya kembali melalui index yang sama.

Jangan membuat duplicate constant pool secara tidak perlu.

---

12. Basic Instruction Execution

Implementasikan semua instruction yang memang diperlukan oleh basic bytecode execution dan yang sudah didefinisikan di "Bytecode.md".

Prioritaskan execution path yang memungkinkan program sederhana berjalan:

literal
↓
constant
↓
expression
↓
stack operation
↓
result

Kemudian implementasikan instruction lain sesuai scope yang ditentukan oleh Bytecode specification.

Jangan menandai instruction sebagai selesai hanya karena dispatch case sudah dibuat.

Setiap implemented instruction harus memiliki behavior dan test yang sesuai.

---

13. Arithmetic

Untuk arithmetic instruction yang sudah didefinisikan specification:

- gunakan Runtime Value yang benar
- validasi operand type
- ikuti operator semantics
- jangan melakukan implicit coercion yang tidak diperbolehkan
- handle division by zero
- handle modulo by zero
- handle non-finite arithmetic result
- ikuti "null → 0" rule hanya pada arithmetic yang memang diperbolehkan

Arithmetic error harus menjadi VM/runtime error, bukan Rust panic.

---

14. Comparison and Boolean Operations

Implementasikan comparison/equality/boolean instructions sesuai "Bytecode.md" dan "Language.md".

Jangan menyamakan:

==

dengan:

===

Jika keduanya direpresentasikan sebagai instruction berbeda, semantics harus dipertahankan.

Tidak boleh memperkenalkan truthy/falsy conversion jika specification tidak mengizinkannya.

---

15. Variable Execution

Jika bytecode specification sudah menyediakan variable/local/global instructions, implementasikan sesuai definisinya.

Perhatikan:

- "let"
- "const"
- mutable vs immutable binding
- lexical scope
- shadowing
- assignment
- compound assignment

Jangan membuat semua variable menjadi global hanya karena lebih mudah.

Jika full lexical environment belum menjadi scope job ini, buat abstraction yang dapat dilanjutkan oleh job berikutnya tanpa mengubah semantics.

---

16. Control Flow

Implementasikan basic control-flow execution untuk opcode yang memang sudah tersedia.

Minimal perhatikan:

- unconditional jump
- conditional branch
- loop back-edge
- termination

Pastikan nested control flow tidak merusak PC.

Contoh konsep:

condition
   ↓
branch
 ↙   ↘
body  alternate
 ↓
jump
 ↓
continue

Jump target harus berupa target bytecode yang sudah tervalidasi.

Tidak boleh ada unresolved placeholder ketika VM mulai execution.

---

17. VM Execution Status

VM harus memiliki status execution yang jelas.

Minimal secara konseptual:

running
completed
failed

Jika "ZeroVM.md" menentukan state tambahan, ikuti specification.

Behavior:

- "running" → execution masih berlangsung
- "completed" → program selesai normal
- "failed" → runtime/VM error menghentikan execution

VM tidak boleh terus menjalankan instruction setelah terminal state tercapai.

---

18. VM Errors

Buat error type khusus untuk VM jika belum tersedia.

Error harus dapat membedakan minimal:

- invalid bytecode
- invalid opcode
- invalid operand
- invalid constant index
- invalid jump target
- stack underflow
- invalid runtime operation
- invalid execution state
- runtime arithmetic error
- execution failure

Sertakan informasi yang membantu debugging jika tersedia:

- program counter
- opcode
- relevant operand
- error category

Jangan menggunakan "unwrap()"/"expect()" untuk kondisi runtime yang dapat berasal dari bytecode atau program.

Rust panic hanya boleh digunakan untuk internal invariant yang benar-benar mustahil setelah validation, bukan untuk input bytecode yang invalid.

---

19. Bytecode Validation

Buat validation layer sebelum execution.

Minimal validasi:

Bytecode
├── version/format
├── constant indices
├── opcode validity
├── operand validity
├── jump targets
├── metadata consistency
└── structural validity

Validation tidak harus menyelesaikan seluruh semantic verification VM jika specification menempatkannya pada runtime.

Jangan membuat validation terlalu pintar sampai mengubah semantics language.

---

20. Integration With Compiler

Hubungkan:

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
Execution

Buat test integration sederhana yang benar-benar:

1. membuat source ZeroScript
2. tokenize
3. parse
4. compile
5. menghasilkan bytecode
6. menjalankan bytecode dengan ZeroVM
7. memeriksa execution result/state

Jangan hanya membuat VM test dengan bytecode manual.

Manual bytecode tests tetap boleh digunakan untuk menguji instruction secara terisolasi.

---

21. Execution Result

Jika "ZeroVM.md" menentukan execution result, ikuti format tersebut.

Jika belum tersedia, gunakan abstraction yang tidak mengunci API secara berlebihan.

Execution result harus dapat membedakan setidaknya:

successful completion
runtime/VM error

Jangan mengarang API publik besar hanya untuk kebutuhan test.

---

22. Tests

Tambahkan unit/integration tests untuk:

VM initialization

- empty/minimal bytecode
- valid bytecode loading
- invalid bytecode rejection

Constant Pool

- valid constant access
- multiple constant access
- invalid constant index

Stack

- push/pop
- peek
- stack depth
- underflow

Execution

- simple program
- literal execution
- arithmetic
- comparison
- boolean operation
- variable operation jika opcode sudah tersedia
- control flow
- termination

Errors

- invalid opcode
- invalid operand
- invalid jump
- stack underflow
- invalid runtime operation
- arithmetic errors

Integration

Gunakan compiler:

source → lexer → parser → compiler → VM

untuk beberapa program ZeroScript sederhana.

Semua test sebelumnya harus tetap pass.

---

23. Regression Requirements

Sebelum menyatakan job selesai:

cargo test

harus berhasil.

Tidak boleh ada:

failed tests
ignored tests added hanya untuk menghindari failure
unexpected panic

Perhatikan compiler dan parser tests yang sudah ada.

Jangan menghapus atau melemahkan test lama hanya agar implementation pass.

---

24. Warnings

Perbaiki warning baru yang disebabkan implementation Job 004.

Warning yang benar-benar berasal dari bagian specification yang belum digunakan boleh tetap ada jika memang tidak realistis menghilangkannya pada job ini, tetapi laporkan secara eksplisit.

Jangan membuat:

#[allow(dead_code)]

secara massal hanya untuk menyembunyikan warning.

Gunakan hanya jika memang diperlukan dan jelaskan alasannya.

---

25. Constraints

1. "Bytecode.md" adalah source of truth untuk instruction set.
2. "ZeroVM.md" adalah source of truth untuk VM architecture.
3. "Language.md" adalah source of truth untuk language semantics.
4. Jangan membuat opcode baru.
5. Jangan mengubah semantics ZeroScript.
6. Jangan melakukan optimization yang mengubah behavior.
7. Jangan mengimplementasikan GC penuh.
8. Jangan mengimplementasikan async runtime.
9. Jangan mengimplementasikan module/dependency system.
10. Jangan menghapus test existing.
11. Jangan mengganti compiler architecture tanpa alasan yang diperlukan.
12. Jika menemukan ambiguity, baca "ISSUES.md".
13. Jika tetap ambiguous, jangan menebak.
14. Jangan mengubah specification kecuali job secara eksplisit memintanya.
15. Jangan membuat fake runtime behavior hanya agar test terlihat selesai.

---

26. Expected Structure

Struktur akhir dapat berupa:

src/
├── ast.rs
├── compiler.rs
├── error.rs
├── lexer.rs
├── main.rs
├── parser.rs
├── token.rs
└── vm.rs

Jika "vm.rs" membutuhkan module tambahan, gunakan struktur minimal yang masuk akal.

Jangan membuat puluhan file hanya untuk memecah implementation kecil.

---

27. Completion Criteria

Job 004 dianggap selesai jika:

- [ ] ZeroVM core tersedia
- [ ] bytecode dapat dimuat
- [ ] bytecode divalidasi sebelum execution
- [ ] program counter berjalan benar
- [ ] instruction dispatch berjalan
- [ ] value stack tersedia
- [ ] constant pool dapat diakses
- [ ] basic runtime values dapat dieksekusi
- [ ] instruction yang termasuk scope sudah diimplementasikan
- [ ] control flow dasar bekerja
- [ ] execution dapat terminate secara normal
- [ ] VM error tersedia
- [ ] invalid bytecode ditolak
- [ ] compiler → bytecode → VM integration test tersedia
- [ ] semua test lama tetap pass
- [ ] "cargo test" berhasil
- [ ] tidak ada unresolved placeholder pada execution path
- [ ] tidak ada opcode buatan sendiri
- [ ] tidak ada perubahan semantics yang tidak dispesifikasikan

Bagian yang memang membutuhkan subsystem job berikutnya harus dilaporkan, bukan dipalsukan sebagai selesai.

---

28. Final Report

Setelah implementasi selesai, laporkan:

1. File yang dibuat/diubah.
2. Architecture ZeroVM yang digunakan.
3. Struktur execution loop.
4. Instruction yang sudah didukung.
5. Constant Pool integration.
6. Value stack.
7. Program counter/control flow.
8. Bytecode validation.
9. VM error handling.
10. Integration test.
11. Jumlah total test.
12. Hasil "cargo test".
13. Warning/error yang ditemukan dan diperbaiki.
14. Bagian ZeroVM yang belum dapat diimplementasikan.
15. Dependency untuk job berikutnya.
16. Ambiguity yang ditemukan, jika ada.

Jangan mengklaim subsystem sebagai fully implemented jika implementation masih berupa placeholder/simplification.

Gunakan format:

Implemented:
- ...

Tests:
- ...

Not yet implemented:
- ...

Dependencies for next job:
- ...

Ambiguities:
- ...

29. Execution Command

Setelah membaca seluruh required files, langsung kerjakan job ini.
