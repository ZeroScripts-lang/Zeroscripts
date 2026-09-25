# INDEX — Peta Section ZeroScript Specification

Dokumen asli `ZeroScript.md` dipecah menjadi beberapa file domain di bawah ini.
Nomor dan urutan section asli dipertahankan apa adanya di masing-masing file.

> Catatan: Section 130 (ZeroVM Closure Environment) ditambahkan atas permintaan user — section ini tidak ada di file ZeroScript.md yang diupload sebelumnya. Section 67 (Module Re-export) sebelumnya sempat kepotong karena diawali typo "Gas " di dokumen asli sehingga tidak terdeteksi sebagai heading; sudah diperbaiki dan dipisah ke Modules.md.

| Section | Judul | File |
|---|---|---|
| 1 | Filosofi | Language.md |
| 2 | Variables | Language.md |
| 3 | Scope | Language.md |
| 4 | Function Scope Assignment | Language.md |
| 5 | Duplicate Declaration | Language.md |
| 6 | Types | Language.md |
| 7 | null | Language.md |
| 8 | Arithmetic Operators | Language.md |
| 9 | Operator Precedence | Language.md |
| 10 | Unary Operators | Language.md |
| 11 | Increment / Decrement | Language.md |
| 12 | Compound Assignment | Language.md |
| 13 | Assignment | Language.md |
| 14 | Equality | Language.md |
| 15 | Comparison | Language.md |
| 16 | Boolean | Language.md |
| 17 | if | Language.md |
| 18 | Ternary Operator | Language.md |
| 19 | while | Language.md |
| 20 | loop | Language.md |
| 21 | break | Language.md |
| 22 | continue | Language.md |
| 23 | switch / case | Language.md |
| 24 | Functions | Language.md |
| 25 | Function Arguments | Language.md |
| 26 | Return | Language.md |
| 27 | Recursive Functions | Language.md |
| 28 | Nested Functions | Language.md |
| 29 | Anonymous Functions | Language.md |
| 30 | Default Parameters | Language.md |
| 31 | Closure | Language.md |
| 32 | Arrow Functions | Language.md |
| 33 | Lists | Language.md |
| 34 | List Mutability | Language.md |
| 35 | List / Object References | Language.md |
| 36 | String Indexing | Language.md |
| 37 | Objects | Language.md |
| 38 | Object Access | Language.md |
| 39 | Object Mutation | Language.md |
| 40 | Function Parameters with Objects/Lists | Language.md |
| 41 | String Comparison | Language.md |
| 42 | String Standard Library | StandardLibrary.md |
| 43 | Error Handling | Language.md |
| 44 | Console Output | StandardLibrary.md |
| 45 | Color | StandardLibrary.md |
| 46 | Module / Import | Modules.md |
| 47 | Export | Modules.md |
| 48 | File System | StandardLibrary.md |
| 49 | Async / Await | Async.md |
| 50 | HTTP | StandardLibrary.md |
| 50.1 | HTTP ADDITIONAL CONCEPT | StandardLibrary.md |
| 51 | JSON | StandardLibrary.md |
| 52 | System | StandardLibrary.md |
| 53 | Type Introspection | Language.md |
| 54 | Explicit Type Conversion | Language.md |
| 55 | Runtime Architecture | Runtime.md |
| 56 | Current Design Philosophy | Language.md |
| 57 | Planned / Not Yet Finalized | ISSUES.md |
| 58 | String API — tambahan | StandardLibrary.md |
| 59 | HTTP Query Parameters | StandardLibrary.md |
| 60 | HTTP Headers | StandardLibrary.md |
| 61 | Module Initialization | Modules.md |
| 62 | Circular Import | Modules.md |
| 63 | Module Import/Export Errors | Modules.md |
| 64 | Module Path Resolution | Modules.md |
| 65 | Module Scope & Encapsulation | Modules.md |
| 66 | Module Export Binding | Modules.md |
| 67 | Module Re-export | Modules.md |
| 68 | Module Export Conflict | Modules.md |
| 69 | Module Import Alias | Modules.md |
| 70 | Module Import Multiple Bindings | Modules.md |
| 71 | Module Import Initialization Order | Modules.md |
| 72 | Module Import Execution | Modules.md |
| 73 | Module Evaluation Failure | Modules.md |
| 74 | Module Export Mutability | Modules.md |
| 75 | Module Import Declaration Scope | Modules.md |
| 76 | Module Namespace Isolation | Modules.md |
| 77 | Module Self-Import | Modules.md |
| 78 | Module Import Ordering | Modules.md |
| 79 | Module Import Duplicate | Modules.md |
| 80 | Module System Completion | Modules.md |
| 81 | Package & Dependency System | Dependencies.md |
| 82 | Project Manifest | Dependencies.md |
| 83 | Package Versioning | Dependencies.md |
| 84 | Dependency Lockfile | Dependencies.md |
| 85 | Dependency Installation | Dependencies.md |
| 86 | Project Scripts | Dependencies.md |
| 87 | Package Resolution During Execution | Dependencies.md |
| 88 | Package Dependency Tree | Dependencies.md |
| 89 | Dependency Scope | Dependencies.md |
| 90 | Dependency Conflict Resolution | Dependencies.md |
| 91 | Dependency Removal | Dependencies.md |
| 92 | Dependency Update | Dependencies.md |
| 93 | Git Package Source | Dependencies.md |
| 94 | Package Publishing | Dependencies.md |
| 95 | Package Registry | Dependencies.md |
| 96 | Package Metadata | Dependencies.md |
| 97 | Package Integrity | Dependencies.md |
| 98 | Package Cache | Dependencies.md |
| 99 | Package Source Priority | Dependencies.md |
| 100 | Package System Completion | Dependencies.md |
| 101 | Asynchronous Tasks | Async.md |
| 102 | Concurrent Task Execution | Async.md |
| 103 | Task Group | Async.md |
| 104 | Task Race | Async.md |
| 105 | Task Cancellation | Async.md |
| 106 | Task State | Async.md |
| 107 | Task Result | Async.md |
| 108 | Task Error Handling | Async.md |
| 109 | Task Waiting | Async.md |
| 110 | Task Timeout | Async.md |
| 111 | Task Sleep | Async.md |
| 112 | Task Scheduling | Async.md |
| 113 | Asynchronous Task Completion | Async.md |
| 114 | ZeroVM Architecture | ZeroVM.md |
| 115 | ZeroVM Execution Model | ZeroVM.md |
| 116 | ZeroVM Value Stack | ZeroVM.md |
| 117 | ZeroVM Call Stack & Execution Frame | ZeroVM.md |
| 118 | ZeroVM Heap | ZeroVM.md |
| 119 | ZeroVM Runtime Values | ZeroVM.md |
| 120 | ZeroVM Memory Management | ZeroVM.md |
| 121 | ZeroVM Garbage Collection | ZeroVM.md |
| 122 | ZeroVM Memory Safety | ZeroVM.md |
| 123 | ZeroVM Error Boundary | ZeroVM.md |
| 124 | ZeroVM Bytecode Validation | Bytecode.md |
| 125 | ZeroVM Bytecode Versioning | Bytecode.md |
| 126 | ZeroVM Constant Pool | Bytecode.md |
| 127 | ZeroVM Variable Instructions | Bytecode.md |
| 128 | ZeroVM Control Flow Instructions | Bytecode.md |
| 129 | ZeroVM Function Instructions | Bytecode.md |
| 130 | ZeroVM Closure Environment | ZeroVM.md |
| 131 | ZeroVM Reference Tracking | ZeroVM.md |
| 132 | ZeroVM Heap Allocation | ZeroVM.md |
| 133 | ZeroVM Memory Limits | ZeroVM.md |
| 134 | ZeroVM Literal Instructions | Bytecode.md |
| 135 | ZeroVM Stack Instructions | Bytecode.md |
| 136 | ZeroVM Arithmetic Instructions | Bytecode.md |
| 137 | ZeroVM Unary Instructions | Bytecode.md |
| 138 | ZeroVM Comparison Instructions | Bytecode.md |
| 139 | ZeroVM Boolean Instructions | Bytecode.md |
| 140 | ZeroVM Collection Access Instructions | Bytecode.md |
| 141 | ZeroVM Function Call Convention | Bytecode.md |
| 142 | ZeroVM Return Convention | Bytecode.md |
| 143 | ZeroVM Function Metadata | Bytecode.md |
| 144 | ZeroVM Object and List Instructions | Bytecode.md |
| 145 | ZeroVM Standard Operation Boundary | Bytecode.md |
| 146 | ZeroVM Native Operation Calling | Bytecode.md |
| 147 | ZeroVM Runtime Context | Runtime.md |
| 148 | ZeroVM I/O Boundary | Runtime.md |
| 149 | ZeroVM Runtime Configuration | Runtime.md |
| 150 | ZeroVM Execution Limits | Runtime.md |
| 151 | ZeroVM Runtime Isolation | Runtime.md |
| 152 | ZeroVM Resource Cleanup | Runtime.md |
| 153 | ZeroVM Program Termination | Runtime.md |
| 154 | ZeroScript Runtime Error Reporting | Runtime.md |
| 155 | ZeroScript Debug Information | Runtime.md |
| 156 | ZeroScript Core Specification Completion | Runtime.md |

## Ringkasan File
- **Language.md** — ZeroScript — Language Specification (45 section)
- **Modules.md** — ZeroScript — Module System (22 section)
- **Dependencies.md** — ZeroScript — Package & Dependency System (20 section)
- **Async.md** — ZeroScript — Asynchronous Tasks & Concurrency (14 section)
- **Bytecode.md** — ZeroScript — Bytecode & Instruction Set (19 section)
- **ZeroVM.md** — ZeroScript — ZeroVM Architecture & Runtime Model (14 section)
- **StandardLibrary.md** — ZeroScript — Standard Library (11 section)
- **Runtime.md** — ZeroScript — Runtime Configuration & Operations (11 section)
- **ISSUES.md** — ZeroScript — Open Issues / Not Yet Finalized (1 section)
