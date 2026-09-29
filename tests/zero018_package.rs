//! Job 018 — integration tests for the advanced dependency system
//! (Dependencies.md §90–§100): conflict resolution, removal, update, Git
//! sources, publishing, registry, metadata, integrity, cache and source
//! priority. One normal + one error/edge case per section, plus the
//! required end-to-end publish flow.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use zeroscript::hash;
use zeroscript::package::{
    install, parse_lockfile, publish, remove_dependency, update, Cache, Lockfile, Registry,
    Sources, CACHE_DIR, DEPS_DIR, LOCK_FILE, MANIFEST_FILE, PACKAGE_FILE,
};

static SEQ: AtomicUsize = AtomicUsize::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "zeroscript_job018_{}_{}_{}",
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

/// Per-test `Sources`: one registry plus a project-local cache (§98), so no
/// test ever reads the shared `$HOME` cache.
fn sources(root: &Path, registry: &Path) -> Sources {
    Sources::new()
        .registry(Registry::new(registry))
        .cache(Cache::new(root.join(CACHE_DIR)))
}

fn add_pkg_meta(
    registry: &Path,
    name: &str,
    version: &str,
    meta: &str,
    files: &[(&str, &str)],
) {
    let dir = registry.join(name).join(version);
    std::fs::create_dir_all(&dir).unwrap();
    write(&dir.join(PACKAGE_FILE), meta);
    for (rel, content) in files {
        write(&dir.join(rel), content);
    }
}

fn add_pkg(
    registry: &Path,
    name: &str,
    version: &str,
    deps: &[(&str, &str)],
    files: &[(&str, &str)],
) {
    let mut dep_body = String::new();
    for (i, (n, s)) in deps.iter().enumerate() {
        if i > 0 {
            dep_body.push_str(", ");
        }
        dep_body.push_str(&format!("\"{}\": \"{}\"", n, s));
    }
    let meta = format!(
        "{{\n    name: \"{}\",\n    version: \"{}\",\n    dependencies: {{ {} }}\n}}\n",
        name, version, dep_body
    );
    add_pkg_meta(registry, name, version, &meta, files);
}

fn new_project(tag: &str, manifest: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(&dir.join(MANIFEST_FILE), manifest);
    dir
}

fn lockfile(root: &Path) -> Lockfile {
    parse_lockfile(&read(&root.join(LOCK_FILE)), LOCK_FILE).unwrap()
}

fn locked_version(root: &Path, key: &str) -> Option<String> {
    lockfile(root)
        .packages
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, e)| e.version.to_string())
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

/// `git init` + commit with an inline identity (edition 2024 forbids
/// `env::set_var`, and the test environment has no global git config).
fn git_init_and_commit(dir: &Path) {
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
}

/// A complete Git package repository (§93): valid package.zson, one source
/// file, committed, tagged `v<version>` (§94's tag form).
fn make_git_package(dir: &Path, name: &str, version: &str) {
    write(
        &dir.join(PACKAGE_FILE),
        &format!(
            r#"{{ name: "{}", version: "{}", dependencies: {{}} }}"#,
            name, version
        ),
    );
    write(&dir.join("index.zs"), "export const X = 1");
    git_init_and_commit(dir);
    git(dir, &["tag", &format!("v{}", version)]);
}

// ---------------------------------------------------------------------------
// A. Dependency conflict resolution (§90)
// ---------------------------------------------------------------------------

