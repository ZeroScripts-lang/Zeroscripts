# Job 035 — Output `--version` zsc/zpm

## Keputusan (user, 2026-10-12)
- `zsc --version` sebelumnya mencetak "ZeroScript VM v0.1.0" — keliru, zsc
  adalah compiler/runner, bukan VM. Diubah menjadi `zsc v0.1.0`.
- `zpm` sebelumnya tidak memiliki `--version`; ditambah `--version`/`-V`
  mencetak `zpm v0.1.0`.
- Teks versi tidak diatur spec → pilihan mekanis (nama binary + versi).

## Files
`src/main.rs` (const VERSION), `src/bin/zpm.rs` (VERSION + dispatch arm),
`tests/zero035_cli_version.rs` (2 tests).

## Tests
Suite → 978 passed / 0 failed.
