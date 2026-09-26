//! Job 017 — end-to-end CLI tests for `zpm` (§85 install, §86 run scripts)
//! and the `zsc` command those scripts invoke.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQ: AtomicUsize = AtomicUsize::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "zeroscript_job017_cli_{}_{}_{}",
        tag,
        std::process::id(),
        n
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

fn zsc_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_zsc"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn with_zsc_on_path(cmd: &mut Command) -> &mut Command {
    let path = std::env::var("PATH").unwrap_or_default();
    cmd.env("PATH", format!("{}:{}", zsc_dir().display(), path));
    cmd
}

fn zpm(cwd: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_zpm"));
    c.current_dir(cwd);
    c
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A local registry with `http-utils` 1.2.0, 1.2.10 and 1.3.2.
fn make_registry(root: &Path) {
    for (v, body) in [
        ("1.2.0", "\"ok-old\""),
        ("1.2.10", "\"ok\""),
        ("1.3.2", "\"ok-next\""),
    ] {
        let pkg = root.join("http-utils").join(v);
        write(
            &pkg.join("package.zson"),
            &format!(
                r#"{{ name: "http-utils", version: "{}", dependencies: {{}} }}"#,
                v
            ),
        );
        write(
            &pkg.join("client.zs"),
            &format!("export function request() {{ return {} }}", body),
        );
    }
}

// §85: zpm install → .zsc_m/ + project.lock.zson, newest matching selector.
#[test]
fn cli_install_writes_zsc_m_and_lockfile() {
    let proj = temp_dir("install");
    let reg = temp_dir("install_reg");
    make_registry(&reg);

    write(
        &proj.join("project.zson"),
        r#"{
    name: "my-app",
    version: "1.0.0",
    dependencies: {
        "http-utils": "1.2.x"
    }
}"#,
    );

    let out = zpm(&proj)
        .arg("--registry")
        .arg(&reg)
        .arg("install")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).contains("+ http-utils@1.2.10"),
        "stdout: {}",
        stdout(&out)
    );
    assert!(proj.join(".zsc_m/http-utils/client.zs").is_file());
    let lock = std::fs::read_to_string(proj.join("project.lock.zson")).unwrap();
    assert!(lock.contains("\"http-utils\": \"1.2.10\""), "{}", lock);
    let _ = std::fs::remove_dir_all(&proj);
    let _ = std::fs::remove_dir_all(&reg);
}

// §85: a valid lockfile pins the version — no auto-upgrade.
#[test]
fn cli_install_honours_existing_lockfile() {
    let proj = temp_dir("lock");
    let reg = temp_dir("lock_reg");
    make_registry(&reg);

    write(
        &proj.join("project.zson"),
        r#"{
    name: "my-app",
    version: "1.0.0",
    dependencies: {
        "http-utils": "1.2.x"
    }
}"#,
    );
    write(
        &proj.join("project.lock.zson"),
        r#"{
    packages: {
        "http-utils": "1.2.0"
    }
}"#,
    );

    let out = zpm(&proj)
        .arg("--registry")
        .arg(&reg)
        .arg("install")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("http-utils@1.2.0"), "{}", stdout(&out));
    let lock = std::fs::read_to_string(proj.join("project.lock.zson")).unwrap();
    assert!(lock.contains("\"1.2.0\""), "{}", lock);
    let pkg = std::fs::read_to_string(proj.join(".zsc_m/http-utils/package.zson")).unwrap();
    assert!(pkg.contains("\"1.2.0\""), "{}", pkg);
    let _ = std::fs::remove_dir_all(&proj);
    let _ = std::fs::remove_dir_all(&reg);
}

// §85: package tidak ditemukan → ERROR (exit code non-zero).
#[test]
fn cli_install_missing_package_is_error() {
    let proj = temp_dir("missing");
    let reg = temp_dir("missing_reg");
    std::fs::create_dir_all(&reg).unwrap();

    write(
        &proj.join("project.zson"),
        r#"{
    name: "my-app",
    version: "1.0.0",
    dependencies: {
        "no-such-package": "x"
    }
}"#,
    );

    let out = zpm(&proj)
        .arg("--registry")
        .arg(&reg)
        .arg("install")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(stderr(&out).contains("ERROR"), "{}", stderr(&out));
    assert!(stderr(&out).contains("no-such-package"), "{}", stderr(&out));
    assert!(!proj.join(".zsc_m/no-such-package").exists());
    let _ = std::fs::remove_dir_all(&proj);
    let _ = std::fs::remove_dir_all(&reg);
}

// §86: `zpm run <name>` executes the command from the project root.
#[test]
fn cli_run_script_executes_from_project_root() {
    let proj = temp_dir("run");
    write(
        &proj.join("project.zson"),
        r#"{
    name: "my-app",
    version: "1.0.0",
    scripts: {
        "hello": "echo hello > out.txt"
    },
    dependencies: {}
}"#,
    );
    write(&proj.join("sub/keep.zs"), "// keeps the directory tree");

    let out = zpm(&proj).args(["run", "hello"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        std::fs::read_to_string(proj.join("out.txt")).unwrap(),
        "hello\n"
    );
    let _ = std::fs::remove_dir_all(&proj);
}