// §90: one version that satisfies several dependency needs may be shared.
#[test]
fn job018_one_version_shared_by_several_needs() {
    let reg = temp_dir("a_shared_reg");
    add_pkg(&reg, "package-a", "1.0.0", &[("shared-utils", "1.x")], &[("a.zs", "x")]);
    add_pkg(&reg, "package-b", "1.0.0", &[("shared-utils", "1.x")], &[("b.zs", "x")]);
    add_pkg(&reg, "shared-utils", "1.5.0", &[], &[("s.zs", "old")]);
    add_pkg(&reg, "shared-utils", "1.8.0", &[], &[("s.zs", "new")]);
    let root = new_project(
        "a_shared",
        r#"{ name: "app", version: "1.0.0", dependencies: { "package-a": "1.x", "package-b": "1.x" } }"#,
    );
    let report = install(&root, &sources(&root, &reg)).unwrap();
    let shared: Vec<_> = report
        .resolved
        .iter()
        .filter(|(n, _)| n == "shared-utils")
        .collect();
    assert_eq!(shared.len(), 1);
    // Highest version satisfying both 1.x needs (§83 selectors, §90).
    assert_eq!(shared[0].1.to_string(), "1.8.0");
    // The shared copy keeps the flat §85 layout (single version).
    assert_eq!(
        read(&root.join(DEPS_DIR).join("shared-utils").join("s.zs")),
        "new"
    );
    assert_eq!(locked_version(&root, "shared-utils").as_deref(), Some("1.8.0"));
}

// §90/§83: every dependency must use a version satisfying its selector —
// with no satisfying version, ERROR; never a non-matching substitute.
#[test]
fn job018_selector_without_matching_version_is_error() {
    let reg = temp_dir("a_noversion_reg");
    add_pkg(&reg, "pkg", "1.0.0", &[], &[("p.zs", "x")]);
    let root = new_project(
        "a_noversion",
        r#"{ name: "app", version: "1.0.0", dependencies: { "pkg": "2.x" } }"#,
    );
    let err = install(&root, &sources(&root, &reg))
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("No version of 'pkg' matches selector '2.x'"),
        "{}",
        err
    );
}

// §90: a locked resolution yields the same dependency tree when the
// project is installed again from a valid lockfile.
#[test]
fn job018_locked_resolution_reproduces_on_reinstall() {
    let reg = temp_dir("a_reinstall_reg");
    add_pkg(&reg, "package-a", "1.0.0", &[("shared-utils", "1.x")], &[("a.zs", "x")]);
    add_pkg(&reg, "shared-utils", "1.5.0", &[], &[("s.zs", "x")]);
    add_pkg(&reg, "shared-utils", "1.8.0", &[], &[("s.zs", "x")]);
    let root = new_project(
        "a_reinstall",
        r#"{ name: "app", version: "1.0.0", dependencies: { "package-a": "1.x" } }"#,
    );
    let first = install(&root, &sources(&root, &reg)).unwrap();
    let lock1 = read(&root.join(LOCK_FILE));
    std::fs::remove_dir_all(root.join(DEPS_DIR)).unwrap();
    let second = install(&root, &sources(&root, &reg)).unwrap();
    assert_eq!(first.resolved, second.resolved);
    assert_eq!(lock1, read(&root.join(LOCK_FILE)));
    assert!(root
        .join(DEPS_DIR)
        .join("package-a")
        .join("a.zs")
        .is_file());
    assert_eq!(
        locked_version(&root, "shared-utils").as_deref(),
        Some("1.8.0")
    );
}

// ---------------------------------------------------------------------------
// B. Dependency removal (§91)
// ---------------------------------------------------------------------------

