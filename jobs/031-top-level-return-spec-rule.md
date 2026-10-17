# Job 031 — Top-level `return`: aturan bahasa (keputusan B)

## Objective

Menetapkan aturan top-level `return` (keputusan user 2026-10-12, opsi B)
dan merevisi teks spec agar tidak saling bertentangan — ketiga teks yang
menyentuh `return` harus selaras:

1. `Language.md` §26 Return — aturan bahasa.
2. `Bytecode.md` instruksi RETURN — aturan bytecode.
3. `ZeroVM.md` §117 Call Stack & Execution Frame — aturan frame.

## Keputusan (user, opsi B)

`return` di top level program = **mengakhiri program**:

- Frame utama program (root frame) dianggap frame aktif terakhir.
- Nilai `return` **diabaikan** (program tidak memiliki pemanggil).
- Statement setelah `return` top level tidak dieksekusi; program selesai
  normal (exit 0; output sebelum `return` tetap dihasilkan).
- Berlaku sama untuk jalur `run()` dan `run_modules` (`zsc file.zs`).

Aturan `return` di dalam fungsi tidak berubah.

## Revisi (jumlah teks minimal, tambahan baru tanpa rewrite paragraf lama)

- `Language.md` §26: tambahkan aturan top-level setelah kalimat
  "`return` hanya menghentikan function tempat ia berada" — selaras karena
  di top level frame "tempat ia berada" = frame utama program.
- `Bytecode.md` (blok Aturan RETURN): bullet pengecualian "tanpa pemanggil".
- `ZeroVM.md` §117 (blok Aturan): bullet aturan top-level.
- `ISSUES.md`: entry §75e baru — Decision (B) + daftar tempat revisi spec.

## Out of Scope

- Perbaikan VM (loop `run_module_range` harus berhenti setelah root frame
  pop, output harus tampil) → **Job 032**.
- CHECK.md candidate-gap row → diperbarui di Job 032 dengan bukti test.

## Testing

- Job 031 = dokumen; verifikasi = baca ulang ketiga teks berdampingan
  (tidak bertentangan satu sama lain).

## Acceptance Criteria

- Tiga teks spec selaras dengan keputusan B; tidak ada paragraf lama yang
  dihapus atau diparafrasekan ulang.
- ISSUES.md mencatat Decision B dan tempat revisi.
- `git diff` hanya menyentuh Language.md, Bytecode.md, ZeroVM.md, ISSUES.md,
  file job ini.

## Required Final Report

- Kutipan aturan final yang ditambahkan per file.
- Daftar file yang diubah.

## Result (2026-10-12)

- Tiga teks selaras (kutipan final):
  - `Language.md` §26: "'return' di top level program (di luar semua
    function) mengakhiri program." + paragraf: frame utama program =
    frame tempat ia berada; nilai diabaikan (tanpa pemanggil); code
    setelahnya tak dieksekusi; selesai normal.
  - `Bytecode.md` Aturan RETURN: "Jika `RETURN` dijalankan pada frame
    utama program (top level, tanpa pemanggil), nilai `RETURN` diabaikan
    dan program selesai normal."
  - `ZeroVM.md` §117 Aturan: bullet `return` top-level mengakhiri program;
    nilai diabaikan; eksekusi berhenti pada titik return.
- `ISSUES.md` §75e dibuat (Decision B) → di-Resolved di Job 032.
- File diubah: Language.md, Bytecode.md, ZeroVM.md, ISSUES.md (semua
  gitignored di repo ini — per `.gitignore`, bukan kelalaian).
- Lanjut: Job 032 (implementasi) — selesai, suite 968/0.