// §86: extra arguments after `--` are forwarded to the script command.
#[test]
fn cli_run_script_forwards_extra_args() {
    let proj = temp_dir("runargs");
    write(
        &proj.join("project.zson"),
        r#"{
    name: "my-app",
    version: "1.0.0",
    scripts: {
        "args": "printf '%s\n' > args.txt"
    },
    dependencies: {}
}"#,
    );

    let out = zpm(&proj)
        .args(["run", "args", "--", "--debug", "x y"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        std::fs::read_to_string(proj.join("args.txt")).unwrap(),
        "--debug\nx y\n"
    );
    let _ = std::fs::remove_dir_all(&proj);
}

// §86: script tidak ditemukan → ERROR (non-zero exit).
#[test]
fn cli_run_unknown_script_is_error() {
    let proj = temp_dir("runmissing");
    write(
        &proj.join("project.zson"),
        r#"{
    name: "my-app",
    version: "1.0.0",
    scripts: {
        "start": "echo start"
    },
    dependencies: {}
}"#,
    );

    let out = zpm(&proj).args(["run", "nope"]).output().unwrap();
    assert!(!out.status.success());
    assert!(stderr(&out).contains("ERROR"), "{}", stderr(&out));
    assert!(stderr(&out).contains("nope"), "{}", stderr(&out));
    let _ = std::fs::remove_dir_all(&proj);
}

// §86 + §87: `zpm run start` → `zsc index.zs`, which imports the installed
// package from .zsc_m/ (the script command itself is never rewritten).
#[test]
fn cli_run_script_then_package_import_works() {
    let proj = temp_dir("e2e");
    let reg = temp_dir("e2e_reg");
    make_registry(&reg);

    write(
        &proj.join("project.zson"),
        r#"{
    name: "my-app",
    version: "1.0.0",
    scripts: {
        start: "zsc index.zs"
    },
    dependencies: {
        "http-utils": "1.2.x"
    }
}"#,
    );
    write(
        &proj.join("index.zs"),
        r#"import { request } from "http-utils/client"
print(request())"#,
    );

    let install = zpm(&proj)
        .arg("--registry")
        .arg(&reg)
        .arg("install")
        .output()
        .unwrap();
    assert!(install.status.success(), "{}", stderr(&install));

    let mut run = zpm(&proj);
    with_zsc_on_path(&mut run);
    let out = run.args(["run", "start"]).output().unwrap();
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "ok");

    // Running `zsc` directly (outside zpm) resolves the same way.
    let direct = Command::new(env!("CARGO_BIN_EXE_zsc"))
        .current_dir(&proj)
        .arg("index.zs")
        .output()
        .unwrap();
    assert!(direct.status.success(), "stderr: {}", stderr(&direct));
    assert_eq!(stdout(&direct).trim(), "ok");

    let _ = std::fs::remove_dir_all(&proj);
    let _ = std::fs::remove_dir_all(&reg);
}

// §87: an installed package that is not declared as a dependency → ERROR.
#[test]
fn cli_run_rejects_undeclared_package_import() {
    let proj = temp_dir("undeclared");
    write(
        &proj.join("project.zson"),
        r#"{
    name: "my-app",
    version: "1.0.0",
    dependencies: {}
}"#,
    );
    write(&proj.join("index.zs"), "import { x } from \"pkg\"\nprint(x)");

    let out = Command::new(env!("CARGO_BIN_EXE_zsc"))
        .current_dir(&proj)
        .arg("index.zs")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("ERROR"), "{}", err);
    assert!(err.contains("is not a dependency"), "{}", err);
    let _ = std::fs::remove_dir_all(&proj);
}

// §85: running zpm outside a project reports a clear error.
#[test]
fn cli_no_project_is_error() {
    let dir = temp_dir("noproject");
    let out = zpm(&dir).arg("install").output().unwrap();
    assert!(!out.status.success());
    assert!(stderr(&out).contains("project.zson"), "{}", stderr(&out));
    let _ = std::fs::remove_dir_all(&dir);
}

// §52 via the CLI: `zsc app.zs a b` → system.args() == ["a", "b"]
// (interpreter and script name excluded).
#[test]
fn cli_zsc_passes_program_arguments() {
    let proj = temp_dir("args");
    write(
        &proj.join("index.zs"),
        "let a = system.args()\nprint(list.length(a))\nprint(a[0])\nprint(a[1])",
    );

    let out = Command::new(env!("CARGO_BIN_EXE_zsc"))
        .current_dir(&proj)
        .args(["index.zs", "hello", "world"])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out), "2\nhello\nworld\n");
    let _ = std::fs::remove_dir_all(&proj);
}

// Smoke: usage / help text names the implemented commands.
#[test]
fn cli_help_lists_commands() {
    let dir = temp_dir("help");
    let out = zpm(&dir).arg("--help").output().unwrap();
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("install"), "{}", text);
    assert!(text.contains("run"), "{}", text);
    let _ = std::fs::remove_dir_all(&dir);
}