// §91: removal drops the direct dependency, keeps packages other
// dependencies still need, prunes the rest, and never touches source code.
#[test]
fn job018_remove_prunes_only_unneeded_packages() {
    let reg = temp_dir("b_remove_reg");
    add_pkg(&reg, "package-a", "1.0.0", &[("shared-utils", "1.x")], &[("a.zs", "x")]);
    add_pkg(&reg, "package-b", "1.0.0", &[("shared-utils", "1.x")], &[("b.zs", "x")]);
    add_pkg(&reg, "shared-utils", "1.0.0", &[], &[("s.zs", "x")]);
    let root = new_project(
        "b_remove",
        r#"{ name: "app", version: "1.0.0", dependencies: { "package-a": "1.x", "package-b": "1.x" } }"#,
    );
    write(
        &root.join("main.zs"),
        "import { x } from \"package-a\"\nprint(x)",
    );
    install(&root, &sources(&root, &reg)).unwrap();

    remove_dependency(&root, "package-b", &sources(&root, &reg)).unwrap();
    let manifest = read(&root.join(MANIFEST_FILE));
    assert!(!manifest.contains("package-b"), "{}", manifest);
    assert!(manifest.contains("package-a"), "{}", manifest);
    // No longer a direct dependency and no longer installed …
    assert!(!root.join(DEPS_DIR).join("package-b").exists());
    // … but shared-utils stays because package-a still requires it.
    assert!(root
        .join(DEPS_DIR)
        .join("shared-utils")
        .join("s.zs")
        .is_file());
    let lock = read(&root.join(LOCK_FILE));
    assert!(lock.contains("package-a"), "{}", lock);
    assert!(!lock.contains("package-b"), "{}", lock);
    assert!(lock.contains("shared-utils"), "{}", lock);
    // §91: project source code is never removed.
    assert!(root.join("main.zs").is_file());

    // Removing the last direct dependency prunes its transitive tree too.
    remove_dependency(&root, "package-a", &sources(&root, &reg)).unwrap();
    assert!(!root.join(DEPS_DIR).join("package-a").exists());
    assert!(!root.join(DEPS_DIR).join("shared-utils").exists());
}

// §91: removing a package that is not a direct project dependency → ERROR.
#[test]
fn job018_remove_undeclared_package_is_error() {
    let reg = temp_dir("b_undeclared_reg");
    let root = new_project(
        "b_undeclared",
        r#"{ name: "app", version: "1.0.0", dependencies: {} }"#,
    );
    let err = remove_dependency(&root, "ghost", &sources(&root, &reg))
        .unwrap_err()
        .to_string();
    assert!(err.contains("not a direct dependency"), "{}", err);
}

// ---------------------------------------------------------------------------
// C. Dependency update (§92)
// ---------------------------------------------------------------------------

// §92: `zpm install` never auto-upgrades a locked version; `zpm update` is
// the explicit operation that bumps within the selector and re-locks.
#[test]
fn job018_update_bumps_locked_version_install_does_not() {
    let reg = temp_dir("c_update_reg");
    add_pkg(&reg, "http-utils", "1.2.0", &[], &[("client.zs", "old")]);
    let root = new_project(
        "c_update",
        r#"{ name: "app", version: "1.0.0", dependencies: { "http-utils": "1.2.x" } }"#,
    );
    install(&root, &sources(&root, &reg)).unwrap();
    assert_eq!(
        locked_version(&root, "http-utils").as_deref(),
        Some("1.2.0")
    );

    // A newer matching version appears in the registry.
    add_pkg(&reg, "http-utils", "1.2.5", &[], &[("client.zs", "mid")]);
    add_pkg(&reg, "http-utils", "1.2.10", &[], &[("client.zs", "new")]);

    // §92: "zpm install tidak melakukan upgrade otomatis".
    install(&root, &sources(&root, &reg)).unwrap();
    assert_eq!(
        locked_version(&root, "http-utils").as_deref(),
        Some("1.2.0")
    );

    // §92: update selects the newest matching version and rewrites the lock.
    let report = update(&root, &sources(&root, &reg)).unwrap();
    assert!(report
        .installed
        .iter()
        .any(|(n, v)| n == "http-utils" && v.to_string() == "1.2.10"));
    assert_eq!(
        locked_version(&root, "http-utils").as_deref(),
        Some("1.2.10")
    );
    assert_eq!(
        read(&root
            .join(DEPS_DIR)
            .join("http-utils")
            .join("client.zs")),
        "new"
    );
}

