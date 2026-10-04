//! Job 018 — end-to-end `zpm` CLI tests: §91 remove, §92 update, §93 git
//! install, §94 publish, §95/§100 install-by-name, §100 command surface.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQ: AtomicUsize = AtomicUsize::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "zeroscript_job018_cli_{}_{}_{}",
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

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn zpm(cwd: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_zpm"));
    c.current_dir(cwd);
    // §98: point the cache at the test's own directory so no test reads
    // the shared $HOME cache.
    c.env("ZPM_CACHE", cwd.join(".zsc_cache"));
    c
}

/// A local registry package (`<registry>/<name>/<version>/`, §95).
fn add_pkg(registry: &Path, name: &str, version: &str, deps: &[(&str, &str)]) {
    let dir = registry.join(name).join(version);
    std::fs::create_dir_all(&dir).unwrap();
    let mut dep_body = String::new();
    for (i, (n, s)) in deps.iter().enumerate() {
        if i > 0 {
            dep_body.push_str(", ");
        }
        dep_body.push_str(&format!("\"{}\": \"{}\"", n, s));
    }
    write(
        &dir.join("package.zson"),
        &format!(
            "{{ name: \"{}\", version: \"{}\", dependencies: {{ {} }} }}",
            name, version, dep_body
        ),
    );
    write(&dir.join("mod.zs"), "export const X = 1");
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `git init` + commit + `v<version>` tag with an inline identity.
fn make_git_package(dir: &Path, name: &str, version: &str) {
    write(
        &dir.join("package.zson"),
        &format!(
            r#"{{ name: "{}", version: "{}", dependencies: {{}} }}"#,
            name, version
        ),
    );
    write(&dir.join("index.zs"), "export const X = 1");
    git(dir, &["init", "--quiet"]);
    git(dir, &["add", "-A"]);
    git(
        dir,
        &[
            "-c",
            "user.name=ZeroScript",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "init",
        ],
    );
    git(dir, &["tag", &format!("v{}", version)]);
}

// §91: `zpm remove` drops the direct dependency, keeps what others still
// need, updates project.zson and project.lock.zson, never touches sources.
#[test]
fn cli_remove_updates_manifest_and_prunes() {
    let proj = temp_dir("remove");
    let reg = temp_dir("remove_reg");
    add_pkg(&reg, "package-a", "1.0.0", &[("shared-utils", "1.x")]);
    add_pkg(&reg, "package-b", "1.0.0", &[("shared-utils", "1.x")]);
    add_pkg(&reg, "shared-utils", "1.0.0", &[]);
    write(
        &proj.join("project.zson"),
        r#"{ name: "my-app", version: "1.0.0", dependencies: { "package-a": "1.x", "package-b": "1.x" } }"#,
    );
    write(
        &proj.join("main.zs"),
        "import { x } from \"package-a\"\nprint(x)",
    );

    let out = zpm(&proj)
        .arg("--registry")
        .arg(&reg)
        .arg("install")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(proj.join(".zsc_m/package-b").exists());

    let out = zpm(&proj)
        .arg("--registry")
        .arg(&reg)
        .args(["remove", "package-b"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));

    // §91: removed from the manifest and the lockfile …
    let manifest = read(&proj.join("project.zson"));
    assert!(!manifest.contains("package-b"), "{}", manifest);
    assert!(manifest.contains("package-a"), "{}", manifest);
    let lock = read(&proj.join("project.lock.zson"));
    assert!(!lock.contains("package-b"), "{}", lock);
    assert!(lock.contains("package-a"), "{}", lock);
    // … gone from .zsc_m/ itself …
    assert!(!proj.join(".zsc_m/package-b").exists());
    // … but shared-utils stays: package-a still depends on it (§91).
    assert!(proj.join(".zsc_m/package-a").exists());
    assert!(proj.join(".zsc_m/shared-utils").exists());
    // §91: source code is never removed.
    assert!(proj.join("main.zs").is_file());

    let _ = std::fs::remove_dir_all(&proj);
    let _ = std::fs::remove_dir_all(&reg);
}

// §91: removing a package that is not a direct project dependency → ERROR.
#[test]
fn cli_remove_undeclared_is_error() {
    let proj = temp_dir("remove_ghost");
    let reg = temp_dir("remove_ghost_reg");
    write(
        &proj.join("project.zson"),
        r#"{ name: "my-app", version: "1.0.0", dependencies: {} }"#,
    );
    let out = zpm(&proj)
        .arg("--registry")
        .arg(&reg)
        .args(["remove", "ghost"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("not a direct dependency"),
        "{}",
        stderr(&out)
    );
    let _ = std::fs::remove_dir_all(&proj);
}

// §92: install never auto-upgrades a locked version; `zpm update` does,
// staying within the selector, and rewrites project.lock.zson.
#[test]
fn cli_update_bumps_locked_version() {
    let proj = temp_dir("update");
    let reg = temp_dir("update_reg");
    add_pkg(&reg, "http-utils", "1.2.0", &[]);
    write(
        &proj.join("project.zson"),
        r#"{ name: "my-app", version: "1.0.0", dependencies: { "http-utils": "1.2.x" } }"#,
    );

    let out = zpm(&proj)
        .arg("--registry")
        .arg(&reg)
        .arg("install")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(read(&proj.join("project.lock.zson")).contains("1.2.0"));

    // Newer matching versions appear in the registry.
    add_pkg(&reg, "http-utils", "1.2.5", &[]);
    add_pkg(&reg, "http-utils", "1.2.10", &[]);

    // §92: install keeps the locked version …
    let out = zpm(&proj)
        .arg("--registry")
        .arg(&reg)
        .arg("install")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let lock = read(&proj.join("project.lock.zson"));
    assert!(lock.contains("1.2.0"), "{}", lock);
    assert!(!lock.contains("1.2.10"), "{}", lock);

    // … update is the explicit operation that upgrades.
    let out = zpm(&proj)
        .arg("--registry")
        .arg(&reg)
        .arg("update")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).contains("+ http-utils@1.2.10"),
        "stdout: {}",
        stdout(&out)
    );
    let lock = read(&proj.join("project.lock.zson"));
    assert!(lock.contains("1.2.10"), "{}", lock);
    assert!(!lock.contains("1.2.0"), "{}", lock);

    let _ = std::fs::remove_dir_all(&proj);
    let _ = std::fs::remove_dir_all(&reg);
}

