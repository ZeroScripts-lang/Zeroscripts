Job 005 — Runtime Value + Heap

1. Objective

Implement fondasi runtime ZeroScript untuk Runtime Value dan Heap yang akan digunakan oleh ZeroVM dan subsystem runtime berikutnya.

Job ini harus membuat representation runtime yang konsisten untuk value ZeroScript dan menyediakan memory/heap abstraction yang aman untuk value yang membutuhkan storage/reference.

Gunakan specification sebagai source of truth.

Jangan mengubah semantics ZeroScript dan jangan membuat behavior yang tidak didefinisikan specification.

---

2. Required Reading

Sebelum implementasi, baca:

1. "AGENTS.md"
2. "INDEX.md"
3. "Language.md"
4. "Bytecode.md"
5. "ZeroVM.md"
6. "Runtime.md"
7. "ISSUES.md"
8. "src/ast.rs"
9. "src/compiler.rs"
10. "src/error.rs"
11. "src/vm.rs"
12. "src/token.rs"
13. "src/parser.rs"
14. "src/lexer.rs"
15. "src/main.rs"

Periksa test yang sudah ada sebelum mengubah runtime architecture.

---

3. Scope

Implementasikan:

- Runtime Value representation
- primitive runtime values
- heap-backed runtime values
- Heap abstraction
- heap allocation
- heap lookup/access
- heap reference representation
- list runtime representation
- object runtime representation
- function runtime representation dasar
- runtime type identification
- value equality/access helpers sesuai specification
- invalid heap reference handling
- integration dengan ZeroVM
- unit tests
- regression tests

---

4. Out of Scope

Jangan implementasikan penuh:

- Garbage Collector
- advanced closure capture
- async/task runtime
- module loader
- dependency manager
- standard library
- JIT
- optimizer
- native machine code
- debugger
- profiler
- networking
- filesystem APIs

GC akan menjadi subsystem terpisah.

Namun, desain Heap harus menyediakan abstraction yang memungkinkan GC ditambahkan kemudian tanpa membongkar seluruh Runtime Value architecture.

---

5. Runtime Value Model

Gunakan tipe runtime yang sesuai dengan "Language.md":

number
string
boolean
null
list
object
function

Runtime Value harus dapat membedakan primitive value dari heap-backed value.

Konsep:

RuntimeValue
├── Number
├── String / heap string jika architecture membutuhkan
├── Boolean
├── Null
├── List(reference)
├── Object(reference)
└── Function(reference)

Struktur actual boleh berbeda jika "ZeroVM.md" menentukan representation tertentu.

Jangan menambahkan runtime type yang bertentangan dengan specification.

---

6. Number Semantics

Pertahankan semantics ZeroScript:

- "number" dapat berupa integer maupun decimal.
- "NaN" bukan Runtime Value ZeroScript.
- "Infinity" bukan Runtime Value ZeroScript.
- arithmetic yang menghasilkan non-finite value harus menghasilkan runtime error sesuai specification.
- division by zero harus mengikuti specification.
- modulo by zero harus mengikuti specification.
- "null" arithmetic conversion harus mengikuti "Language.md" dan "ISSUES.md".

Jangan menggunakan Rust "NaN"/"Infinity" sebagai valid Runtime Value tanpa validation.

---

7. Null

Representasikan "null" sebagai runtime value yang jelas.

"null" harus dapat dibedakan dari:

- missing value
- invalid reference
- uninitialized Rust data
- empty collection

Jangan menggunakan "None" secara internal sebagai pengganti semantic "null" jika hal tersebut membuat keduanya tidak dapat dibedakan.

---

8. Strings

Implementasikan string runtime sesuai semantics ZeroScript.

Minimal support:

- store string
- retrieve string
- length/access helpers yang diperlukan VM
- immutable string semantics

String harus tidak dapat dimodifikasi melalui runtime operation yang seharusnya tidak mengizinkannya.

Jika string disimpan di heap, gunakan reference abstraction yang konsisten.

Jangan membuat string mutation API jika language specification tidak mendukung mutation.

---

9. Lists

Implementasikan heap-backed list representation.

List harus mendukung operasi yang dibutuhkan oleh runtime dan specification, seperti:

- creation
- index access
- index mutation untuk mutable list
- length
- add
- remove

Perhatikan semantics:

let list → mutable
const list → immutable

Jangan menentukan mutability hanya berdasarkan Heap reference.

Binding mutability dan object/list mutability harus mengikuti semantics ZeroScript.

Index out of bounds harus menjadi runtime error.

Jangan menggunakan Rust panic untuk index yang berasal dari program ZeroScript.

---

10. Objects

Implementasikan heap-backed object representation.

Object harus:

- menggunakan string keys
- mendukung property access
- mendukung dynamic/bracket access
- mempertahankan creation order jika specification mensyaratkannya
- mendukung key lookup
- mendukung mutation untuk mutable bindings
- mendukung remove/keys/values/entries sesuai runtime requirements

Missing key harus menghasilkan behavior yang ditentukan "Language.md".

Jangan silently return "null" untuk missing key jika specification menyatakan missing key sebagai error.

