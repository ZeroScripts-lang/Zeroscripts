Lakukan audit implementasi ZeroScript saat ini terhadap Spec yang tersedia di repository.

Tujuan utama: menghasilkan snapshot faktual tentang apa saja yang sudah diimplementasikan, apa yang belum, dan apa yang masih parsial, sehingga kita dapat menentukan Job 008 berdasarkan gap nyata setelah audit selesai.

Aturan penting

1. Spec adalah satu-satunya source of truth untuk semantics dan requirements ZeroScript.
2. Jangan mengarang semantics, requirements, roadmap, milestone, atau intended behavior yang tidak tertulis di Spec.
3. Jangan mengubah source code hanya untuk menyelesaikan audit ini.
4. Jangan membuat Job 008 sekarang.
5. Jangan menentukan desain implementasi baru.
6. Jika suatu requirement tidak dapat dipastikan dari Spec atau source code, tandai sebagai "Unknown" dan jelaskan apa yang tidak dapat dipastikan.
7. Bedakan dengan jelas:
   - Implemented
   - Partially Implemented
   - Not Implemented
   - Unknown
8. Gunakan source code dan test yang benar-benar ada sebagai bukti implementasi.
9. Jangan menganggap sebuah fitur implemented hanya karena ada nama struct/function/opcode yang berkaitan. Periksa behavior dan integrasinya.
10. Jangan menghapus atau mengubah file yang sudah ada.
11. Buat satu file baru bernama "CHECK.md" di root repository sebagai hasil audit.

Yang harus diaudit

Audit seluruh domain yang didefinisikan oleh Spec, termasuk tetapi tidak terbatas pada:

- Lexer
- Parser
- AST
- Compiler
- Bytecode
- Constant Pool
- ZeroVM
- Runtime Value
- Heap
- HeapRef
- Garbage Collector
- GC integration
- Function
- Closure
- lexical environment
- variable/binding semantics
- expression dan statement semantics
- error handling
- built-in/runtime behavior
- Modules
- Async/Task
- Standard Library
- dependency/runtime features
- domain lain yang memang didefinisikan oleh Spec

Jangan menganggap daftar di atas lengkap. Ikuti struktur dan isi Spec yang benar-benar tersedia.

Audit implementation yang sudah dikerjakan

Pastikan hasil audit secara eksplisit mencatat status dari:

- Job 001 — Lexer
- Job 002 — Parser
- Job 003 — Compiler & AST → Bytecode
- Job 004 — ZeroVM Core
- Job 005 — Runtime Value + Heap
- Job 006 — Garbage Collector
- Job 006.5 — GC VM Integration
- Job 007 — Function & Closure Runtime

Untuk Job 007, pastikan audit mencakup function invocation, call frames, parameter/default handling, recursion, lexical scope, closure capture, shared captured state, captured "let" mutation, captured "const", closure lifetime, serta hubungan closure environment dengan Heap dan GC.

Bukti

Untuk setiap domain/fitur yang diaudit, gunakan bukti konkret bila tersedia, misalnya:

- file/source location
- struct/enum/function/opcode terkait
- test yang membuktikan behavior
- hasil "cargo test"
- job terkait

Jangan perlu menyalin source code panjang ke "CHECK.md".

Format CHECK.md

Gunakan struktur yang jelas seperti:

ZeroScript Implementation Check

Audit Scope

Jelaskan Spec dan source tree yang diperiksa.

Status Legend

- "Implemented"
- "Partially Implemented"
- "Not Implemented"
- "Unknown"

Implementation Matrix

Domain / Feature| Status| Evidence| Related Job| Notes

Isi matrix berdasarkan hasil audit nyata.

Implemented

Ringkas fitur yang benar-benar sudah tersedia.

Partially Implemented

Jelaskan bagian yang sudah ada dan bagian yang masih kurang.

Not Implemented

Hanya masukkan fitur yang memang dapat dipastikan belum tersedia berdasarkan Spec dan source code.

Unknown / Needs Verification

Masukkan hal yang tidak dapat dipastikan tanpa informasi tambahan.

Test Status

Catat hasil test terakhir yang benar-benar dijalankan.

Saat ini, jika hasil test masih sama dengan yang baru saja diverifikasi, catat:

"355 passed; 0 failed; 0 ignored"

Jangan mengklaim hasil test tanpa benar-benar menjalankannya.

Current Implementation Boundary

Jelaskan batas implementasi saat ini secara faktual. Jangan membuat roadmap.

Candidate Gaps for Future Work

Hanya daftar gap yang ditemukan dari audit. Jangan memilih atau mendesain Job 008.

Setelah audit

Setelah "CHECK.md" selesai:

1. tampilkan ringkasan singkat hasil audit;
2. sebutkan file yang dibuat;
3. sebutkan test yang dijalankan dan hasilnya;
4. jangan melakukan implementasi tambahan;
5. jangan membuat commit;
6. jangan mengubah Spec;
7. jangan membuat Job 008.

Berhenti setelah audit dan laporan selesai.
