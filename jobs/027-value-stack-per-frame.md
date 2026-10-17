# Job 027 — Value Stack per Execution Frame (ZeroVM.md §116-§117)

## Objective

Penuhi aturan **§116**: *"Value Stack bersifat lokal terhadap execution
frame"* — saat ini seluruh frame berbagi satu `stack: Vec<Value>` di level
VM (`src/vm/mod.rs:37`), tercatat sebagai satu-satunya baris **Not
Implemented** di CHECK.md.

Selesai: stack pindah ke field `Frame.stack`; semua operasi PUSH/POP/DUP/
SWAP/underflow, handler depth, marker finally, GC roots, dan task context
menjadi per-frame; perilaku observable tak berubah (semua 944 test lama
hijau) kecuali pembacaan internal `vm.stack` yang jadi `vm.frame_stack()`.

## Required Reading

- ZeroVM.md §116 (Value Stack — lokal per frame, LIFO, underflow → VM Error,
  batas stack impl-defined), §117 (Call Stack & Execution Frame), §115
  (execution model), §121 (roots).
- ISSUES §71a-§71g (limits — call-depth tetap berlaku), §59d (finally
  markers di stack), §130/§31 (closure/eval frames).
- src/vm/mod.rs (semua jalur `self.stack`), src/vm/task.rs (TaskContext
  save/load), src/vm/types.rs (Frame, TaskContext, collect_frame_refs).

## Scope

A. `Frame` mendapat field `stack: Vec<Value>`; `ZeroVM.stack` dan
   `TaskContext.stack` dihapus (stack hidup di dalam frame → ikut
   save/load/replace context otomatis).
B. Akses via helper `stack()` / `stack_mut()` (current frame; fallback ke
   slot `orphan_stack` hanya bila `call_stack` kosong) + view pub
   `get_stack()` (API lama, kini mengembalikan stack frame aktif).
C. Semua jalur menyesuaikan: operasi operand, call/return (nilai kembali
   ke stack pemanggil), try handler `stack_depth` kini dalam-frame,
   truncate unwind, marker finally, GC roots (`collect_frame_refs` ikut
   scan `frame.stack`, loop `self.stack` di `gc_roots` dihapus), task
   save/load/fail/clear, `evaluate_default_body`.
D. Top-level return (spec senyap soal `return` skala program): root frame
   tetap dibuang seperti sebelum refactor; nilai kembali jatuh ke slot
   `ZeroVM.orphan_stack` sehingga seluruh alur observabel (eksekusi ulang,
   pesan `No root frame` mode module, exit code) identik byte-per-byte
   dengan binary baseline — keputusan mekanis dicatat di ISSUES §75a,
   quirk pre-existing dicatat sebagai candidate gap di CHECK §18.
E. Test baru `test_job027_*`.

## Out of Scope

- Batas maksimum stack (§116 mengizinkan impl-defined; tidak dikonfigurasi
  — dicatat sebagai catatan, bukan gap baru).
- Perubahan compiler/opcode/§124-§125/§158 (Frame adalah runtime state,
  tidak ikut serialisasi bytecode).
- Perubahan limits §150.

## Testing Requirements

1. Semua 944 test existing tetap hijau (termasuk suite async/gc/try/
   default-param/bytecode/CLI) — perilaku tak boleh berubah.
2. Test baru: fungsi berserakan intermediate value TIDAK meninggalkan
   apapun di stack frame pemanggil (bandingkan frame_stack program dengan
   pemanggilan fungsi "junk" vs tanpanya).
3. Test baru: recursion dalam (mis. puluhan frame) → hasil benar; stack
   frame root bersih dari intermediate callee.
4. Test baru: try/catch lintas frame tetap memotong stack ke kedalaman
   try-entry dalam-frame (unwind residue = 0).
5. `cargo build` warning tetap persis 2 (baseline).

## Acceptance Criteria

- §116 baris 158 terpenuhi dan diuji; CHECK.md baris §116 → Implemented,
  ringkasan §16 diperbarui.
- Nol perubahan perilaku observable; nol perubahan format bytecode/§158.

## Required Final Report

1. Files changed + keputusan mekanis (root-frame retention, frame_stack
   view, batas stack tak dikonfigurasi) + lokasi catatan (ISSUES §75).
2. Peta aturan §116-§117 → mekanisme kode + test.
3. Full suite (angka) + warning count.
4. Konfirmasi: serialisasi bytecode tak tersentuh, tasks/async hijau,
   GC roots mencakup stack per-frame.
