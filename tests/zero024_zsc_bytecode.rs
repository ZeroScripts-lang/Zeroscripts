//! Job 024 (§158): bytecode serialization CLI — `zsc compile` / `zsc run`.
//!
//! Exit-code conventions follow Dependencies.md §100 (0 success, 1 failure);
//! distribution semantics follow Bytecode.md §158 and Runtime.md §155 (the
//! artifact runs without the original `.zs` source).

use std::path::{Path, PathBuf};
use std::process::Command;

fn zsc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_zsc"))
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zs024_{}_{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const UTIL: &str = "export function greet(name) {\n    return \"hello \" + name\n}\n";
const MAIN: &str =
    "import { greet } from \"./util\"\nprint(greet(\"world\"))\n";

fn write_pair(dir: &Path) -> PathBuf {
    std::fs::write(dir.join("util.zs"), UTIL).unwrap();
    std::fs::write(dir.join("main.zs"), MAIN).unwrap();
    dir.join("main.zs")
}

#[test]
fn test_job024_compile_then_run_matches_direct_run() {
    let dir = temp_dir("roundtrip");
    let main = write_pair(&dir);

    let direct = zsc().arg(&main).output().unwrap();
    assert_eq!(direct.status.code(), Some(0));
    let direct_out = String::from_utf8_lossy(&direct.stdout).to_string();

    let compiled = zsc().arg("compile").arg(&main).output().unwrap();
    assert_eq!(
        compiled.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let artifact = dir.join("main.zsbc");
    assert!(artifact.is_file(), "default .zsbc next to the source");

    let run = zsc().arg("run").arg(&artifact).output().unwrap();
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), direct_out);
    assert_eq!(String::from_utf8_lossy(&run.stdout), "hello world\n");

    let _ = std::fs::remove_dir_all(&dir);
}

/// §155/§158: the artifact is self-contained — running it needs no source.
#[test]
fn test_job024_artifact_runs_without_source() {
    let src_dir = temp_dir("nosource_src");
    let main = write_pair(&src_dir);
    let compiled = zsc().arg("compile").arg(&main).output().unwrap();
    assert_eq!(compiled.status.code(), Some(0));

    let bare_dir = temp_dir("nosource_bare");
    let artifact = bare_dir.join("app.zsbc");
    std::fs::copy(src_dir.join("main.zsbc"), &artifact).unwrap();

    let run = zsc().arg("run").arg(&artifact).output().unwrap();
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "hello world\n");

    let _ = std::fs::remove_dir_all(&src_dir);
    let _ = std::fs::remove_dir_all(&bare_dir);
}

/// §158: compile writes the artifact; side-effects happen only at run time.
#[test]
fn test_job024_compile_does_not_execute() {
    let dir = temp_dir("noexec");
    let script = dir.join("side.zs");
    std::fs::write(&script, "print(\"side-effect\")\n").unwrap();

    let compiled = zsc().arg("compile").arg(&script).output().unwrap();
    assert_eq!(compiled.status.code(), Some(0));
    assert!(
        compiled.stdout.is_empty(),
        "compile must not run the program: {:?}",
        String::from_utf8_lossy(&compiled.stdout)
    );
    assert!(dir.join("side.zsbc").is_file());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_job024_flag_o_overrides_output_path() {
    let src_dir = temp_dir("flago_src");
    let out_dir = temp_dir("flago_out");
    let main = write_pair(&src_dir);
    let dest = out_dir.join("custom.bin");

    let compiled = zsc()
        .arg("compile")
        .arg(&main)
        .arg("-o")
        .arg(&dest)
        .output()
        .unwrap();
    assert_eq!(
        compiled.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(dest.is_file());
    assert!(!src_dir.join("main.zsbc").exists(), "-o replaces the default");

    let run = zsc().arg("run").arg(&dest).output().unwrap();
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "hello world\n");

    let _ = std::fs::remove_dir_all(&src_dir);
    let _ = std::fs::remove_dir_all(&out_dir);
}

/// §52 + §158: `zsc run app.zsbc a b` → system.args() == ["a", "b"].
#[test]
fn test_job024_run_passes_program_args() {
    let dir = temp_dir("args");
    let script = dir.join("args.zs");
    std::fs::write(
        &script,
        "let a = system.args()\nprint(a[0])\nprint(a[1])\n",
    )
    .unwrap();
    let compiled = zsc().arg("compile").arg(&script).output().unwrap();
    assert_eq!(compiled.status.code(), Some(0));

    let run = zsc()
        .arg("run")
        .arg(dir.join("args.zsbc"))
        .arg("x")
        .arg("y")
        .output()
        .unwrap();
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "x\ny\n");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_job024_cli_usage_and_missing_files_exit_1() {
    let dir = temp_dir("usage");

    // compile without a file argument.
    let out = zsc().arg("compile").output().unwrap();
    assert_eq!(out.status.code(), Some(1));

    // compile of a missing source.
    let out = zsc()
        .arg("compile")
        .arg(dir.join("missing.zs"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));

    // run without a file argument.
    let out = zsc().arg("run").output().unwrap();
    assert_eq!(out.status.code(), Some(1));

    // run of a missing artifact.
    let out = zsc().arg("run").arg(dir.join("missing.zsbc")).output().unwrap();
    assert_eq!(out.status.code(), Some(1));

    let _ = std::fs::remove_dir_all(&dir);
}

/// §158: a file that is not a valid bytecode artifact is an ERROR (exit 1),
/// with nothing executed.
#[test]
fn test_job024_run_non_bytecode_file_exits_1() {
    let dir = temp_dir("notbc");
    let bad = dir.join("bad.zsbc");
    std::fs::write(&bad, "this is not bytecode at all").unwrap();

    let out = zsc().arg("run").arg(&bad).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(!out.stderr.is_empty());
    assert!(out.stdout.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

/// §158: checksum failure is caught before anything runs.
#[test]
fn test_job024_run_corrupted_artifact_exits_1() {
    let dir = temp_dir("corrupt");
    let script = dir.join("app.zs");
    std::fs::write(&script, "print(\"ok\")\n").unwrap();
    let compiled = zsc().arg("compile").arg(&script).output().unwrap();
    assert_eq!(compiled.status.code(), Some(0));

    let artifact = dir.join("app.zsbc");
    let mut bytes = std::fs::read(&artifact).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0x01;
    std::fs::write(&artifact, &bytes).unwrap();

    let out = zsc().arg("run").arg(&artifact).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("checksum"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "nothing may execute");

    let _ = std::fs::remove_dir_all(&dir);
}

/// §158 subcommand disambiguation: the literal first argument selects the
/// mode; a file with that name runs as `./run`.
#[test]
fn test_job024_file_named_run_requires_dot_slash() {
    let dir = temp_dir("disambig");
    std::fs::write(dir.join("run"), "print(\"plain\")\n").unwrap();

    let as_subcommand = zsc().current_dir(&dir).arg("run").output().unwrap();
    assert_eq!(
        as_subcommand.status.code(),
        Some(1),
        "`zsc run` without a bytecode argument is a usage error"
    );

    let as_file = zsc().current_dir(&dir).arg("./run").output().unwrap();
    assert_eq!(
        as_file.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&as_file.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&as_file.stdout), "plain\n");

    let _ = std::fs::remove_dir_all(&dir);
}
