//! `input()` (StandardLibrary §157, I/O boundary §148): reads one line from
//! stdin; optional string prompt written to stdout exactly as given (flushed
//! before the blocking read); CRLF normalized; empty line → ""; EOF → null;
//! more than one argument / non-string argument → catchable ERROR; I/O
//! failure → catchable runtime error.

use std::io::Write;
use std::process::{Command, Stdio};

fn run_with_stdin(code: &str, stdin: &[u8]) -> (String, String, i32) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_zsc"))
        .arg("-e")
        .arg(code)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    let out = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn input_reads_line_without_prompt() {
    let (stdout, stderr, code) = run_with_stdin("print(input())", b"hello\n");
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "hello\n");
}

#[test]
fn input_prompt_printed_exactly_then_line() {
    let (stdout, stderr, code) = run_with_stdin(
        "let n = input(\"Nama: \")\nprint(n)",
        b"Budi\n",
    );
    assert_eq!(code, 0, "{}", stderr);
    // Prompt appears verbatim (no auto space/newline) before the line.
    assert_eq!(stdout, "Nama: Budi\n");
}

#[test]
fn input_eof_returns_null() {
    let (stdout, stderr, code) = run_with_stdin("print(input() == null)", b"");
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "true\n");
}

#[test]
fn input_empty_line_is_empty_string_not_null() {
    let (stdout, stderr, code) = run_with_stdin(
        "let a = input()\nprint(a == \"\")\nprint(a == null)\nprint(input() == null)",
        b"\n",
    );
    assert_eq!(code, 0, "{}", stderr);
    // First read: Enter only → ""; second read: stdin exhausted → null.
    assert_eq!(stdout, "true\nfalse\ntrue\n");
}

#[test]
fn input_crlf_normalized() {
    let (stdout, stderr, code) = run_with_stdin(
        "let a = input()\nprint(a == \"x\")\nprint(a)",
        b"x\r\n",
    );
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "true\nx\n");
}

#[test]
fn input_partial_line_at_eof_is_that_line() {
    // read_line returns the bytes seen before EOF; §157 defines EOF (zero
    // bytes) as null — a partial line without newline is still a line.
    let (stdout, stderr, code) = run_with_stdin("print(input())", b"abc");
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "abc\n");
}

#[test]
fn input_extra_argument_is_catchable_argument_error() {
    let (stdout, stderr, code) = run_with_stdin(
        "try {\n    input(\"a\", \"b\")\n} catch e {\n    print(e.type)\n}",
        b"",
    );
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "ArgumentError\n");
}

#[test]
fn input_non_string_argument_is_catchable_type_error() {
    let (stdout, stderr, code) = run_with_stdin(
        "try {\n    input(42)\n} catch e {\n    print(e.type)\n    print(e.message)\n}",
        b"",
    );
    assert_eq!(code, 0, "{}", stderr);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "TypeError");
    assert!(lines[1].contains("input()"), "{}", lines[1]);
}

#[test]
fn input_uncaught_arity_error_exits_1() {
    let (stdout, _stderr, code) = run_with_stdin("input(1, 2)", b"");
    assert_eq!(code, 1);
    assert_eq!(stdout, "");
}

#[test]
fn input_through_dynamic_builtin_value() {
    // The dynamic BuiltIn path (Value::BuiltIn → execute_builtin →
    // call_native) behaves identically to the static CALL_NATIVE path.
    let (stdout, stderr, code) = run_with_stdin(
        "let f = input\nprint(f())",
        b"dyn\n",
    );
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "dyn\n");
}
