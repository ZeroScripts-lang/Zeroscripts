//! Job 025 (§100 "Mode Evaluasi", ISSUES §72a): `zsc -e` / `zsc --eval`.
//!
//! Exit-code conventions follow Dependencies.md §100 (0 success, 1 failure).
//! Import base for the eval entry is the current working directory (virtual
//! file `<cwd>/__zsc_eval__.zs`, §72a butir 4); file programs (§64) are
//! unaffected.

use std::path::PathBuf;
use std::process::Command;

fn zsc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_zsc"))
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zs025_{}_{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn test_zsc_eval_runs_code() {
    let out = zsc().args(["-e", "print(1 + 2)"]).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "3\n");
}

#[test]
fn test_zsc_eval_alias_long_flag() {
    let out = zsc().args(["--eval", "print(\"hi\")"]).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hi\n");
}

#[test]
fn test_zsc_eval_missing_code_exits_1() {
    for flag in ["-e", "--eval"] {
        let out = zsc().arg(flag).output().unwrap();
        assert_eq!(out.status.code(), Some(1), "flag {}", flag);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("usage"), "stderr: {}", stderr);
    }
}

#[test]
fn test_zsc_eval_empty_code_is_valid() {
    let out = zsc().args(["-e", ""]).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "");
}

/// §100 example: `system.args()` = the arguments after the code (§52).
#[test]
fn test_zsc_eval_program_args_after_code() {
    let code = "let a = system.args()\nprint(a[0])\nprint(a[1])\n";
    let out = zsc().args(["-e", code, "x", "y"]).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "x\ny\n");
}

/// §72a butir 1: the flag is recognized only as the first argument; a second
/// `-e` is program data, never re-parsed.
#[test]
fn test_zsc_eval_flags_not_reparsed() {
    let code = "let a = system.args()\nprint(a[0])\n";
    let out = zsc().args(["-e", code, "-e", "x"]).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "-e\n");
}

/// §72a butir 4 / §64: relative imports resolve from the current working
/// directory (eval entry = virtual file at cwd).
#[test]
fn test_zsc_eval_import_resolves_from_cwd() {
    let dir = temp_dir("import_cwd");
    std::fs::write(
        dir.join("lib.zs"),
        "export function f() {\n    return 42\n}\n",
    )
    .unwrap();
    let code = "import { f } from \"./lib\"\nprint(f())\n";
    let out = zsc().current_dir(&dir).args(["-e", code]).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "42\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_zsc_eval_syntax_error_exits_1() {
    let out = zsc().args(["-e", "print("]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(!out.stderr.is_empty());
    assert!(out.stdout.is_empty());
}

#[test]
fn test_zsc_eval_runtime_error_exits_1() {
    let out = zsc().args(["-e", "let a = [1]\nprint(a[9])"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(!out.stderr.is_empty());
}

/// A file literally named `-e` is shadowed by the flag; run it as `./-e`
/// (same convention as `./compile` / `./run`, §158 / ISSUES §72a follow-up).
#[test]
fn test_zsc_eval_file_named_dash_e_runs_via_dot_slash() {
    let dir = temp_dir("dash_e_file");
    std::fs::write(dir.join("-e"), "print(\"file mode\")\n").unwrap();

    let shadowed = zsc().arg("-e").output().unwrap();
    assert_eq!(shadowed.status.code(), Some(1));

    let file_mode = zsc().arg("./-e").current_dir(&dir).output().unwrap();
    assert_eq!(
        file_mode.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&file_mode.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&file_mode.stdout), "file mode\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_zsc_eval_help_lists_dash_e_and_file_mode_unchanged() {
    let help = zsc().arg("--help").output().unwrap();
    assert_eq!(help.status.code(), Some(0));
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(text.contains("-e <code>"), "help: {}", text);
    assert!(text.contains("compile"), "help: {}", text);
    assert!(text.contains("run <file.zsbc>"), "help: {}", text);

    let dir = temp_dir("file_mode");
    std::fs::write(dir.join("main.zs"), "print(\"file\")\n").unwrap();
    let run = zsc().arg(dir.join("main.zs")).output().unwrap();
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "file\n");
    let _ = std::fs::remove_dir_all(&dir);
}
