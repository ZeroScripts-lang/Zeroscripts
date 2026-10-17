Job 007 — Function & Closure Runtime

Objective

Implementasikan runtime function dan closure ZeroScript pada ZeroVM sesuai semantics yang sudah ditentukan dalam spec.

Job ini menyelesaikan function invocation dan lexical closure runtime yang sebelumnya masih disederhanakan pada Compiler/ZeroVM, termasuk environment capture dan integrasi closure dengan Garbage Collector.

---

Required Reading — WAJIB

Sebelum melakukan perubahan apa pun, baca:

1. "AGENTS.md"
2. "Language.md"
3. "ZeroVM.md"
4. "Runtime.md"
5. "Bytecode.md"
6. source code compiler, AST, bytecode, RuntimeValue, Heap, GC, dan VM yang relevan

Jangan membaca task lama sebagai source of truth. Implementation saat ini adalah hasil dari job sebelumnya.

Spec adalah source of truth.

Jangan mengarang atau mengubah semantics ZeroScript yang tidak tercantum dalam spec.

---

Scope

Implementasikan:

- runtime function representation
- function invocation
- function call frames
- parameter binding
- argument handling
- return value handling
- lexical environments
- closure creation
- closure capture
- captured variable access
- captured "let" mutation
- captured "const" immutability
- closure lifetime
- closure environment sebagai GC root
- integration dengan existing RuntimeValue / Heap / GC
- compiler/bytecode integration yang memang diperlukan
- unit tests
- VM integration tests
- regression testing seluruh test suite

---

Function Runtime

Function harus memiliki runtime representation yang berbeda dari primitive "number".

Jangan merepresentasikan callable function sebagai "Number".

Function runtime harus dapat menyimpan informasi yang diperlukan untuk invocation, seperti:

- function body/code reference
- parameter metadata
- lexical environment jika function merupakan closure
- metadata lain yang memang diperlukan oleh existing bytecode architecture

Gunakan representation yang kompatibel dengan RuntimeValue dan Heap dari Job 005.

Jangan membuat sistem RuntimeValue atau Heap kedua.

---

Function Invocation

Function invocation harus mengikuti semantics ZeroScript yang sudah ditentukan.

Pastikan:

- argument dievaluasi sebelum invocation sesuai evaluation order yang sudah ditentukan
- parameter menjadi local binding function
- parameter bersifat mutable seperti "let"
- missing argument menghasilkan "null" atau default parameter sesuai spec
- extra argument menghasilkan error sesuai spec
- return value dikembalikan ke caller
- function tanpa explicit return menghasilkan behavior sesuai spec
- recursion tetap dapat berjalan
- nested function tetap dapat dibuat dan dipanggil
- anonymous/arrow function yang sudah didukung compiler dapat digunakan jika representation bytecode-nya sudah tersedia

Jangan mengubah syntax function.

---

Default Parameters

Pertahankan semantics default parameter yang sudah ditentukan spec:

- default expression dievaluasi ketika function dipanggil
- default dapat menggunakan outer lexical variables
- default dapat menggunakan parameter sebelumnya
- default tidak dapat menggunakan parameter yang dideklarasikan setelahnya

Jangan melakukan eager evaluation ketika function didefinisikan.

Jika compiler saat ini belum memiliki representation yang diperlukan, lakukan perubahan minimum yang diperlukan agar runtime dapat menerapkan semantics tersebut.

---

Call Frames

Function invocation harus memiliki execution state yang benar.

Call frame harus menjaga informasi yang diperlukan untuk:

- return address
- function/code reference
- local bindings
- parameter bindings
- lexical environment
- stack/frame state yang diperlukan VM

Pastikan nested call dan recursion tidak merusak caller frame.

Contoh:

main
  ↓
function A
  ↓
function B
  ↓
return B
  ↓
return A
  ↓
main

Setiap frame harus memiliki lifecycle yang benar.

---

Lexical Environment

Implementasikan lexical environment untuk function dan closure.

Lookup harus mengikuti lexical scope:

current environment
        ↓
parent environment
        ↓
outer environment
        ↓
global environment

Function harus dapat membaca binding dari lexical scope tempat function tersebut dibuat, bukan berdasarkan lokasi function dipanggil.

Jangan menggunakan dynamic scoping.

---

Closure Capture

Ketika nested function menggunakan variable dari outer scope, variable tersebut harus dapat di-capture oleh closure.

Contoh konseptual:

function outer() {
    let count = 0

    function increment() {
        count++
        return count
    }

    return increment
}

Setelah "outer()" selesai, closure yang dikembalikan tetap harus dapat mengakses "count".

Environment tidak boleh dihancurkan hanya karena execution frame "outer" sudah selesai.

---

Captured "let"

Captured "let" harus mempertahankan semantics mutable.

Jika beberapa closure mengakses binding "let" yang sama, mereka harus melihat binding yang sama, bukan salinan independen.

Contoh konseptual:

function outer() {
    let value = 0

    function increment() {
        value++
    }

    function read() {
        return value
    }

    return [increment, read]
}

Mutation melalui "increment()" harus terlihat oleh "read()".

Jangan melakukan capture dengan copy-by-value jika semantics membutuhkan shared lexical binding.

---

Captured "const"

Captured "const" tetap immutable.

Closure boleh membaca captured "const", tetapi tidak boleh mengubahnya.

Attempt untuk melakukan assignment terhadap captured "const" harus menghasilkan error sesuai semantics variable immutability ZeroScript.

---

Closure Lifetime

Lexical environment harus tetap hidup selama masih diperlukan oleh closure yang reachable.

