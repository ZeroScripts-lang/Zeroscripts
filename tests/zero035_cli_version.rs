//! `--version` output (user request 2026-10-12): `zsc --version` prints
//! `zsc v0.1.0` (the binary's own identity — not "ZeroScript VM", which
//! wrongly described the compiler as a VM); `zpm --version` prints
//! `zpm v0.1.0`. Spec does not define the exact text (mechanical choice).

use std::process::Command;

#[test]
fn zsc_version_prints_binary_name() {
    let out = Command::new(env!("CARGO_BIN_EXE_zsc"))
        .arg("--version")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "zsc v0.1.0\n");
}

#[test]
fn zpm_version_prints_binary_name() {
    let out = Command::new(env!("CARGO_BIN_EXE_zpm"))
        .arg("--version")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "zpm v0.1.0\n");
}
