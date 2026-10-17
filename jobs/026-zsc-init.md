# Job 026 — `zsc init` interactive scaffold (ISSUES §74)

## Objective

`zsc init` = scaffold project ala `npm init` (ISSUES §74a lalu §74b —
user correction 2026-10-08): tanya interaktif name/version/description/
author/license/keywords/entry script; tulis `project.zson` (§82 core +
field opsional §96-style + keywords array, arahan user) + template entry
script default `main.zs` (struktur contoh §82). Spec tak menyebut command
init — semua keputusan di ISSUES §74a/§74b.

## Scope

- src/main.rs: `cmd_init` (prompt loop via stdin, EOF = semua default) +
  dispatch `init` + baris usage (`./init` konvensi §158).
- tests/zero026_zsc_init.rs.
- ISSUES §74a/§74b, CHECK.md §12 baris manifest (satu kalimat).

## Out of Scope

`zpm`/`package.zson` (publish flow §94 tetap), README/.gitignore, konfirmasi
akhir ala npm ("Is this ok?"), flags non-interaktif (--name dst.).

## Testing

1. EOF stdin → semua default: name = folder, version 0.1.0, metadata
   opsional absen, dependencies kosong, main.zs hello template terbuat.
2. Jawaban interaktif penuh → semua field masuk; keywords jadi array
   (trim + buang kosong); entry custom `src/start.zs` terbuat berparent.
3. project.zson sudah ada → exit 1, file tak disentuh.
4. main.zs sudah ada → tidak ditimpa, manifest tetap ditulis.
5. Argumen tambahan → usage exit 1.
6. File bernama `init` → `./init` jalankan file; `init` tetap subcommand.

## Acceptance

Suite 938/0 → 0 failed; warning tetap 2; manifest selalu lolos
`parse_manifest` (self-check sebelum tulis + diuji test).