---

11. Function Runtime Representation

Implementasikan representation dasar untuk function sebagai runtime value.

Function representation minimal harus dapat menyimpan metadata yang memang diperlukan oleh VM.

Contoh metadata yang mungkin diperlukan:

function identifier
function bytecode/body reference
parameter information
function index

Gunakan metadata yang benar-benar tersedia dari compiler/bytecode specification.

Jangan mengimplementasikan closure capture penuh pada job ini jika runtime lexical environment belum tersedia.

Namun, desain representation harus memungkinkan closure support ditambahkan pada job berikutnya.

Jangan menyimpan function sebagai "number" hanya karena function index berbentuk integer.

Function harus memiliki runtime identity/type yang jelas.

---

12. Heap

Buat Heap abstraction yang menangani heap-backed runtime values.

Minimal secara konseptual:

Heap
├── allocate
├── get
├── get_mut
├── contains / validity check
└── reference validation

Actual API boleh disesuaikan dengan Rust architecture.

Heap reference harus memiliki representation yang memungkinkan VM mendeteksi invalid references.

---

13. Heap References

Runtime Value yang menunjuk ke heap harus menggunakan explicit reference/handle abstraction.

Contoh konseptual:

HeapRef
   ↓
Heap
   ↓
HeapObject

Jangan menyebarkan raw Rust pointers ke seluruh VM jika dapat dihindari.

Heap reference harus:

- valid jika allocation masih ada
- invalid jika reference tidak dikenal
- tidak menyebabkan undefined behavior
- dapat divalidasi sebelum access

Invalid reference harus menjadi VM/runtime error.

---

14. Heap Object

Gunakan abstraction untuk heap allocation.

Konsep:

HeapObject
├── String
├── List
├── Object
└── Function

Jika specification menentukan bentuk berbeda, ikuti specification.

HeapObject harus memungkinkan subsystem GC berikutnya mengetahui object kind dan references yang dimilikinya.

Jangan mengimplementasikan GC sekarang.

---

15. Mutability

Jangan menganggap:

same heap reference = same language-level mutability

Mutability adalah bagian dari language semantics.

Runtime architecture harus memungkinkan VM membedakan operation yang diizinkan oleh "let" dan "const".

Untuk "const" collection:

- reassign binding tidak boleh
- mutation terhadap collection harus mengikuti semantics "Language.md"

Jangan menggunakan "Arc", "Rc", "RefCell", atau struktur lain sebagai alasan untuk mengubah semantics language.

---

16. Type Identification

Pastikan runtime dapat mengidentifikasi:

number
string
boolean
null
list
object
function

Behavior harus konsisten dengan "type()" yang sudah didefinisikan oleh language specification.

"type()" harus dapat membedakan heap-backed types secara benar.

Jangan mengembalikan internal type seperti:

HeapObject
HeapRef
FunctionObject

sebagai language-level type.

---

17. Equality

Implementasikan helper/runtime behavior yang dibutuhkan oleh VM untuk equality.

Pertahankan perbedaan:

==

dan:

===

sesuai "Language.md".

Jangan menganggap Rust "PartialEq" otomatis sama dengan semantics ZeroScript.

Jika list/object/function equality memiliki semantics khusus di specification, ikuti specification.

Jika belum digunakan oleh VM pada job ini, buat abstraction yang dapat digunakan kemudian tanpa mengarang semantics baru.

---

18. ZeroVM Integration

Integrasikan Runtime Value + Heap dengan ZeroVM.

ZeroVM harus menggunakan runtime representation baru untuk value yang membutuhkan heap.

Minimal:

ZeroVM
├── Value Stack
├── Heap
├── Runtime Values
└── Execution State

Jangan membuat second independent runtime value system di "vm.rs".

Jika "vm.rs" sudah memiliki Runtime Value implementation sementara, refactor menuju satu source of truth.

Jangan meninggalkan dua representation yang berbeda tanpa alasan yang jelas.

---

19. Constant Pool Integration

Constant Pool tetap menjadi source untuk constant bytecode.

Ketika constant digunakan:

Constant Pool
      ↓
Runtime Value
      ↓
Value Stack / Heap

Primitive constants dapat digunakan sebagai value langsung jika architecture memungkinkan.

Heap-backed constants harus dialokasikan/reference sesuai design.

Jangan memodifikasi Constant Pool selama execution.

---

20. Memory Safety

Rust implementation harus menjaga memory safety.

Jangan menggunakan unsafe code kecuali benar-benar diperlukan oleh architecture specification.

Jika "unsafe" diperlukan:

- gunakan scope sekecil mungkin
- dokumentasikan invariant
- jangan gunakan untuk menghindari borrow checker secara sembarangan

Runtime invalid state harus menghasilkan error, bukan undefined behavior.

---

21. Error Handling

Tambahkan error type/variants yang diperlukan untuk:

- invalid heap reference
- invalid runtime type
- invalid collection access
- invalid mutation
- missing object key
- runtime allocation/access failure

Gunakan error system yang sudah ada jika memungkinkan.

