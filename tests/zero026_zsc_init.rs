//! Job 026 (ISSUES §74, §82/§86/§96): `zsc init` interactive scaffold.
//!
//! Exit-code conventions follow Dependencies.md §100 (0 success, 1 failure).
//! With no stdin (EOF), init behaves as if every prompt was answered with
//! its default.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use zeroscript::json::{self, JsonValue};
use zeroscript::package;

fn zsc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_zsc"))
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zs026_{}_{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn init_with_answers(dir: &std::path::Path, answers: &str) -> std::process::Output {
    let mut child = zsc()
        .arg("init")
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(answers.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn manifest_object(dir: &std::path::Path) -> JsonValue {
    let text = std::fs::read_to_string(dir.join("project.zson")).unwrap();
    // The manifest must always pass the §82 parser.
    package::parse_manifest(&text, "project.zson").unwrap();
    json::parse_zson(&text).unwrap()
}

fn field_string(v: &JsonValue, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|f| f.as_str())
        .map(|s| s.to_string())
}

/// EOF on stdin = every prompt takes its default: name = folder,
/// version 0.1.0, optional metadata omitted, entry = main.zs (created).
#[test]
fn test_zsc_init_eof_defaults_scaffold() {
    let dir = temp_dir("defaults");
    let out = zsc().arg("init").current_dir(&dir).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let manifest = manifest_object(&dir);
    assert_eq!(
        field_string(&manifest, "name").as_deref(),
        Some(dir.file_name().unwrap().to_str().unwrap())
    );
    assert_eq!(field_string(&manifest, "version").as_deref(), Some("0.1.0"));
    assert!(manifest.get("description").is_none());
    assert!(manifest.get("author").is_none());
    assert!(manifest.get("license").is_none());
    assert!(manifest.get("keywords").is_none());
    assert!(manifest
        .get("dependencies")
        .and_then(|d| d.as_object())
        .unwrap()
        .is_empty());
    match manifest.get("scripts").and_then(|s| s.as_object()) {
        Some(scripts) => assert_eq!(
            scripts.iter().find(|(k, _)| k == "start").map(|(_, v)| v.as_str()),
            Some(Some("zsc main.zs"))
        ),
        other => panic!("scripts object expected, got {:?}", other),
    }

    let entry = std::fs::read_to_string(dir.join("main.zs")).unwrap();
    assert!(entry.contains("print("), "hello template: {}", entry);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Interactive answers: custom metadata lands in the manifest, keywords
/// become a string array, custom entry script gets the template.
#[test]
fn test_zsc_init_interactive_answers() {
    let dir = temp_dir("interactive");
    let out = init_with_answers(
        &dir,
        "cool-app\n2.5.0\nA cool app\nAda Lovelace\nMIT\nalpha, beta , ,gamma\nsrc/start.zs\n",
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Project name"), "prompts shown: {}", stdout);

    let manifest = manifest_object(&dir);
    assert_eq!(field_string(&manifest, "name").as_deref(), Some("cool-app"));
    assert_eq!(field_string(&manifest, "version").as_deref(), Some("2.5.0"));
    assert_eq!(
        field_string(&manifest, "description").as_deref(),
        Some("A cool app")
    );
    assert_eq!(field_string(&manifest, "author").as_deref(), Some("Ada Lovelace"));
    assert_eq!(field_string(&manifest, "license").as_deref(), Some("MIT"));
    match manifest.get("keywords") {
        Some(JsonValue::Array(items)) => {
            let words: Vec<&str> = items.iter().filter_map(|i| i.as_str()).collect();
            assert_eq!(words, ["alpha", "beta", "gamma"]);
        }
        other => panic!("keywords array expected, got {:?}", other),
    }
    match manifest.get("scripts").and_then(|s| s.as_object()) {
        Some(scripts) => assert_eq!(
            scripts
                .iter()
                .find(|(k, _)| k == "start")
                .map(|(_, v)| v.as_str()),
            Some(Some("zsc src/start.zs")),
            "start script follows the custom entry"
        ),
        other => panic!("scripts object expected, got {:?}", other),
    }

    let entry = dir.join("src/start.zs");
    assert!(entry.is_file(), "custom entry created");
    assert!(std::fs::read_to_string(&entry)
        .unwrap()
        .contains("cool-app"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_zsc_init_refuses_existing_manifest() {
    let dir = temp_dir("exists");
    let first = zsc().arg("init").current_dir(&dir).output().unwrap();
    assert_eq!(first.status.code(), Some(0));
    let original = std::fs::read_to_string(dir.join("project.zson")).unwrap();

    let second = zsc().arg("init").current_dir(&dir).output().unwrap();
    assert_eq!(second.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(stderr.contains("already exists"), "stderr: {}", stderr);
    assert_eq!(
        std::fs::read_to_string(dir.join("project.zson")).unwrap(),
        original,
        "existing file untouched"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// An existing entry script is never overwritten — only the manifest is
/// written (the manifest did not exist, so init proceeds).
#[test]
fn test_zsc_init_does_not_overwrite_entry_script() {
    let dir = temp_dir("entry_exists");
    std::fs::write(dir.join("main.zs"), "print(\"keep me\")\n").unwrap();

    let out = zsc().arg("init").current_dir(&dir).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("main.zs")).unwrap(),
        "print(\"keep me\")\n"
    );
    assert!(dir.join("project.zson").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_zsc_init_extra_args_is_usage_error() {
    let dir = temp_dir("extra_args");
    let out = zsc()
        .arg("init")
        .arg("unexpected")
        .current_dir(&dir)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("usage"), "stderr: {}", stderr);
    assert!(!dir.join("project.zson").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A file literally named `init` is shadowed by the subcommand; run it as
/// `./init` (same convention as `./compile` / `./run` / `./-e`, §158/§100).
#[test]
fn test_zsc_init_file_named_init_runs_via_dot_slash() {
    let dir = temp_dir("init_file");
    std::fs::write(dir.join("init"), "print(\"file mode\")\n").unwrap();

    let shadowed = zsc().arg("init").current_dir(&dir).output().unwrap();
    assert_eq!(
        shadowed.status.code(),
        Some(0),
        "subcommand wins over the file"
    );
    assert!(dir.join("project.zson").is_file());

    std::fs::remove_file(dir.join("project.zson")).unwrap();
    let file_mode = zsc().arg("./init").current_dir(&dir).output().unwrap();
    assert_eq!(
        file_mode.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&file_mode.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&file_mode.stdout), "file mode\n");
    assert!(!dir.join("project.zson").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
