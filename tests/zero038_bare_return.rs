//! §26 "Batas statement untuk return" (user decision 2026-10-14, option A):
//! the return value expression is parsed only when it starts on the same
//! line as the `return` keyword — a token on the next line is a new
//! statement, so a bare `return\n<stmt>` no longer executes the next
//! statement as the return value.

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
fn bare_return_does_not_eat_next_statement() {
    let (stdout, stderr, code) = run(
        "function f() {\n    return\n    print(\"x\")\n}\nf()\nprint(\"after\")",
    );
    assert_eq!(code, 0, "{}", stderr);
    // The print after the bare return is a separate statement inside f,
    // never executed (function already returned).
    assert_eq!(stdout, "after\n");
}

#[test]
fn bare_return_value_is_null() {
    let (stdout, stderr, code) = run("function f() {\n    return\n}\nprint(f() == null)");
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "true\n");
}

#[test]
fn top_level_bare_return_skips_following_statement() {
    let (stdout, stderr, code) = run("return\nprint(\"x\")");
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "");
}

#[test]
fn same_line_return_expression_unchanged() {
    let (stdout, stderr, code) = run("function tambah(A, B) {\n    return A + B\n}\nprint(tambah(2, 3))");
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "5\n");
}

#[test]
fn trailing_operator_continues_across_newline() {
    // The expression started on the return line, so it still spans the
    // newline — only the *first* token's line matters.
    let (stdout, stderr, code) = run("function f() {\n    return 1 +\n    2\n}\nprint(f())");
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "3\n");
}

#[test]
fn multiline_list_literal_return_unchanged() {
    let (stdout, stderr, code) = run(
        "function f() {\n    return [\n        1,\n        2\n    ]\n}\nlet A = f()\nprint(A[0])\nprint(A[1])",
    );
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "1\n2\n");
}

#[test]
fn multiline_paren_return_unchanged() {
    let (stdout, stderr, code) = run("function f() {\n    return (\n        1 + 2\n    )\n}\nprint(f())");
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "3\n");
}

#[test]
fn spec_27_recursion_example_unchanged() {
    let (stdout, stderr, code) = run(
        "function count(A) {\n    if A <= 0 {\n        return\n    }\n\n    print(A)\n    return count(A - 1)\n}\ncount(3)",
    );
    assert_eq!(code, 0, "{}", stderr);
    assert_eq!(stdout, "3\n2\n1\n");
}