// §92: when no newer version exists, the dependency stays where it is.
#[test]
fn job018_update_keeps_version_when_no_newer_exists() {
    let reg = temp_dir("c_noupdate_reg");
    add_pkg(&reg, "pkg", "1.0.0", &[], &[("p.zs", "x")]);
    let root = new_project(
        "c_noupdate",
        r#"{ name: "app", version: "1.0.0", dependencies: { "pkg": "1.x" } }"#,
    );
    install(&root, &sources(&root, &reg)).unwrap();
    let report = update(&root, &sources(&root, &reg)).unwrap();
    assert!(report
        .resolved
        .iter()
        .any(|(n, v)| n == "pkg" && v.to_string() == "1.0.0"));
    // Nothing was re-copied: same version already installed (§85).
    assert!(report.installed.is_empty());
    assert_eq!(locked_version(&root, "pkg").as_deref(), Some("1.0.0"));
}

// §92: a version that does not satisfy the selector must never be chosen.
#[test]
fn job018_update_selector_no_longer_satisfiable_is_error() {
    let reg = temp_dir("c_error_reg");
    add_pkg(&reg, "pkg", "2.0.0", &[], &[("p.zs", "x")]);
    let root = new_project(
        "c_error",
        r#"{ name: "app", version: "1.0.0", dependencies: { "pkg": "1.x" } }"#,
    );
    let err = update(&root, &sources(&root, &reg))
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("No version of 'pkg' matches selector '1.x'"),
        "{}",
        err
    );
}

// ---------------------------------------------------------------------------
// D. Git package source (§93)
// ---------------------------------------------------------------------------

// §93: a git: dependency is fetched from the repository into .zsc_m/ and
// the concrete commit is recorded in project.lock.zson.
#[test]
fn job018_git_dependency_installs_and_records_commit() {
    let repo = temp_dir("d_repo");
    make_git_package(&repo, "gitpkg", "1.0.0");
    let root = new_project(
        "d_install",
        &format!(
            r#"{{ name: "app", version: "1.0.0", dependencies: {{ "gitpkg": "git:{}#v1.0.0" }} }}"#,
            repo.display()
        ),
    );
    install(&root, &sources(&root, &temp_dir("d_empty_reg"))).unwrap();
    assert!(root
        .join(DEPS_DIR)
        .join("gitpkg")
        .join("index.zs")
        .is_file());
    // §98: the fetched package also lands in the cache.
    assert!(root
        .join(CACHE_DIR)
        .join("gitpkg")
        .join("1.0.0")
        .join("index.zs")
        .is_file());
    let entry = lockfile(&root)
        .packages
        .into_iter()
        .find(|(k, _)| k == "gitpkg")
        .expect("git dependency must be locked");
    assert_eq!(
        entry.1.source.clone().unwrap(),
        format!("git:{}#v1.0.0", repo.display())
    );
    // §93: "Commit yang digunakan dapat dicatat pada project.lock.zson".
    let commit = entry.1.commit.clone().expect("commit recorded");
    assert_eq!(commit.len(), 40);
    assert!(commit.chars().all(|c| c.is_ascii_hexdigit()), "{}", commit);
}

// §93: an unreachable/invalid Git source → ERROR.
#[test]
fn job018_git_source_unreachable_is_error() {
    // The directory exists but is not a Git repository.
    let missing = temp_dir("d_missing_repo");
    let root = new_project(
        "d_bad",
        &format!(
            r#"{{ name: "app", version: "1.0.0", dependencies: {{ "gitpkg": "git:{}" }} }}"#,
            missing.display()
        ),
    );
    let err = install(&root, &sources(&root, &temp_dir("d_bad_reg")))
        .unwrap_err()
        .to_string();
    assert!(err.contains("git clone failed"), "{}", err);
}

// ---------------------------------------------------------------------------
// E. Package publishing (§94) + required end-to-end flow
// ---------------------------------------------------------------------------

