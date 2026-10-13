//! Flag execution limits (Dependencies.md §100 "Flag Execution Limits",
//! ISSUES §71h, job 033/034): five `--name=<value>` flags recognized only
//! before the code/mode argument; invalid or 0 value = usage error exit 1;
//! a breached limit = "Execution limit exceeded" on stderr, exit 1.

use std::process::Command;

fn zsc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_zsc"))
}

fn write_script(name: &str, body: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("zeroscript_limits_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn max_instructions_breached_is_error() {
    let script = write_script("inf.zs", "while true { }\n");
    let out = zsc().arg("--max-instructions=100").arg(&script).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Execution limit exceeded: instruction count"), "stderr: {}", err);
}

#[test]
fn time_limit_breached_is_error() {
    let out = zsc().arg("--time-limit=50").arg("-e").arg("while true { }").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Execution limit exceeded: wall clock time"), "stderr: {}", err);
}

#[test]
fn max_call_depth_breached_is_error() {
    let out = zsc()
        .arg("--max-call-depth=10")
        .arg("-e")
        .arg("function f(n) {\n    return f(n + 1)\n}\nf(0)")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Execution limit exceeded: call stack depth"), "stderr: {}", err);
}

#[test]
fn invalid_value_is_usage_error() {
    let out = zsc().arg("--max-instructions=abc").arg("-e").arg("print(1)").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("invalid value"), "stderr: {}", err);
}

#[test]
fn zero_value_is_usage_error() {
    let out = zsc().arg("--max-tasks=0").arg("-e").arg("print(1)").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("greater than 0"), "stderr: {}", err);
}

#[test]
fn flags_after_code_are_program_args() {
    let script = write_script("args.zs", "print(system.args())\n");
    let out = zsc()
        .arg(&script)
        .arg("--max-instructions=5")
        .arg("--time-limit=9")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout, "[--max-instructions=5, --time-limit=9]\n");
}

#[test]
fn flags_apply_in_file_mode() {
    let script = write_script("f_ok.zs", "print(\"ok\")\n");
    let out = zsc().arg("--max-instructions=10000").arg(&script).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "ok\n");
}

#[test]
fn flags_apply_to_eval_and_bytecode_run() {
    // eval mode
    let out = zsc().arg("--max-instructions=50").arg("-e").arg("while true { }").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    // bytecode mode: compile then run with a limit
    let script = write_script("bc.zs", "while true { }\n");
    let out = zsc().arg("compile").arg(&script).arg("-o").arg(
        &std::env::temp_dir().join(format!("zs_limits_{}.zsbc", std::process::id())),
    ).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "compile: {:?}", out);
    let zsb = std::env::temp_dir().join(format!("zs_limits_{}.zsbc", std::process::id()));
    let out = zsc().arg("--max-instructions=100").arg("run").arg(&zbc_path(&zsb)).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Execution limit exceeded: instruction count"), "stderr: {}", err);
}

fn zbc_path(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}
