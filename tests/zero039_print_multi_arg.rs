//! `print()` / `print.err()` multi-arg (StandardLibrary §44 "Argumen",
//! user decisions 2026-10-14): 0+ arguments — each argument formatted,
//! joined with a single space, written as one line; zero arguments →
//! empty line. Same rule for both functions.

use std::process::Command;

fn run(code: &str) -> (String, String, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_zsc"))
        .arg("-e")
        .arg(code)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn print_joins_arguments_with_single_space() {
    let (stdout, stderr, code) = run("print(1, 2, true)");
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "1 2 true\n");
}

#[test]
fn print_mixed_types_space_joined() {
    let (stdout, stderr, code) = run("print(10, null, true, \"x\")");
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "10 null true x\n");
}

#[test]
fn print_without_arguments_is_empty_line() {
    let (stdout, stderr, code) = run("print()");
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "\n");
}

#[test]
fn print_single_argument_unchanged() {
    let (stdout, stderr, code) = run("print(\"Hello\")\nprint(10)\nprint(true)\nprint(null)");
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "Hello\n10\ntrue\nnull\n");
}

#[test]
fn print_string_arguments_keep_their_own_spaces() {
    let (stdout, stderr, code) = run("print(\"a b\", \"c\")");
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "a b c\n");
}

#[test]
fn print_err_joins_arguments_with_single_space() {
    let (stdout, stderr, code) = run("print.err(\"gagal:\", 404)");
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "");
    assert_eq!(stderr, "gagal: 404\n");
}

#[test]
fn print_err_caught_error_message_then_extra_argument() {
    let (stdout, stderr, code) = run(
        "try {\n    let A = [10]\n    print(A[99])\n} catch e {\n    print.err(e, \"!\")\n}",
    );
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "");
    // §59a: the error object prints as its message (not the object dump),
    // then the space-joined extra argument per §44.
    assert_eq!(stderr, "List index 99 out of bounds (length 1) !\n");
}

#[test]
fn print_err_without_arguments_is_empty_error_line() {
    let (_, stderr, code) = run("print.err()");
    assert_eq!(code, 0);
    assert_eq!(stderr, "\n");
}
