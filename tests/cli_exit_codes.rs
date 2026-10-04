//! Exit-code conventions transcribed into Dependencies.md §100
//! ("Nama Binary dan Kode Keluar", ISSUES.md §66l follow-up): `zsc` and
//! `zpm` exit 0 on success and 1 on failure.

use std::process::Command;

#[test]
fn zsc_missing_file_exits_1() {
    let out = Command::new(env!("CARGO_BIN_EXE_zsc"))
        .arg("/nonexistent/definitely_missing_script.zs")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn zsc_version_exits_0() {
    let out = Command::new(env!("CARGO_BIN_EXE_zsc"))
        .arg("--version")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn zsc_successful_program_exits_0() {
    let dir = std::env::temp_dir().join(format!(
        "zeroscript_exitcodes_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("main.zs");
    std::fs::write(&script, "print(\"ok\")\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_zsc"))
        .arg(&script)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn zpm_unknown_command_exits_1() {
    let out = Command::new(env!("CARGO_BIN_EXE_zpm"))
        .arg("definitely-not-a-command")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