// §92: the basic command is `zpm update` — extra arguments are rejected.
#[test]
fn cli_update_with_argument_is_usage_error() {
    let proj = temp_dir("update_arg");
    write(
        &proj.join("project.zson"),
        r#"{ name: "my-app", version: "1.0.0", dependencies: {} }"#,
    );
    let out = zpm(&proj)
        .args(["update", "http-utils"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert_eq!(stderr(&out).trim(), "ERROR: usage: zpm update");
    let _ = std::fs::remove_dir_all(&proj);
}

// §94/§95/§100: publish into the local registry, then another project
// installs the package by name (`zpm install <name>` declares it, §100).
#[test]
fn cli_publish_then_install_by_name() {
    let reg = temp_dir("pub_reg");
    let pkg = temp_dir("pub_src");
    write(
        &pkg.join("package.zson"),
        r#"{ name: "cli-published", version: "1.0.0", description: "published package", dependencies: {} }"#,
    );
    write(&pkg.join("mod.zs"), "export function f() { return 42 }");

    let out = zpm(&pkg)
        .arg("--registry")
        .arg(&reg)
        .arg("publish")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("Published to"), "{}", stdout(&out));
    assert!(reg
        .join("cli-published")
        .join("1.0.0")
        .join("package.zson")
        .is_file());
    assert!(reg
        .join("cli-published")
        .join("1.0.0")
        .join("mod.zs")
        .is_file());

    let proj = temp_dir("pub_consumer");
    write(
        &proj.join("project.zson"),
        r#"{ name: "app", version: "1.0.0", dependencies: {} }"#,
    );
    let out = zpm(&proj)
        .arg("--registry")
        .arg(&reg)
        .args(["install", "cli-published"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let manifest = read(&proj.join("project.zson"));
    assert!(manifest.contains("cli-published"), "{}", manifest);
    assert!(manifest.contains("\"x\""), "{}", manifest);
    assert!(proj
        .join(".zsc_m/cli-published/mod.zs")
        .is_file());
    assert!(
        stdout(&out).contains("+ cli-published@1.0.0"),
        "stdout: {}",
        stdout(&out)
    );

    let _ = std::fs::remove_dir_all(&proj);
    let _ = std::fs::remove_dir_all(&pkg);
    let _ = std::fs::remove_dir_all(&reg);
}

// §95/§100: installing a name no registry knows → ERROR.
#[test]
fn cli_install_named_missing_package_is_error() {
    let reg = temp_dir("missing_reg");
    let proj = temp_dir("missing");
    write(
        &proj.join("project.zson"),
        r#"{ name: "app", version: "1.0.0", dependencies: {} }"#,
    );
    let out = zpm(&proj)
        .arg("--registry")
        .arg(&reg)
        .args(["install", "ghost-pkg"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("not found in any registry"),
        "{}",
        stderr(&out)
    );
    let _ = std::fs::remove_dir_all(&proj);
    let _ = std::fs::remove_dir_all(&reg);
}

// ISSUES.md §67k: a failed `zpm install <name>` leaves project.zson
// byte-identical — the declaration is staged in memory and only committed
// after the whole install succeeds (prepare-then-commit).
#[test]
fn cli_install_named_failure_leaves_manifest_untouched() {
    let reg = temp_dir("manifest_untouched_reg");
    let proj = temp_dir("manifest_untouched");
    write(
        &proj.join("project.zson"),
        r#"{ name: "app", version: "1.0.0", dependencies: {} }"#,
    );
    let before = read(&proj.join("project.zson"));
    let out = zpm(&proj)
        .arg("--registry")
        .arg(&reg)
        .args(["install", "ghost-pkg"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    // §100 "Nama Binary dan Kode Keluar": a failed command exits with 1.
    assert_eq!(out.status.code(), Some(1), "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("not found in any registry"),
        "{}",
        stderr(&out)
    );
    assert_eq!(before, read(&proj.join("project.zson")));
    let _ = std::fs::remove_dir_all(&proj);
    let _ = std::fs::remove_dir_all(&reg);
}

// §86 + §100 (ISSUES.md §66l): `zpm run` forwards the script process's exit
// code as zpm's own exit code; a successful script exits 0.
#[test]
fn cli_run_forwards_child_exit_code() {
    let proj = temp_dir("run_exit_code");
    write(
        &proj.join("project.zson"),
        r#"{ name: "app", version: "1.0.0", scripts: { boom: "exit 3", ok: "true" }, dependencies: {} }"#,
    );
    let out = zpm(&proj).args(["run", "boom"]).output().unwrap();
    assert_eq!(out.status.code(), Some(3), "stderr: {}", stderr(&out));
    let out = zpm(&proj).args(["run", "ok"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "stderr: {}", stderr(&out));
    let _ = std::fs::remove_dir_all(&proj);
}

// §93: `zpm install git:<url>#<ref>` fetches the repository, declares the
// dependency (§100) and records source + commit in the lockfile.
#[test]
fn cli_install_git_source_records_commit() {
    let repo = temp_dir("git_repo");
    make_git_package(&repo, "gitpkg", "1.0.0");
    let proj = temp_dir("git_proj");
    write(
        &proj.join("project.zson"),
        r#"{ name: "app", version: "1.0.0", dependencies: {} }"#,
    );

    let spec = format!("git:{}#v1.0.0", repo.display());
    let out = zpm(&proj).args(["install", &spec]).output().unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(proj.join(".zsc_m/gitpkg/index.zs").is_file());
    let manifest = read(&proj.join("project.zson"));
    assert!(manifest.contains(&spec), "{}", manifest);
    let lock = read(&proj.join("project.lock.zson"));
    assert!(lock.contains("source: \"git:"), "{}", lock);
    assert!(lock.contains("commit: \""), "{}", lock);
    assert!(
        stdout(&out).contains("+ gitpkg@1.0.0"),
        "stdout: {}",
        stdout(&out)
    );

    let _ = std::fs::remove_dir_all(&proj);
    let _ = std::fs::remove_dir_all(&repo);
}

// §100: the command surface of the completed package system.
#[test]
fn cli_help_lists_all_commands() {
    let dir = temp_dir("help18");
    let out = zpm(&dir).arg("--help").output().unwrap();
    assert!(out.status.success());
    let text = stdout(&out);
    for cmd in ["install", "remove", "update", "publish", "run"] {
        assert!(text.contains(cmd), "missing '{}': {}", cmd, text);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