// Required e2e: manifest → install → publish to the local registry →
// another project installs the published package (and its dependency).
#[test]
fn job018_e2e_install_publish_reinstall_from_another_project() {
    let reg = temp_dir("e_registry");
    add_pkg(&reg, "dep-base", "1.0.0", &[], &[("base.zs", "b")]);

    // The package under development is itself a project with a dependency.
    let pkg_src = temp_dir("e_pkgsrc");
    write(
        &pkg_src.join(MANIFEST_FILE),
        r#"{ name: "pub-pkg", version: "1.0.0", dependencies: { "dep-base": "1.x" } }"#,
    );
    write(
        &pkg_src.join(PACKAGE_FILE),
        r#"{ name: "pub-pkg", version: "1.0.0", description: "published", dependencies: { "dep-base": "1.x" } }"#,
    );
    write(
        &pkg_src.join("mod.zs"),
        "export function f() { return 7 }",
    );
    install(&pkg_src, &sources(&pkg_src, &reg)).unwrap();
    assert!(pkg_src
        .join(DEPS_DIR)
        .join("dep-base")
        .join("base.zs")
        .is_file());

    // §94: publish into the local registry directory (minimum local
    // implementation of Git-based distribution — see final report).
    let target = publish(&pkg_src, Some(&reg)).unwrap();
    assert!(target.join(PACKAGE_FILE).is_file());
    assert!(target.join("mod.zs").is_file());
    // zpm's own state is not package content.
    assert!(!target.join(DEPS_DIR).exists());
    assert!(!target.join(CACHE_DIR).exists());

    // Another project installs it: the declared dependency arrives too.
    let other = new_project(
        "e_other",
        r#"{ name: "other-app", version: "1.0.0", dependencies: { "pub-pkg": "1.x" } }"#,
    );
    install(&other, &sources(&other, &reg)).unwrap();
    assert!(other
        .join(DEPS_DIR)
        .join("pub-pkg")
        .join("mod.zs")
        .is_file());
    assert!(other
        .join(DEPS_DIR)
        .join("dep-base")
        .join("base.zs")
        .is_file());
}