Heap/GC harus memperlakukan closure environment sebagai object yang dapat ditrace.

Contoh lifecycle:

outer frame
    ↓
closure
    ↓
captured environment
    ↓
captured variables

Setelah "outer" selesai:

outer frame       → destroyed
closure           → alive
environment       → alive
captured variables → alive

Jika closure kemudian menjadi unreachable, environment-nya juga harus dapat dikumpulkan oleh GC.

---

GC Integration

Integrasikan closure environment dengan GC dari Job 006 dan automatic GC dari Job 006.5.

Closure environment harus menjadi bagian dari object graph yang dapat ditrace.

Pastikan:

- reachable closure tetap hidup
- reachable captured environment tetap hidup
- captured heap references tetap hidup
- unreachable closure dapat dikumpulkan
- unreachable closure cycle dapat dikumpulkan
- closure yang masih reachable tidak kehilangan captured values

Jangan membuat mekanisme GC baru.

Gunakan GC/Heap existing.

---

Root Set

Perluas root set yang sudah tersedia agar closure environment yang masih reachable dari runtime execution dapat ditrace.

Jangan membuat fake global/module root.

Global/module bindings tetap out of scope untuk job ini jika subsystem tersebut belum tersedia.

Root yang harus dipertimbangkan sesuai architecture yang tersedia:

- value stack
- active call frames
- function/closure runtime values
- lexical environments yang direferensikan oleh active/reachable closures

Pastikan environment yang hanya hidup melalui closure tidak hilang ketika call frame pembuatnya sudah selesai.

---

Bytecode / Compiler Integration

Periksa hasil Job 003 dan Job 004 sebelum mengubah compiler atau bytecode.

Jika existing bytecode belum dapat merepresentasikan closure capture dengan benar, lakukan perubahan minimum yang diperlukan.

Jangan mendesain ulang seluruh instruction set.

Jika diperlukan instruction baru, dokumentasikan:

- opcode
- operands
- stack behavior
- runtime behavior

Perubahan bytecode harus tetap kompatibel dengan architecture ZeroVM yang sudah ada.

---

Error Handling

Function/closure runtime error harus menggunakan error mechanism yang sudah tersedia.

Pastikan tidak terjadi:

- Rust panic untuk runtime error normal
- dangling Rust reference
- use-after-free
- invalid environment access
- undefined behavior
- unsafe memory access tanpa alasan kuat

Invalid function/closure state harus menghasilkan runtime error yang sesuai.

---

Out of Scope

Jangan mengimplementasikan:

- module loader
- import/export runtime
- dependency manager
- global/module binding system
- async/task runtime
- standard library baru
- JIT
- optimizer
- native compilation
- debugger
- profiler
- generational/incremental/concurrent/compacting GC
- perubahan besar terhadap Heap/GC architecture yang sudah selesai

Job ini hanya menyelesaikan function dan closure runtime serta integration minimum yang diperlukan.

---

Testing Requirements

Tambahkan test untuk:

1. basic function invocation
2. function parameters
3. missing arguments
4. default parameters
5. extra arguments
6. return value
7. function without explicit return
8. nested function
9. anonymous function
10. arrow function jika sudah tersedia
11. recursion
12. lexical variable lookup
13. closure capture
14. closure surviving outer function return
15. captured "let" mutation
16. shared captured "let" between multiple closures
17. captured "const" read
18. captured "const" mutation error
19. multiple nested lexical environments
20. closure with heap-backed captured value
21. reachable closure survives GC
22. captured environment survives GC
23. unreachable closure collected by GC
24. unreachable closure cycle collected by GC
25. stale closure/environment reference produces safe error
26. nested function calls preserve caller frames
27. recursive calls preserve independent local frames
28. default parameter evaluation timing
29. default parameter referencing outer variable
30. default parameter referencing earlier parameter
31. invalid reference to later parameter
32. regression seluruh test sebelumnya

Jalankan:

cargo test

---

Acceptance Criteria

- function memiliki runtime representation yang benar
- callable tidak direpresentasikan sebagai "Number"
- function invocation berjalan melalui ZeroVM
- call frames bekerja untuk nested calls dan recursion
- parameter binding sesuai spec
- return value benar
- default parameter sesuai semantics spec
- lexical scoping berjalan
- closure capture berjalan
- captured "let" dapat dimutasi
- captured "let" shared antar closure jika berasal dari binding yang sama
- captured "const" tetap immutable
- closure dapat hidup setelah outer function selesai
- lexical environment terintegrasi dengan existing Heap
- lexical environment dapat ditrace oleh existing GC
- reachable closure/environment survive GC
- unreachable closure/environment dapat dikumpulkan
- tidak ada duplicate RuntimeValue/Heap/GC system
- tidak ada perubahan semantics bahasa yang tidak diperlukan
- existing tests tetap pass
- tests baru ditambahkan
- "cargo test" tidak memiliki failure

---

Required Final Report

Laporkan:

1. Files changed
2. Function runtime representation
3. Call frame design
4. Parameter/argument handling
5. Lexical environment design
6. Closure capture mechanism
7. Captured "let" behavior
8. Captured "const" behavior
9. GC integration
10. Root set changes
11. Bytecode/compiler changes jika ada
12. Tests added
13. Total test count
14. "cargo test" result
15. Remaining warnings
16. Known limitations
17. Intentionally unimplemented items

Jika terdapat conflict atau ambiguity antara "AGENTS.md", spec, dan implementation:

jangan menebak.

Laporkan conflict tersebut dan jelaskan keputusan yang diambil.
