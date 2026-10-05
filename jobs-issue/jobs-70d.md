Implement perbaikan §70d berdasarkan aturan semantics ZeroScript yang sudah ditetapkan.

Masalah:

- "compile()" menghasilkan semantics §4 yang benar: assignment di dalam function tidak mengubah binding outer.
- Pipeline module ("zsc"/"zpm") saat ini salah karena function biasa yang hanya melihat binding module-scope otomatis dianggap melakukan capture ("VarRes::Captured"), sehingga assignment mengubah binding outer.
- Contoh §4 wajib menghasilkan:
  "20"
  "10"

Aturan yang harus dipertahankan:

1. §4 Function Scope Assignment tetap berlaku: assignment di dalam function biasa bersifat local dan tidak mengubah binding outer.
2. §31 Closure tetap berlaku: closure dapat menangkap lexical scope dan memodifikasi captured variable.
3. §130 Closure Environment tetap berlaku: captured binding menggunakan shared environment dan mutation terlihat oleh closure lain yang berbagi environment.
4. §66 Live Export Binding tetap berlaku: mutation terhadap exported binding harus tetap terlihat oleh importer.
5. Jangan menyelesaikan masalah dengan sekadar mengubah "resolve_for_store()" secara global, karena itu berisiko merusak semantics closure.
6. Jangan mengubah §4 menjadi hanya berlaku untuk global. §4 adalah aturan umum function scope assignment sesuai contoh normatifnya.

Tugas:

- Telusuri bagaimana "new_module()" menentukan "VarRes::Captured" untuk "function" declaration biasa.
- Perbaiki classification/trigger capture agar function biasa tidak otomatis menjadi captured hanya karena binding module-scope terlihat secara lexical.
- Pastikan genuine closure tetap melakukan capture sesuai §31/§130.
- Pastikan live export binding §66 tidak rusak.
- Tambahkan regression tests yang secara eksplisit menguji:
  1. function biasa + assignment outer → "20, 10";
  2. genuine closure + captured mutable variable → mutation tetap shared;
  3. exported live binding → importer tetap melihat mutation.
- Jalankan seluruh test suite.
- Laporkan file yang diubah, alasan perubahan, test yang ditambahkan, serta hasil full test.

Jangan hanya mendokumentasikan §70d. Perbaiki implementasinya.