// §94: publishing marks the release with the v<version> tag; a published
// version is final — publishing it again → ERROR.
#[test]
fn job018_publish_git_repo_marks_release_tag_and_is_final() {
    let reg = temp_dir("e_tag_reg");
    let repo = temp_dir("e_tag_repo");
    write(
        &repo.join(PACKAGE_FILE),
        r#"{ name: "tagged-pkg", version: "1.0.0", dependencies: {} }"#,
    );
    write(&repo.join("f.zs"), "export const X = 1");
    git_init_and_commit(&repo);

    publish(&repo, Some(&reg)).unwrap();
    let out = Command::new("git")
        .args(["tag", "--list", "v1.0.0"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "v1.0.0");

    let err = publish(&repo, Some(&reg)).unwrap_err().to_string();
    assert!(err.contains("already published"), "{}", err);
}

// §94: a package without package.zson cannot be published → ERROR.
#[test]
fn job018_publish_without_package_zson_is_error() {
    let reg = temp_dir("e_nopkg_reg");
    let pkg = temp_dir("e_nopkg_src");
    write(&pkg.join("f.zs"), "export const X = 1");
    let err = publish(&pkg, Some(&reg)).unwrap_err().to_string();
    assert!(err.contains("No package.zson found"), "{}", err);
}

// ---------------------------------------------------------------------------
// F. Package registry (§95)
// ---------------------------------------------------------------------------

// §95: the official and additional registries can all be consulted.
#[test]
fn job018_additional_registry_supplies_missing_package() {
    let official = temp_dir("f_official");
    let extra = temp_dir("f_extra");
    add_pkg(&extra, "http-utils", "1.2.10", &[], &[("client.zs", "x")]);
    let root = new_project(
        "f_extra",
        r#"{ name: "app", version: "1.0.0", dependencies: { "http-utils": "1.2.x" } }"#,
    );
    let all = Sources::new()
        .registry(Registry::new(&official))
        .registry(Registry::new(&extra))
        .cache(Cache::new(root.join(CACHE_DIR)));
    install(&root, &all).unwrap();
    assert!(root
        .join(DEPS_DIR)
        .join("http-utils")
        .join("client.zs")
        .is_file());
    assert_eq!(
        locked_version(&root, "http-utils").as_deref(),
        Some("1.2.10")
    );
}

// §95: a package unknown to every registry → ERROR.
#[test]
fn job018_package_unknown_to_every_registry_is_error() {
    let reg = temp_dir("f_unknown_reg");
    let root = new_project(
        "f_unknown",
        r#"{ name: "app", version: "1.0.0", dependencies: { "ghost": "1.x" } }"#,
    );
    let err = install(&root, &sources(&root, &reg))
        .unwrap_err()
        .to_string();
    assert!(err.contains("not found in any registry"), "{}", err);
}

// §95: an invalid registry index → ERROR.
#[test]
fn job018_invalid_registry_index_is_error() {
    let reg = temp_dir("f_badindex_reg");
    std::fs::create_dir_all(reg.join("pkg").join("1.0.0")).unwrap();
    write(
        &reg.join("pkg").join("index.zson"),
        "definitely not valid zson {{{",
    );
    let root = new_project(
        "f_badindex",
        r#"{ name: "app", version: "1.0.0", dependencies: { "pkg": "1.x" } }"#,
    );
    let err = install(&root, &sources(&root, &reg))
        .unwrap_err()
        .to_string();
    assert!(err.contains("index.zson"), "{}", err);
}

// ---------------------------------------------------------------------------
// G. Package metadata — package.zson (§96)
// ---------------------------------------------------------------------------

// §96: name/version required, description/author/license/repository
// optional but must be strings when present.
#[test]
fn job018_optional_metadata_fields_are_accepted() {
    let reg = temp_dir("g_optional_reg");
    add_pkg_meta(
        &reg,
        "meta-pkg",
        "1.0.0",
        r#"{ name: "meta-pkg", version: "1.0.0", description: "HTTP utilities", author: "KaelTzy", license: "MIT", repository: "https://example.com/repo", dependencies: {} }"#,
        &[("m.zs", "export const M = 1")],
    );
    let root = new_project(
        "g_optional",
        r#"{ name: "app", version: "1.0.0", dependencies: { "meta-pkg": "1.x" } }"#,
    );
    install(&root, &sources(&root, &reg)).unwrap();
    assert!(root
        .join(DEPS_DIR)
        .join("meta-pkg")
        .join(PACKAGE_FILE)
        .is_file());
}

// §96/§94: the name in package.zson must match the name the resolver uses
// — invalid metadata → ERROR.
#[test]
fn job018_registry_copy_with_wrong_name_is_error() {
    let reg = temp_dir("g_mismatch_reg");
    add_pkg_meta(
        &reg,
        "shared",
        "1.0.0",
        r#"{ name: "other-name", version: "1.0.0", dependencies: {} }"#,
        &[("s.zs", "x")],
    );
    let root = new_project(
        "g_mismatch",
        r#"{ name: "app", version: "1.0.0", dependencies: { "shared": "1.x" } }"#,
    );
    let err = install(&root, &sources(&root, &reg))
        .unwrap_err()
        .to_string();
    assert!(err.contains("package.zson declares name"), "{}", err);
}

// ---------------------------------------------------------------------------
// H. Package integrity (§97)
// ---------------------------------------------------------------------------

/// Write `<registry>/<name>/index.zson` per §95/§97: an entry is either a
/// plain source string or `{ source, hash }`.
fn write_index(registry: &Path, name: &str, version: &str, hash: Option<&str>) {
    let body = match hash {
        Some(h) => format!(r#"{{ source: "{}", hash: "{}" }}"#, version, h),
        None => format!(r#"{{ source: "{}" }}"#, version),
    };
    write(
        &registry.join(name).join("index.zson"),
        &format!(
            r#"{{ name: "{}", versions: {{ "{}": {} }} }}"#,
            name, version, body
        ),
    );
}

// §97: a registry-provided hash is verified against the received package
// and recorded in project.lock.zson.
#[test]
fn job018_index_hash_is_verified_and_recorded() {
    let reg = temp_dir("h_ok_reg");
    add_pkg(&reg, "http-utils", "1.2.0", &[], &[("client.zs", "x")]);
    let digest = hash::hash_dir(&reg.join("http-utils").join("1.2.0")).unwrap();
    write_index(&reg, "http-utils", "1.2.0", Some(&digest));
    let root = new_project(
        "h_ok",
        r#"{ name: "app", version: "1.0.0", dependencies: { "http-utils": "1.2.x" } }"#,
    );
    install(&root, &sources(&root, &reg)).unwrap();
    let entry = lockfile(&root)
        .packages
        .into_iter()
        .find(|(k, _)| k == "http-utils")
        .unwrap()
        .1;
    assert_eq!(entry.hash.as_deref(), Some(digest.as_str()));
}

// §97: "Hash yang berbeda → ERROR" — and a package that fails verification
// must not be counted as installed.
#[test]
fn job018_index_hash_mismatch_is_error() {
    let reg = temp_dir("h_bad_reg");
    add_pkg(&reg, "http-utils", "1.2.0", &[], &[("client.zs", "x")]);
    let wrong = format!("sha256:{}", "0".repeat(64));
    write_index(&reg, "http-utils", "1.2.0", Some(&wrong));
    let root = new_project(
        "h_bad",
        r#"{ name: "app", version: "1.0.0", dependencies: { "http-utils": "1.2.x" } }"#,
    );
    let err = install(&root, &sources(&root, &reg))
        .unwrap_err()
        .to_string();
    assert!(err.contains("Integrity check failed"), "{}", err);
    assert!(!root.join(DEPS_DIR).join("http-utils").exists());
    assert!(!root.join(LOCK_FILE).exists());
}

// ---------------------------------------------------------------------------
// I. Package cache (§98)
// ---------------------------------------------------------------------------

// §98/§99: a verified cache copy satisfies the install — content comes
// from the cache before the registry.
#[test]
fn job018_valid_cache_is_used_when_registry_copy_is_gone() {
    let reg = temp_dir("i_cache_reg");
    add_pkg(
        &reg,
        "http-utils",
        "1.2.0",
        &[],
        &[("client.zs", "cached-content")],
    );
    let root = new_project(
        "i_cache",
        r#"{ name: "app", version: "1.0.0", dependencies: { "http-utils": "1.2.x" } }"#,
    );
    install(&root, &sources(&root, &reg)).unwrap();
    assert!(root
        .join(CACHE_DIR)
        .join("http-utils")
        .join("1.2.0")
        .join("client.zs")
        .is_file());

    // Gut the registry's content and the project's install; only the cache
    // can still serve the package.
    std::fs::remove_file(
        reg.join("http-utils")
            .join("1.2.0")
            .join(PACKAGE_FILE),
    )
    .unwrap();
    std::fs::remove_dir_all(root.join(DEPS_DIR)).unwrap();
    install(&root, &sources(&root, &reg)).unwrap();
    assert_eq!(
        read(&root
            .join(DEPS_DIR)
            .join("http-utils")
            .join("client.zs")),
        "cached-content"
    );
}

// §98: "Cache yang tidak valid atau rusak tidak boleh digunakan" — a
// tampered cache is rejected; with no other valid source → ERROR.
#[test]
fn job018_corrupt_cache_is_not_used() {
    let reg = temp_dir("i_corrupt_reg");
    add_pkg(
        &reg,
        "http-utils",
        "1.2.0",
        &[],
        &[("client.zs", "original")],
    );
    let root = new_project(
        "i_corrupt",
        r#"{ name: "app", version: "1.0.0", dependencies: { "http-utils": "1.2.x" } }"#,
    );
    install(&root, &sources(&root, &reg)).unwrap();

    // Damage the cached copy (sidecar digest no longer matches) and make
    // the registry copy unusable as well.
    write(
        &root
            .join(CACHE_DIR)
            .join("http-utils")
            .join("1.2.0")
            .join("client.zs"),
        "tampered",
    );
    std::fs::remove_file(
        reg.join("http-utils")
            .join("1.2.0")
            .join(PACKAGE_FILE),
    )
    .unwrap();
    std::fs::remove_dir_all(root.join(DEPS_DIR)).unwrap();

    let err = install(&root, &sources(&root, &reg))
        .unwrap_err()
        .to_string();
    assert!(err.contains("not found"), "{}", err);
    // The damaged cache copy was refused, not installed.
    assert!(!root.join(DEPS_DIR).join("http-utils").exists());
}

// §98: clearing the cache never changes project.zson, project.lock.zson
// or the installed dependencies.
#[test]
fn job018_clearing_cache_leaves_project_untouched() {
    let reg = temp_dir("i_clear_reg");
    add_pkg(&reg, "pkg", "1.0.0", &[], &[("p.zs", "x")]);
    let root = new_project(
        "i_clear",
        r#"{ name: "app", version: "1.0.0", dependencies: { "pkg": "1.x" } }"#,
    );
    install(&root, &sources(&root, &reg)).unwrap();
    let lock_before = read(&root.join(LOCK_FILE));
    let manifest_before = read(&root.join(MANIFEST_FILE));
    std::fs::remove_dir_all(root.join(CACHE_DIR)).unwrap();
    assert_eq!(lock_before, read(&root.join(LOCK_FILE)));
    assert_eq!(manifest_before, read(&root.join(MANIFEST_FILE)));
    assert!(root.join(DEPS_DIR).join("pkg").join("p.zs").is_file());
}

// ---------------------------------------------------------------------------
// J. Package source priority (§99)
// ---------------------------------------------------------------------------

// §99: an explicit git: dependency goes straight to Git and bypasses the
// registry — the installed version is Git's, not the registry's.
#[test]
fn job018_explicit_git_source_wins_over_registry() {
    let reg = temp_dir("j_git_reg");
    add_pkg(&reg, "gitpkg", "1.0.0", &[], &[("marker.zs", "from-registry")]);
    let repo = temp_dir("j_git_repo");
    make_git_package(&repo, "gitpkg", "2.5.0");
    let root = new_project(
        "j_gitwins",
        &format!(
            r#"{{ name: "app", version: "1.0.0", dependencies: {{ "gitpkg": "git:{}#v2.5.0" }} }}"#,
            repo.display()
        ),
    );
    install(&root, &sources(&root, &reg)).unwrap();
    // §99: the explicit source decides the version — Git's 2.5.0, never
    // the registry's 1.0.0.
    assert_eq!(locked_version(&root, "gitpkg").as_deref(), Some("2.5.0"));
    let entry = lockfile(&root)
        .packages
        .into_iter()
        .find(|(k, _)| k == "gitpkg")
        .unwrap()
        .1;
    assert!(entry
        .source
        .clone()
        .unwrap()
        .starts_with("git:"));
    assert!(root
        .join(DEPS_DIR)
        .join("gitpkg")
        .join("index.zs")
        .is_file());
    assert!(!root
        .join(DEPS_DIR)
        .join("gitpkg")
        .join("marker.zs")
        .exists());
}

// §94/§99: a source that does not match the dependency requirement (the
// repository declares a different package) → ERROR.
#[test]
fn job018_git_source_with_wrong_package_name_is_error() {
    let repo = temp_dir("j_wrongname_repo");
    make_git_package(&repo, "totally-other", "1.0.0");
    let root = new_project(
        "j_wrongname",
        &format!(
            r#"{{ name: "app", version: "1.0.0", dependencies: {{ "gitpkg": "git:{}#v1.0.0" }} }}"#,
            repo.display()
        ),
    );
    let err = install(&root, &sources(&root, &temp_dir("j_wrongname_reg")))
        .unwrap_err()
        .to_string();
    assert!(err.contains("package.zson declares name"), "{}", err);
}