Jangan membuat banyak error type yang duplikatif tanpa kebutuhan.

Error harus kompatibel dengan VM error architecture yang sudah ada.

---

22. Tests

Tambahkan unit tests untuk:

Runtime Value

- number
- string
- boolean
- null
- list reference
- object reference
- function reference
- type identification

Heap

- allocation
- retrieval
- mutable retrieval
- multiple allocations
- invalid reference
- reference validation

Strings

- storage
- access
- length
- immutability

Lists

- creation
- index access
- mutation
- add
- remove
- length
- out-of-bounds error

Objects

- creation
- key insertion
- key lookup
- dynamic lookup
- mutation
- removal
- keys/values/entries
- missing key behavior

Functions

- function representation
- function identity
- function metadata
- runtime type

Integration

Test:

source
  ↓
lexer
  ↓
parser
  ↓
compiler
  ↓
bytecode
  ↓
ZeroVM
  ↓
RuntimeValue + Heap

Gunakan actual ZeroScript source untuk integration tests.

---

23. Regression

Run:

cargo test

Semua test dari Job 001–004 harus tetap pass.

Jangan:

- menghapus test
- melemahkan assertion
- menambahkan "#[ignore]" untuk failure yang seharusnya diperbaiki
- menggunakan "unwrap()" untuk menghindari proper error handling
- mengubah language semantics demi membuat test pass

---

24. Warnings

Perbaiki warning baru yang disebabkan Job 005.

Jangan menggunakan broad:

#[allow(dead_code)]

hanya untuk menyembunyikan warning.

Jika field/API memang sengaja disiapkan untuk subsystem berikutnya dan belum digunakan, boleh dipertahankan, tetapi laporkan dalam final report.

---

25. Architecture Constraints

1. "Language.md" adalah source of truth untuk language semantics.
2. "ZeroVM.md" adalah source of truth untuk VM architecture.
3. "Bytecode.md" adalah source of truth untuk bytecode.
4. Jangan membuat runtime type baru tanpa specification basis.
5. Jangan mengubah opcode.
6. Jangan mengubah compiler semantics.
7. Jangan mengimplementasikan GC penuh.
8. Jangan mengimplementasikan closure capture penuh.
9. Jangan mengimplementasikan async.
10. Jangan mengimplementasikan JIT.
11. Jangan membuat duplicate Runtime Value systems.
12. Jangan menggunakan fake implementation hanya agar tests pass.
13. Jangan mengubah specification kecuali job secara eksplisit meminta perubahan.
14. Jika ada ambiguity, baca "ISSUES.md".
15. Jika ambiguity tetap ada, jangan menebak.

---

26. Suggested Structure

Jika architecture membutuhkan file baru:

src/
├── ast.rs
├── compiler.rs
├── error.rs
├── heap.rs
├── lexer.rs
├── main.rs
├── parser.rs
├── runtime_value.rs
├── token.rs
└── vm.rs

Nama file boleh disesuaikan.

Jangan memecah file secara berlebihan jika abstraction masih kecil.

---

27. Completion Criteria

Job 005 dianggap selesai jika:

- [ ] Runtime Value representation tersedia
- [ ] primitive values bekerja
- [ ] heap-backed values bekerja
- [ ] Heap abstraction tersedia
- [ ] Heap references tervalidasi
- [ ] list runtime tersedia
- [ ] object runtime tersedia
- [ ] function runtime representation tersedia
- [ ] runtime type identification tersedia
- [ ] mutability semantics tidak rusak
- [ ] invalid references menghasilkan error
- [ ] ZeroVM menggunakan Runtime Value/Heap yang sama
- [ ] Constant Pool terintegrasi
- [ ] unit tests tersedia
- [ ] integration tests tersedia
- [ ] semua test Job 001–004 tetap pass
- [ ] "cargo test" berhasil
- [ ] tidak ada fake runtime behavior
- [ ] GC belum diimplementasikan secara prematur
- [ ] architecture dapat dilanjutkan untuk GC dan closure

---

28. Final Report

Setelah selesai, laporkan:

1. File yang dibuat/diubah.
2. Runtime Value architecture.
3. Heap architecture.
4. Heap reference design.
5. List/Object representation.
6. Function representation.
7. ZeroVM integration.
8. Type identification.
9. Error handling.
10. Jumlah test baru.
11. Jumlah total test.
12. Hasil "cargo test".
13. Warning yang ditemukan dan diperbaiki.
14. Bagian yang sengaja belum diimplementasikan.
15. Dependency untuk job berikutnya.
16. Ambiguity jika ada.

Gunakan format:

Implemented:
- ...

Runtime Value:
- ...

Heap:
- ...

ZeroVM Integration:
- ...

Tests:
- ...

Not yet implemented:
- ...

Dependencies for next job:
- ...

Ambiguities:
- ...

Jangan menyatakan subsystem sebagai fully implemented jika masih berupa placeholder.

---

29. Execution Command

Setelah membaca required files, langsung kerjakan job ini.

Gunakan:

Read AGENTS.md and jobs/005-runtime-value-heap.md, then execute the job.
