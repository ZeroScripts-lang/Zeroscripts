//! Top-level `return` per ISSUES §75e / keputusan user opsi B (2026-10-12):
//! `Language.md` §26 — top-level return mengakhiri program, nilai diabaikan,
//! exit 0, output sebelum return tetap tampil, code setelahnya tidak jalan.

use std::process::Command;

#[test]
fn zsc_top_level_return_exits_0_with_output() {
    let dir = std::env::temp_dir().join(format!("zeroscript_tlr_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("main.zs");
    std::fs::write(&script, "print(\"a\")\nreturn 42\nprint(\"b\")\n").unwrap();
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
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout, "a\n", "stdout: {:?}", stdout);
}

#[test]
fn zsc_eval_top_level_return_exits_0() {
    let out = Command::new(env!("CARGO_BIN_EXE_zsc"))
        .arg("-e")
        .arg("print(\"a\")\nreturn 42\nprint(\"b\")")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout, "a\n");
}
