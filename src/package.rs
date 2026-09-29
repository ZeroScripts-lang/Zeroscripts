//! Package & dependency system core (Dependencies.md §81-§100).
//!
//! Layout produced by `zpm install` (§82, §85) — one installed copy per
//! package name:
//!
//! ```text
//! my-app/
//! ├── project.zson
//! ├── project.lock.zson
//! └── .zsc_m/
//!     ├── http-utils/
//!     └── database/
//! ```
//!
//! When §90 conflict resolution keeps several versions of the same package,
//! every copy of that name moves under `.zsc_m/<name>/versions/<version>/`;
//! the single-copy layout above stays byte-for-byte the layout §82/§85/§87
//! show. Local downloads are cached in `.zsc_cache/<name>/<version>/` (§98).

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::git::{self, GitSource};
use crate::hash;
use crate::json::{self, JsonValue};
use crate::version::{highest_matching, Selector, Version};

pub const MANIFEST_FILE: &str = "project.zson";
pub const LOCK_FILE: &str = "project.lock.zson";
pub const PACKAGE_FILE: &str = "package.zson";
pub const DEPS_DIR: &str = ".zsc_m";
pub const CACHE_DIR: &str = ".zsc_cache";
/// Sub-directory holding the extra copies of a package that §90 keeps apart.
pub const VERSIONS_DIR: &str = "versions";
/// Registry index file (§95): `<registry>/<name>/index.zson`.
pub const INDEX_FILE: &str = "index.zson";

#[derive(Debug, Clone, PartialEq)]
pub enum PackageError {
    Io(String),
    /// Structural ZSON/JSON parse failure in a `.zson` file.
    Parse { file: String, detail: String },
    /// Manifest field missing or of the wrong type.
    Manifest(String),
    /// Package (or package version) does not exist at its source.
    NotFound(String),
    /// Package exists but no version satisfies the selector (§83, §90).
    NoVersion { package: String, selectors: String },
    /// package.zson does not describe the package it claims to (§94, §96).
    Metadata { package: String, detail: String },
    /// Installed copy differs from the locked version (§87).
    VersionMismatch {
        package: String,
        installed: String,
        locked: String,
    },
    Circular(Vec<String>),
    ScriptNotFound(String),
    Command(String),
    Resolution(String),
    /// Registry cannot be accessed or its index is invalid (§95).
    Registry(String),
    /// Downloaded package does not match the recorded hash (§97).
    Integrity(String),
    /// Git source could not be used (§93).
    Git(String),
    /// `zpm remove` target is not a direct dependency of the project (§91).
    NotDeclared(String),
    /// Package cannot be published (§94).
    Publish(String),
}

impl PackageError {
    /// Message without the `ERROR: ` prefix, for embedding in other errors.
    pub fn message(&self) -> String {
        match self {
            PackageError::Io(m) => m.clone(),
            PackageError::Parse { file, detail } => format!("{}: {}", file, detail),
            PackageError::Manifest(m) => m.clone(),
            PackageError::NotFound(m) => m.clone(),
            PackageError::NoVersion { package, selectors } => format!(
                "No version of '{}' matches selector '{}'",
                package, selectors
            ),
            PackageError::Metadata { package, detail } => {
                format!("Invalid package metadata for '{}': {}", package, detail)
            }
            PackageError::VersionMismatch {
                package,
                installed,
                locked,
            } => format!(
                "Installed version of '{}' is {} but project.lock.zson locks {} — run `zpm install`",
                package, installed, locked
            ),
            PackageError::Circular(chain) => format!(
                "Circular package dependency detected: {}",
                chain.join(" -> ")
            ),
            PackageError::ScriptNotFound(name) => format!("Script not found: {}", name),
            PackageError::Command(m) => m.clone(),
            PackageError::Resolution(m) => m.clone(),
            PackageError::Registry(m) => m.clone(),
            PackageError::Integrity(m) => m.clone(),
            PackageError::Git(m) => m.clone(),
            PackageError::NotDeclared(m) => m.clone(),
            PackageError::Publish(m) => m.clone(),
        }
    }
}

impl fmt::Display for PackageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ERROR: {}", self.message())
    }
}

impl std::error::Error for PackageError {}

fn io_err(e: std::io::Error) -> PackageError {
    PackageError::Io(e.to_string())
}

// ---------------------------------------------------------------------------
// Manifests (§82 project.zson, §96 package.zson)
// ---------------------------------------------------------------------------

/// project.zson (§82).
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    /// Declaration order preserved (resolution order follows it).
    pub dependencies: Vec<(String, String)>,
    pub scripts: Vec<(String, String)>,
}

/// package.zson (§94, §96) — the subset Job 017 needs for §88/§89.
#[derive(Debug, Clone, PartialEq)]
pub struct PackageMeta {
    pub name: String,
    pub version: String,
    pub dependencies: Vec<(String, String)>,
}

fn root_object(text: &str, file: &str) -> Result<JsonValue, PackageError> {
    let v = json::parse_zson(text).map_err(|detail| PackageError::Parse {
        file: file.to_string(),
        detail,
    })?;
    if v.as_object().is_none() {
        return Err(PackageError::Manifest(format!(
            "{}: root must be an object",
            file
        )));
    }
    Ok(v)
}

fn require_string(v: &JsonValue, key: &str, file: &str) -> Result<String, PackageError> {
    let field = v.get(key).ok_or_else(|| {
        PackageError::Manifest(format!("{}: missing required field '{}'", file, key))
    })?;
    field
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| {
            PackageError::Manifest(format!("{}: field '{}' must be a string", file, key))
        })
}

fn safe_package_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("package name must not be empty".to_string());
    }
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(format!("invalid package name '{}'", name));
    }
    if name.contains('@') {
        // `@` is reserved: the lockfile uses `name@version` as the key of a
        // multi-version package (§90), so a name containing `@` would be
        // ambiguous there. Mechanical choice — the spec fixes no name charset.
        return Err(format!("invalid package name '{}' ('@' is reserved)", name));
    }
    if name.contains('\0') {
        return Err("package name must not contain NUL".to_string());
    }
    Ok(())
}

/// A dependency requirement as written in a manifest: a §83 version selector
/// or a §93 `git:` source.
#[derive(Debug, Clone, PartialEq)]
pub enum DepReq {
    Selector(Selector),
    Git(GitSource),
}

impl DepReq {
    pub fn parse(s: &str) -> Result<DepReq, String> {
        if s.starts_with("git:") {
            Ok(DepReq::Git(GitSource::parse(s)?))
        } else {
            Ok(DepReq::Selector(Selector::parse(s)?))
        }
    }
}

impl fmt::Display for DepReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DepReq::Selector(s) => write!(f, "{}", s),
            DepReq::Git(g) => write!(f, "{}", g),
        }
    }
}

fn parse_dependencies(
    v: &JsonValue,
    file: &str,
    required: bool,
) -> Result<Vec<(String, String)>, PackageError> {
    let field = match v.get("dependencies") {
        Some(f) => f,
        None if required => {
            return Err(PackageError::Manifest(format!(
                "{}: missing required field 'dependencies'",
                file
            )))
        }
        None => return Ok(Vec::new()),
    };
    let entries = field.as_object().ok_or_else(|| {
        PackageError::Manifest(format!("{}: field 'dependencies' must be an object", file))
    })?;
    let mut deps = Vec::new();
    for (name, value) in entries {
        safe_package_name(name).map_err(|e| PackageError::Manifest(format!("{}: {}", file, e)))?;
        let sel = value.as_str().ok_or_else(|| {
            PackageError::Manifest(format!(
                "{}: dependency '{}' version must be a string",
                file, name
            ))
        })?;
        DepReq::parse(sel).map_err(|e| {
            PackageError::Manifest(format!("{}: dependency '{}': {}", file, name, e))
        })?;
        deps.push((name.clone(), sel.to_string()));
    }
    Ok(deps)
}

/// Parse `project.zson` (§82). `name`, `version` and `dependencies` are
/// required; `scripts` (§86) is optional but must be an object of string
/// commands when present. Unknown fields are ignored (§96).
pub fn parse_manifest(text: &str, file: &str) -> Result<Manifest, PackageError> {
    let v = root_object(text, file)?;
    let name = require_string(&v, "name", file)?;
    if name.is_empty() {
        return Err(PackageError::Manifest(format!(
            "{}: field 'name' must not be empty",
            file
        )));
    }
    let version = require_string(&v, "version", file)?;
    let dependencies = parse_dependencies(&v, file, true)?;

    let mut scripts = Vec::new();
    if let Some(s) = v.get("scripts") {
        let entries = s.as_object().ok_or_else(|| {
            PackageError::Manifest(format!("{}: field 'scripts' must be an object", file))
        })?;
        for (name, value) in entries {
            let cmd = value.as_str().ok_or_else(|| {
                PackageError::Manifest(format!(
                    "{}: script '{}' must be a string command",
                    file, name
                ))
            })?;
            scripts.push((name.clone(), cmd.to_string()));
        }
    }

    Ok(Manifest {
        name,
        version,
        dependencies,
        scripts,
    })
}

/// Parse `package.zson` (§94, §96): `name` and `version` required,
/// `version` must be valid SemVer, `dependencies` optional. Optional §96
/// metadata (`description`, `author`, `license`, `repository`) must be
/// strings when present — "Metadata yang tidak valid → ERROR"; unknown
/// fields are ignored ("Field tambahan dapat ditambahkan pada masa depan").
pub fn parse_package_meta(text: &str, file: &str) -> Result<PackageMeta, PackageError> {
    let v = root_object(text, file)?;
    let name = require_string(&v, "name", file)?;
    safe_package_name(&name).map_err(|e| PackageError::Manifest(format!("{}: {}", file, e)))?;
    let version = require_string(&v, "version", file)?;
    let parsed: Version = version.parse().map_err(|e| {
        PackageError::Manifest(format!("{}: field 'version': {}", file, e))
    })?;
    for key in ["description", "author", "license", "repository"] {
        if let Some(field) = v.get(key) {
            if field.as_str().is_none() {
                return Err(PackageError::Manifest(format!(
                    "{}: field '{}' must be a string",
                    file, key
                )));
            }
        }
    }
    let dependencies = parse_dependencies(&v, file, false)?;
    Ok(PackageMeta {
        name,
        version: parsed.to_string(),
        dependencies,
    })
}

// ---------------------------------------------------------------------------
// Lockfile (§84, §90, §93, §97)
// ---------------------------------------------------------------------------

/// One resolved package in `project.lock.zson`. A plain version (§84's
/// `"http-utils": "1.2.10"`) is stored without any extra field; `source`
/// (§93), `commit` (§93/§97) and `hash` (§97) appear when the source
/// provides them.
#[derive(Debug, Clone, PartialEq)]
pub struct LockEntry {
    pub version: Version,
    /// `git:` specifier or index source (§93, §95, §97).
    pub source: Option<String>,
    /// Concrete Git commit (§93, §97).
    pub commit: Option<String>,
    /// `sha256:...` identity (§97).
    pub hash: Option<String>,
}

impl LockEntry {
    pub fn new(version: Version) -> Self {
        LockEntry {
            version,
            source: None,
            commit: None,
            hash: None,
        }
    }

    fn has_extra(&self) -> bool {
        self.source.is_some() || self.commit.is_some() || self.hash.is_some()
    }
}

impl fmt::Display for LockEntry {
    /// The locked version only — the extra identity fields (§93/§97) are
    /// printed by `render_lockfile`, not by the entry itself.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.version.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lockfile {
    /// Resolution order. Key is the plain package name while the package has
    /// a single installed version, `name@version` once §90 keeps several
    /// versions of the same name (mechanical key choice: package names may
    /// not contain `@`).
    pub packages: Vec<(String, LockEntry)>,
}

impl Lockfile {
    /// The locked version of a package installed as a single copy (§84).
    pub fn get(&self, name: &str) -> Option<&Version> {
        self.packages
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, e)| &e.version)
    }

    /// Every lock entry belonging to `name`: the plain key plus all
    /// `name@version` keys (§90 multi-version installs).
    pub fn entries_for(&self, name: &str) -> Vec<&LockEntry> {
        self.packages
            .iter()
            .filter(|(n, _)| n == name || n.starts_with(&format!("{}@", name)))
            .map(|(_, e)| e)
            .collect()
    }

    /// The entry locking exactly `name@version`: matched through either the
    /// plain `name` key or a `name@version` key of the same version.
    pub fn entry(&self, name: &str, version: &Version) -> Option<&LockEntry> {
        self.packages
            .iter()
            .filter(|(n, e)| lock_key_name(n) == name && e.version == *version)
            .map(|(_, e)| e)
            .next()
    }

    /// The locked entry whose `source` matches (used to recognise a git
    /// requirement that is already pinned, §93/§99).
    pub fn entry_by_source(&self, source: &str) -> Option<(&str, &LockEntry)> {
        self.packages
            .iter()
            .find(|(_, e)| e.source.as_deref() == Some(source))
            .map(|(n, e)| (n.as_str(), e))
    }
}

fn lock_error(file: &str, detail: impl Into<String>) -> PackageError {
    PackageError::Parse {
        file: file.to_string(),
        detail: detail.into(),
    }
}

/// Parse `project.lock.zson`. Any structural problem makes the lockfile
/// invalid, in which case the caller re-resolves (§84: "Jika lockfile
/// tersedia dan masih valid …").
pub fn parse_lockfile(text: &str, file: &str) -> Result<Lockfile, PackageError> {
    let v = root_object(text, file)?;
    let packages = v
        .get("packages")
        .and_then(|p| p.as_object())
        .ok_or_else(|| lock_error(file, "field 'packages' must be an object"))?;
    let mut out = Vec::new();
    for (key, value) in packages {
        // Key: `name` or `name@version`.
        let (name, key_version) = match key.find('@') {
            Some(i) => (&key[..i], Some(&key[i + 1..])),
            None => (key.as_str(), None),
        };
        safe_package_name(name).map_err(|e| lock_error(file, e))?;
        if name.is_empty() {
            return Err(lock_error(file, "package key must not be empty"));
        }
        if let Some(kv) = key_version {
            if kv.is_empty() {
                return Err(lock_error(file, format!("package key '{}' has an empty version", key)));
            }
        }

        let entry = match value {
            JsonValue::String(s) => {
                let version: Version = s
                    .parse()
                    .map_err(|e| lock_error(file, format!("package '{}': {}", name, e)))?;
                LockEntry::new(version)
            }
            JsonValue::Object(fields) => {
                let raw = fields
                    .iter()
                    .find(|(k, _)| k == "version")
                    .map(|(_, v)| v)
                    .ok_or_else(|| {
                        lock_error(file, format!("package '{}' is missing field 'version'", key))
                    })?;
                let s = raw.as_str().ok_or_else(|| {
                    lock_error(file, format!("package '{}' field 'version' must be a string", key))
                })?;
                let version: Version = s
                    .parse()
                    .map_err(|e| lock_error(file, format!("package '{}': {}", name, e)))?;
                let string_field = |k: &str| -> Result<Option<String>, PackageError> {
                    match fields.iter().find(|(key, _)| key == k) {
                        Some((_, JsonValue::Null)) | None => Ok(None),
                        Some((_, JsonValue::String(s))) => Ok(Some(s.clone())),
                        Some((_, _)) => Err(lock_error(
                            file,
                            format!("package '{}' field '{}' must be a string", key, k),
                        )),
                    }
                };
                LockEntry {
                    version,
                    source: string_field("source")?,
                    commit: string_field("commit")?,
                    hash: string_field("hash")?,
                }
            }
            _ => {
                return Err(lock_error(
                    file,
                    format!("package '{}' must map to a version string or object", key),
                ))
            }
        };
        if let Some(kv) = key_version {
            let kv: Version = kv.parse().map_err(|e| {
                lock_error(file, format!("package key '{}': {}", key, e))
            })?;
            if kv != entry.version {
                return Err(lock_error(
                    file,
                    format!(
                        "package key '{}' locks version {}",
                        key, entry.version
                    ),
                ));
            }
        }
        out.push((key.clone(), entry));
    }
    Ok(Lockfile { packages: out })
}

/// Render the lockfile in the shape of §84's example (bare `packages` key,
/// quoted package names, 4-space indent) — plain version strings wherever no
/// §93/§97 identity is recorded, the §97 object form otherwise.
pub fn render_lockfile(lock: &Lockfile) -> String {
    let mut out = String::from("{\n    packages: ");
    if lock.packages.is_empty() {
        out.push_str("{}\n}\n");
        return out;
    }
    out.push_str("{\n");
    for (i, (key, entry)) in lock.packages.iter().enumerate() {
        out.push_str(&format!("        \"{}\": ", key));
        if !entry.has_extra() {
            // Plain §84 form; `name@version` keys (§90) still carry their
            // own concrete version string.
            out.push_str(&format!("\"{}\"", entry.version));
        } else {
            let mut fields = vec![format!("version: \"{}\"", entry.version)];
            if let Some(source) = &entry.source {
                fields.push(format!("source: \"{}\"", source));
            }
            if let Some(commit) = &entry.commit {
                fields.push(format!("commit: \"{}\"", commit));
            }
            if let Some(hash) = &entry.hash {
                fields.push(format!("hash: \"{}\"", hash));
            }
            out.push_str("{\n");
            for (j, field) in fields.iter().enumerate() {
                out.push_str(&format!("            {}", field));
                if j + 1 < fields.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str("        }");
        }
        if i + 1 < lock.packages.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("    }\n}\n");
    out
}

// ---------------------------------------------------------------------------
// Project (§82, §87)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Project {
    /// Absolute project root: the directory holding project.zson.
    pub root: PathBuf,
    pub manifest: Manifest,
    /// `None` when absent or invalid (§84).
    pub lock: Option<Lockfile>,
}

impl Project {
    /// Walk up from `start` (file or directory) until a directory containing
    /// project.zson is found. Never consults the current working directory,
    /// per §87 ("Package resolution tidak bergantung pada current working
    /// directory").
    pub fn discover(start: &Path) -> Result<Option<Project>, PackageError> {
        let mut cur = if start.is_dir() {
            start.to_path_buf()
        } else {
            start
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        };
        if cur.as_os_str().is_empty() {
            cur = PathBuf::from(".");
        }
        if let Ok(abs) = cur.canonicalize() {
            cur = abs;
        }
        loop {
            if cur.join(MANIFEST_FILE).is_file() {
                return Project::open(&cur).map(Some);
            }
            if !cur.pop() {
                return Ok(None);
            }
        }
    }

    pub fn open(root: &Path) -> Result<Project, PackageError> {
        let manifest_path = root.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&manifest_path)
            .map_err(|e| PackageError::Io(format!("Cannot read {}: {}", MANIFEST_FILE, e)))?;
        let manifest = parse_manifest(&text, MANIFEST_FILE)?;

        let lock_path = root.join(LOCK_FILE);
        let lock = match std::fs::read_to_string(&lock_path) {
            Ok(lock_text) => parse_lockfile(&lock_text, LOCK_FILE).ok(),
            Err(_) => None,
        };

        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        Ok(Project {
            root,
            manifest,
            lock,
        })
    }

    pub fn deps_dir(&self) -> PathBuf {
        self.root.join(DEPS_DIR)
    }

    pub fn dep_dir(&self, name: &str) -> PathBuf {
        self.deps_dir().join(name)
    }

    pub fn direct_dependencies(&self) -> Vec<String> {
        self.manifest
            .dependencies
            .iter()
            .map(|(n, _)| n.clone())
            .collect()
    }

    /// Dependency names declared by an installed package (§88/§89).
    /// A package without package.zson declares nothing.
    pub fn package_dependencies(&self, name: &str) -> Result<Vec<String>, PackageError> {
        package_dependencies_at(&self.dep_dir(name))
    }

    /// Every installed copy of `name`: `versions/` sub-directories (§90) or
    /// the single flat copy (§82/§85), each paired with its package.zson
    /// version.
    pub fn installed_copies(&self, name: &str) -> Result<Vec<(Version, PathBuf)>, PackageError> {
        let dir = self.dep_dir(name);
        let versions_dir = dir.join(VERSIONS_DIR);
        if is_versioned_layout(&self.deps_dir(), name) {
            let mut out = Vec::new();
            for entry in std::fs::read_dir(&versions_dir).map_err(io_err)? {
                let entry = entry.map_err(io_err)?;
                if !entry.file_type().map_err(io_err)?.is_dir() {
                    continue;
                }
                let fname = entry.file_name().to_string_lossy().into_owned();
                let version = match fname.parse::<Version>() {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                out.push((version, entry.path()));
            }
            out.sort();
            Ok(out)
        } else if dir.join(PACKAGE_FILE).is_file() {
            match self.installed_version(name)? {
                Some(v) => Ok(vec![(v, dir)]),
                None => Ok(Vec::new()),
            }
        } else {
            Ok(Vec::new())
        }
    }

    /// Installed version of a package, if its metadata says so (§87).
    pub fn installed_version(&self, name: &str) -> Result<Option<Version>, PackageError> {
        let path = self.dep_dir(name).join(PACKAGE_FILE);
        if !path.is_file() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| PackageError::Io(format!("Cannot read {}: {}", PACKAGE_FILE, e)))?;
        let meta = parse_package_meta(&text, &path.display().to_string())?;
        Ok(meta.version.parse().ok())
    }
}

/// Dependency names declared by the `package.zson` inside an installed
/// package directory (§88, §89). A package without package.zson declares
/// nothing.
pub fn package_dependencies_at(dir: &Path) -> Result<Vec<String>, PackageError> {
    let path = dir.join(PACKAGE_FILE);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| PackageError::Io(format!("Cannot read {}: {}", PACKAGE_FILE, e)))?;
    let meta = parse_package_meta(&text, &path.display().to_string())?;
    Ok(meta.dependencies.into_iter().map(|(n, _)| n).collect())
}

/// `true` when `name` is installed in the §90 multi-version layout —
/// `.zsc_m/<name>/versions/<version>/` holding at least one real package
/// (a package.zson inside a directory that merely happens to be called
/// `versions` does not make the layout versioned).
pub fn is_versioned_layout(deps_dir: &Path, name: &str) -> bool {
    let versions = deps_dir.join(name).join(VERSIONS_DIR);
    let entries = match std::fs::read_dir(&versions) {
        Ok(e) => e,
        Err(_) => return false,
    };
    entries.filter_map(Result::ok).any(|e| {
        e.path().join(PACKAGE_FILE).is_file()
            && e.file_name()
                .to_string_lossy()
                .parse::<Version>()
                .is_ok()
    })
}

// ---------------------------------------------------------------------------
// Registry (§95), cache (§98) and source configuration (§99)
// ---------------------------------------------------------------------------

/// Where one version of a package actually lives (§95): a local directory
/// inside the registry, or a Git repository the index points at.
#[derive(Debug, Clone, PartialEq)]
pub enum VersionSource {
    Path(PathBuf),
    Git(GitSource),
}

/// One `versions` entry of a registry index (§95, §97).
#[derive(Debug, Clone)]
struct IndexEntry {
    source: String,
    hash: Option<String>,
}

/// Local package source. Directory layout mirrors §95's registry example
/// (`<registry>/<package>/<version>/`); an optional
/// `<registry>/<package>/index.zson` may additionally declare where each
/// version comes from (§95's index) and its integrity hash (§97).
///
/// The spec fixes no transport for registries (§95 allows a Git-backed
/// registry with no database or HTTP service), so this is the local,
/// testable minimum: directories and index files on disk.
#[derive(Debug, Clone)]
pub struct Registry {
    pub root: PathBuf,
}

impl Registry {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Registry { root: root.into() }
    }

    pub fn package_dir(&self, name: &str, version: &Version) -> PathBuf {
        self.root.join(name).join(version.to_string())
    }

    fn index_path(&self, name: &str) -> PathBuf {
        self.root.join(name).join(INDEX_FILE)
    }

    /// Parse `<registry>/<name>/index.zson`. `Ok(None)` when there is no
    /// index (or no registry root at all — an absent registry contributes no
    /// packages); a present-but-broken index is an ERROR (§95: "index tidak
    /// valid → ERROR").
    fn read_index(&self, name: &str) -> Result<Option<Vec<(Version, IndexEntry)>>, PackageError> {
        let path = self.index_path(name);
        if !path.is_file() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path).map_err(|e| {
            PackageError::Registry(format!("Cannot read {}: {}", path.display(), e))
        })?;
        let v = json::parse_zson(&text).map_err(|detail| {
            PackageError::Registry(format!("{}: {}", path.display(), detail))
        })?;
        let entries = v.as_object().ok_or_else(|| {
            PackageError::Registry(format!("{}: index root must be an object", path.display()))
        })?;
        if let Some(idx_name) = entries.iter().find(|(k, _)| k == "name").map(|(_, v)| v) {
            let s = idx_name.as_str().ok_or_else(|| {
                PackageError::Registry(format!("{}: field 'name' must be a string", path.display()))
            })?;
            if s != name {
                return Err(PackageError::Registry(format!(
                    "{}: index declares package '{}' but is stored as '{}'",
                    path.display(),
                    s,
                    name
                )));
            }
        }
        let versions = entries
            .iter()
            .find(|(k, _)| k == "versions")
            .map(|(_, v)| v)
            .ok_or_else(|| {
                PackageError::Registry(format!(
                    "{}: index is missing field 'versions'",
                    path.display()
                ))
            })?;
        let versions = versions.as_object().ok_or_else(|| {
            PackageError::Registry(format!(
                "{}: field 'versions' must be an object",
                path.display()
            ))
        })?;
        let mut out = Vec::new();
        for (raw_version, value) in versions {
            let version: Version = raw_version.parse().map_err(|e| {
                PackageError::Registry(format!("{}: version '{}': {}", path.display(), raw_version, e))
            })?;
            let entry = match value {
                JsonValue::String(source) => IndexEntry {
                    source: source.clone(),
                    hash: None,
                },
                JsonValue::Object(fields) => {
                    let source = fields
                        .iter()
                        .find(|(k, _)| k == "source")
                        .map(|(_, v)| v)
                        .ok_or_else(|| {
                            PackageError::Registry(format!(
                                "{}: version '{}' is missing field 'source'",
                                path.display(),
                                raw_version
                            ))
                        })?;
                    let source = source.as_str().ok_or_else(|| {
                        PackageError::Registry(format!(
                            "{}: version '{}' field 'source' must be a string",
                            path.display(),
                            raw_version
                        ))
                    })?;
                    let hash = match fields.iter().find(|(k, _)| k == "hash") {
                        Some((_, JsonValue::String(h))) => {
                            if !h.starts_with("sha256:") || h.len() != "sha256:".len() + 64 {
                                return Err(PackageError::Registry(format!(
                                    "{}: version '{}' hash must be 'sha256:<64 hex>' (§97)",
                                    path.display(),
                                    raw_version
                                )));
                            }
                            Some(h.clone())
                        }
                        Some((_, JsonValue::Null)) | None => None,
                        Some((_, _)) => {
                            return Err(PackageError::Registry(format!(
                                "{}: version '{}' field 'hash' must be a string",
                                path.display(),
                                raw_version
                            )))
                        }
                    };
                    IndexEntry {
                        source: source.to_string(),
                        hash,
                    }
                }
                _ => {
                    return Err(PackageError::Registry(format!(
                        "{}: version '{}' must map to a source string or object",
                        path.display(),
                        raw_version
                    )))
                }
            };
            out.push((version, entry));
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(Some(out))
    }

    /// Does this registry know `name` at all (directory or index)?
    pub fn has_package(&self, name: &str) -> bool {
        self.root.join(name).is_dir()
    }

    /// All SemVer versions available for `name`, ascending: the union of the
    /// index and the version directories (§95: "Setiap package dapat memiliki
    /// beberapa versi").
    pub fn versions(&self, name: &str) -> Result<Vec<Version>, PackageError> {
        let mut out: Vec<Version> = Vec::new();
        let dir = self.root.join(name);
        let mut found = false;
        if dir.is_dir() {
            found = true;
            let entries = std::fs::read_dir(&dir).map_err(|e| {
                PackageError::Registry(format!("Cannot read {}: {}", dir.display(), e))
            })?;
            for entry in entries {
                let entry = entry.map_err(|e| {
                    PackageError::Registry(format!("Cannot read {}: {}", dir.display(), e))
                })?;
                if !entry.file_type().map_err(io_err)?.is_dir() {
                    continue;
                }
                let fname = entry.file_name().to_string_lossy().into_owned();
                if let Ok(v) = fname.parse::<Version>() {
                    out.push(v);
                }
            }
        }
        if let Some(index) = self.read_index(name)? {
            found = true;
            for (version, _) in index {
                if !out.contains(&version) {
                    out.push(version);
                }
            }
        }
        if !found {
            return Err(PackageError::NotFound(format!(
                "Package '{}' not found in registry {}",
                name,
                self.root.display()
            )));
        }
        out.sort();
        Ok(out)
    }

    /// Where `name@version` comes from (§95): the index declaration wins
    /// over the local directory, because the index is the registry's own
    /// statement of the source.
    pub fn source(
        &self,
        name: &str,
        version: &Version,
    ) -> Result<Option<VersionSource>, PackageError> {
        if let Some(index) = self.read_index(name)? {
            if let Some((_, entry)) = index.iter().find(|(v, _)| v == version) {
                return Ok(Some(parse_index_source(
                    &entry.source,
                    &self.root.join(name),
                )?));
            }
        }
        let dir = self.package_dir(name, version);
        if dir.is_dir() {
            return Ok(Some(VersionSource::Path(dir)));
        }
        Ok(None)
    }

    /// The raw index source string for `name@version`, recorded in the
    /// lockfile when the registry declares one (§95/§97).
    pub fn index_source(&self, name: &str, version: &Version) -> Option<String> {
        let index = self.read_index(name).ok()??;
        index
            .iter()
            .find(|(v, _)| v == version)
            .map(|(_, e)| e.source.clone())
    }

    /// `sha256:` identity the index records for `name@version` (§97).
    pub fn index_hash(&self, name: &str, version: &Version) -> Option<String> {
        let index = self.read_index(name).ok()??;
        index
            .iter()
            .find(|(v, _)| v == version)
            .and_then(|(_, e)| e.hash.clone())
    }
}

/// Interpret one index `source` value (§95's example uses plain `https://`
/// URLs, §93 requires the `git:` prefix for hand-written git dependencies):
/// `git:`-prefixed or URL-like values are Git sources, anything else is a
/// directory path relative to `<registry>/<name>/`.
fn parse_index_source(raw: &str, base: &Path) -> Result<VersionSource, PackageError> {
    let invalid = |why: &str| {
        PackageError::Registry(format!("invalid index source '{}': {}", raw, why))
    };
    if let Some(rest) = raw.strip_prefix("git:") {
        let g = GitSource::parse(rest).map_err(|e| invalid(&e))?;
        return Ok(VersionSource::Git(g));
    }
    let looks_like_url = raw.contains("://") || raw.ends_with(".git") || raw.contains(".git#");
    if looks_like_url {
        let (url, reference) = match raw.find('#') {
            Some(i) => (&raw[..i], Some(raw[i + 1..].to_string())),
            None => (raw, None),
        };
        if let Some(r) = &reference {
            if r.is_empty() {
                return Err(invalid("empty reference after '#'"));
            }
        }
        return Ok(VersionSource::Git(GitSource {
            url: url.to_string(),
            reference,
        }));
    }
    let path = Path::new(raw);
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    Ok(VersionSource::Path(joined))
}

/// Local package cache (§98): `<cache>/<name>/<version>/` plus a
/// `<cache>/<name>/<version>.sha256` sidecar written together with the copy,
/// so a corrupted or partially written cache entry can be recognised and
/// ignored ("Cache yang tidak valid atau rusak tidak boleh digunakan").
#[derive(Debug, Clone)]
pub struct Cache {
    pub root: PathBuf,
}

impl Cache {
    /// `$ZPM_CACHE` when set, otherwise `~/.zsc_cache` as §98 shows.
    pub fn default_root() -> PathBuf {
        if let Ok(dir) = std::env::var("ZPM_CACHE") {
            if !dir.is_empty() {
                return PathBuf::from(dir);
            }
        }
        match std::env::var("HOME") {
            Ok(home) if !home.is_empty() => PathBuf::from(home).join(".zsc_cache"),
            // Mechanical fallback when HOME is unset: a well-known temp path
            // rather than a write to an unknown location.
            _ => std::env::temp_dir().join("zsc_cache"),
        }
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Cache { root: root.into() }
    }

    pub fn dir(&self, name: &str, version: &Version) -> PathBuf {
        self.root.join(name).join(version.to_string())
    }

    fn sidecar(&self, name: &str, version: &Version) -> PathBuf {
        self.root
            .join(name)
            .join(format!("{}.sha256", version))
    }

    /// The cached copy of `name@version` when it is intact (metadata matches
    /// and the recorded digest still describes the contents), else `None`.
    pub fn entry(&self, name: &str, version: &Version) -> Option<PathBuf> {
        let dir = self.dir(name, version);
        if !dir.is_dir() {
            return None;
        }
        let meta_path = dir.join(PACKAGE_FILE);
        let text = std::fs::read_to_string(&meta_path).ok()?;
        let meta = parse_package_meta(&text, &meta_path.display().to_string()).ok()?;
        if meta.name != name || meta.version != version.to_string() {
            return None;
        }
        let recorded = std::fs::read_to_string(self.sidecar(name, version)).ok()?;
        let computed = hash::hash_dir(&dir).ok()?;
        if recorded.trim() != computed {
            return None;
        }
        Some(dir)
    }

    /// Copy `src` into the cache and record its digest (§98: "Setelah
    /// berhasil diambil dan diverifikasi, package dapat disimpan ke cache").
    pub fn store(
        &self,
        name: &str,
        version: &Version,
        src: &Path,
    ) -> Result<PathBuf, PackageError> {
        let dest = self.dir(name, version);
        if dest.exists() {
            std::fs::remove_dir_all(&dest).map_err(io_err)?;
        }
        copy_dir_skipping_vcs(src, &dest)?;
        let digest = hash::hash_dir(&dest)
            .map_err(|e| PackageError::Integrity(format!("Cannot hash {}: {}", dest.display(), e)))?;
        let sidecar = self.sidecar(name, version);
        if let Some(parent) = sidecar.parent() {
            std::fs::create_dir_all(parent).map_err(io_err)?;
        }
        std::fs::write(&sidecar, format!("{}\n", digest))
            .map_err(|e| PackageError::Io(format!("Cannot write {}: {}", sidecar.display(), e)))?;
        Ok(dest)
    }
}

/// Everything `zpm` can install from (§99): ordered registries plus the
/// local cache. An empty registry list is legal (git-only projects).
#[derive(Debug, Clone)]
pub struct Sources {
    /// Ordered registries: official first, additional ones after it (§95:
    /// "Registry resmi dan registry tambahan dapat digunakan oleh zpm").
    pub registries: Vec<Registry>,
    pub cache: Cache,
}

impl Default for Sources {
    fn default() -> Self {
        Sources::new()
    }
}

impl Sources {
    pub fn new() -> Self {
        Sources {
            registries: Vec::new(),
            cache: Cache::new(Cache::default_root()),
        }
    }

    pub fn registry(mut self, registry: Registry) -> Self {
        self.registries.push(registry);
        self
    }

    pub fn with_registries(mut self, registries: Vec<Registry>) -> Self {
        self.registries = registries;
        self
    }

    pub fn cache(mut self, cache: Cache) -> Self {
        self.cache = cache;
        self
    }
}

// ---------------------------------------------------------------------------
// Installation (§85, §90–§99)
// ---------------------------------------------------------------------------

/// Result of a successful `zpm install` / `zpm update` (§85, §92).
#[derive(Debug, Clone)]
pub struct InstallReport {
    /// Every package in resolution order: (name, version).
    pub resolved: Vec<(String, Version)>,
    /// Subset that was actually copied into `.zsc_m/` this run.
    pub installed: Vec<(String, Version)>,
}

/// Scope identifier of the project itself in the §90 scope graph.
pub const PROJECT_SCOPE: &str = "project";

/// Scope identifier of one package instance (`name@version`): §90 keeps
/// every version inside the scope of the dependency that requires it.
pub fn scope_id(name: &str, version: &Version) -> String {
    format!("{}@{}", name, version)
}

/// The package-name part of a lockfile key (`name` or `name@version`).
fn lock_key_name(key: &str) -> &str {
    match key.find('@') {
        Some(i) => &key[..i],
        None => key,
    }
}

/// One package as the §90 resolver produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPackage {
    pub name: String,
    pub version: Version,
    /// Explicit `git:` requirement this package came from (§93). When a
    /// package is demanded both by Git and by a selector, the explicit
    /// source wins (§99: "explicit source ... memiliki prioritas langsung
    /// terhadap registry").
    pub git: Option<GitSource>,
}

/// Complete resolution: BFS order plus the §90 dependency scopes.
#[derive(Debug, Clone)]
pub struct Resolution {
    pub packages: Vec<ResolvedPackage>,
    /// scope id → (dependency name, resolved version), declaration order.
    pub scopes: HashMap<String, Vec<(String, Version)>>,
}

/// Content fetched from a Git source (§93): where it landed plus the
/// identity the lockfile records (§93 commit, §97 hash).
#[derive(Debug, Clone)]
pub struct GitInfo {
    pub name: String,
    pub version: Version,
    pub commit: Option<String>,
    pub dir: PathBuf,
    pub hash: Option<String>,
}

/// The facts resolution needs from the outside world: registry discovery
/// (§95), package metadata (§96), explicit Git sources (§93) and the
/// integrity hashes of §97.
pub trait Pool {
    /// Versions available for `name` (§95).
    fn candidates(&self, name: &str) -> Result<Vec<Version>, PackageError>;
    /// Dependencies declared by `name@version` (§89, §96).
    fn deps(&self, name: &str, version: &Version) -> Result<Vec<(String, String)>, PackageError>;
    /// Fetch (or look up) a Git source (§93).
    fn git(&self, name: &str, source: &GitSource) -> Result<GitInfo, PackageError>;
    /// Raw registry index source for `name@version` when it points at Git
    /// (recorded in the lockfile, §95/§97).
    fn source(&self, name: &str, version: &Version) -> Result<Option<String>, PackageError>;
    /// `sha256:` hash the registry index records for `name@version` (§97).
    fn hash(&self, name: &str, version: &Version) -> Option<String>;
}

/// Read and parse one `package.zson` (§96).
fn read_meta(path: &Path) -> Result<PackageMeta, PackageError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| PackageError::Io(format!("Cannot read {}: {}", path.display(), e)))?;
    parse_package_meta(&text, &path.display().to_string())
}

/// Validate that `dir` really holds `name@version` (§94: "Nama package pada
/// package.zson harus sesuai dengan nama package yang digunakan oleh
/// dependency resolver"; §96: invalid metadata → ERROR). A directory without
/// `package.zson` is not a package at all → "not found".
fn check_meta(dir: &Path, name: &str, version: &Version) -> Result<PackageMeta, PackageError> {
    let path = dir.join(PACKAGE_FILE);
    if !path.is_file() {
        return Err(PackageError::NotFound(format!(
            "Package '{}' version '{}' not found",
            name, version
        )));
    }
    let meta = read_meta(&path)?;
    if meta.name != name {
        return Err(PackageError::Metadata {
            package: name.to_string(),
            detail: format!(
                "package.zson declares name '{}' but is stored as '{}'",
                meta.name, name
            ),
        });
    }
    if meta.version != version.to_string() {
        return Err(PackageError::Metadata {
            package: name.to_string(),
            detail: format!(
                "package.zson declares version {} but is stored as {}",
                meta.version, version
            ),
        });
    }
    Ok(meta)
}

/// §97: hash the package and compare it with a recorded identity. Returns
/// the computed `sha256:` digest; a different hash is an ERROR. `.git` is
/// not package content and is excluded by `hash_dir`.
fn verify_integrity(dir: &Path, expected: Option<&str>) -> Result<String, PackageError> {
    let digest = hash::hash_dir(dir)
        .map_err(|e| PackageError::Integrity(format!("Cannot hash {}: {}", dir.display(), e)))?;
    if let Some(want) = expected {
        if want != digest {
            return Err(PackageError::Integrity(format!(
                "Integrity check failed for {}: expected {} but computed {}",
                dir.display(),
                want,
                digest
            )));
        }
    }
    Ok(digest)
}

static STAGING_SEQ: AtomicUsize = AtomicUsize::new(0);

/// Scratch directory inside the cache root (§98): a failed download never
/// leaves a half-written package where the cache looks for content.
fn staging_dir(cache_root: &Path) -> PathBuf {
    cache_root.join(format!(
        ".staging-{}-{}",
        std::process::id(),
        STAGING_SEQ.fetch_add(1, Ordering::SeqCst)
    ))
}

/// Copy the tree `src` → `dst`, skipping directories named in `skip` at
/// every level (VCS metadata and zpm's own state are not package content).
fn copy_filtered(src: &Path, dst: &Path, skip: &[&str]) -> Result<(), PackageError> {
    std::fs::create_dir_all(dst).map_err(io_err)?;
    for entry in std::fs::read_dir(src).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
        let file_type = entry.file_type().map_err(io_err)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if file_type.is_dir() && skip.contains(&name.as_str()) {
            continue;
        }
        let to = dst.join(&name);
        if file_type.is_dir() {
            copy_filtered(&entry.path(), &to, skip)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), &to).map_err(io_err)?;
        }
    }
    Ok(())
}

/// Copy into `.zsc_m/` or the cache without Git metadata (§98 stores the
/// package; §97's identity excludes `.git`, so both sides agree).
fn copy_dir_skipping_vcs(src: &Path, dst: &Path) -> Result<(), PackageError> {
    copy_filtered(src, dst, &[".git"])
}

fn detect_cycle(
    roots: &[String],
    edges: &HashMap<String, Vec<String>>,
) -> Result<(), PackageError> {
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }
    let mut color: HashMap<&str, Color> = HashMap::new();
    let mut stack: Vec<String> = Vec::new();

    fn dfs<'a>(
        node: &'a str,
        edges: &'a HashMap<String, Vec<String>>,
        color: &mut HashMap<&'a str, Color>,
        stack: &mut Vec<String>,
    ) -> Result<(), PackageError> {
        match color.get(node).copied().unwrap_or(Color::White) {
            Color::Black => return Ok(()),
            Color::Gray => {
                let start = stack.iter().position(|n| n == node).unwrap_or(0);
                let mut chain: Vec<String> = stack[start..].to_vec();
                chain.push(node.to_string());
                return Err(PackageError::Circular(chain));
            }
            Color::White => {}
        }
        color.insert(node, Color::Gray);
        stack.push(node.to_string());
        if let Some(deps) = edges.get(node) {
            for dep in deps {
                dfs(dep, edges, color, stack)?;
            }
        }
        stack.pop();
        color.insert(node, Color::Black);
        Ok(())
    }

    for root in roots {
        dfs(root, edges, &mut color, &mut stack)?;
    }
    Ok(())
}

fn installed_matches(dir: &Path, name: &str, version: &Version) -> bool {
    let path = dir.join(PACKAGE_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return false,
    };
    match parse_package_meta(&text, &path.display().to_string()) {
        Ok(meta) => meta.name == name && meta.version == version.to_string(),
        Err(_) => false,
    }
}


/// Installation pool (§93, §95, §97, §98): registries for discovery, the
/// cache and registries for content, Git for explicit sources. `lock` is
/// `Some` for `zpm install` (honour §84/§99 pinning) and `None` for
/// `zpm update`, which explicitly replaces locked versions (§92).
pub struct InstallPool<'a> {
    sources: &'a Sources,
    lock: Option<&'a Lockfile>,
    /// `(name, git spec)` → already fetched checkout: resolution and
    /// materialisation together fetch each Git source at most once.
    git_resolved: RefCell<HashMap<(String, String), GitInfo>>,
}

impl<'a> InstallPool<'a> {
    /// Pool for `zpm install`: the project's lockfile pins versions (§84)
    /// plus sources and commits (§93/§97) while they remain valid (§99).
    pub fn new(project: &'a Project, sources: &'a Sources) -> Self {
        InstallPool {
            sources,
            lock: project.lock.as_ref(),
            git_resolved: RefCell::new(HashMap::new()),
        }
    }

    /// Pool for `zpm update`: §92 may replace what the lockfile records.
    pub fn for_update(sources: &'a Sources) -> Self {
        InstallPool {
            sources,
            lock: None,
            git_resolved: RefCell::new(HashMap::new()),
        }
    }

    /// The lockfile entry for exactly this package version, if any.
    fn locked_entry(&self, name: &str, version: &Version) -> Option<&LockEntry> {
        self.lock.and_then(|l| l.entry(name, version))
    }

    /// `sha256:` the registry index records for `name@version` (§97).
    fn index_hash(&self, name: &str, version: &Version) -> Option<String> {
        for reg in &self.sources.registries {
            if let Some(h) = reg.index_hash(name, version) {
                return Some(h);
            }
        }
        None
    }

    /// Raw index source string when the index points at Git (§95/§97): a
    /// plain registry directory is described by the registry itself, so
    /// only a Git source is worth recording as `source` in the lockfile.
    fn index_source(&self, name: &str, version: &Version) -> Result<Option<String>, PackageError> {
        for reg in &self.sources.registries {
            let raw = match reg.index_source(name, version) {
                Some(raw) => raw,
                None => continue,
            };
            if let Some(VersionSource::Git(_)) = reg.source(name, version)? {
                return Ok(Some(raw));
            }
        }
        Ok(None)
    }

    /// §93/§97: obtain `name` from `source`, honouring a locked commit.
    fn fetch_git(&self, name: &str, source: &GitSource) -> Result<GitInfo, PackageError> {
        let spec = source.spec();
        if let Some(info) = self
            .git_resolved
            .borrow()
            .get(&(name.to_string(), spec.clone()))
        {
            return Ok(info.clone());
        }

        // §99: while the locked source and package are still valid the
        // lockfile wins — a cache entry that hashes to the locked identity
        // is used without fetching again.
        let locked: Option<LockEntry> = self
            .lock
            .and_then(|l| l.entry_by_source(&spec))
            .filter(|(key, _)| lock_key_name(key) == name)
            .map(|(_, e)| e.clone());
        if let Some(entry) = &locked {
            if let (Some(commit), Some(hash)) = (&entry.commit, &entry.hash) {
                if let Some(dir) = self.sources.cache.entry(name, &entry.version) {
                    if hash::hash_dir(&dir).ok().as_deref() == Some(hash.as_str()) {
                        let info = GitInfo {
                            name: name.to_string(),
                            version: entry.version.clone(),
                            commit: Some(commit.clone()),
                            dir,
                            hash: Some(hash.clone()),
                        };
                        self.git_resolved
                            .borrow_mut()
                            .insert((name.to_string(), spec), info.clone());
                        return Ok(info);
                    }
                }
            }
        }

        // Fetch into staging so a failure never pollutes the cache (§98).
        let staging = staging_dir(&self.sources.cache.root);
        let outcome = self.fetch_git_into(name, source, locked.as_ref(), &staging);
        let _ = std::fs::remove_dir_all(&staging);
        match outcome {
            Ok(info) => {
                self.git_resolved
                    .borrow_mut()
                    .insert((name.to_string(), spec), info.clone());
                Ok(info)
            }
            Err(e) => Err(e),
        }
    }

    fn fetch_git_into(
        &self,
        name: &str,
        source: &GitSource,
        locked: Option<&LockEntry>,
        staging: &Path,
    ) -> Result<GitInfo, PackageError> {
        let checkout = git::checkout(source, staging)?;
        // §97: the recorded commit is the source identity — make the clone
        // match it exactly before anything else is checked.
        let mut commit = checkout.commit.clone();
        if let Some(want) = locked.and_then(|e| e.commit.as_ref()) {
            commit = git::checkout_commit(staging, want)?;
        }
        // A lock entry pins the version too; without one the package
        // declares its own (§96).
        let version = locked
            .map(|e| e.version.clone())
            .unwrap_or_else(|| checkout.version.clone());
        // §94: package.zson's name must match the dependency's name;
        // §97: content must match a recorded hash.
        check_meta(staging, name, &version)?;
        let digest = verify_integrity(staging, locked.and_then(|e| e.hash.as_deref()))?;
        let dir = self.sources.cache.store(name, &version, staging)?;
        Ok(GitInfo {
            name: name.to_string(),
            version,
            commit: Some(commit),
            dir,
            hash: Some(digest),
        })
    }
}

impl Pool for InstallPool<'_> {
    fn candidates(&self, name: &str) -> Result<Vec<Version>, PackageError> {
        let mut out: Vec<Version> = Vec::new();
        let mut found = false;
        for reg in &self.sources.registries {
            if !reg.has_package(name) {
                continue;
            }
            found = true;
            for v in reg.versions(name)? {
                if !out.contains(&v) {
                    out.push(v);
                }
            }
        }
        if !found {
            return Err(PackageError::NotFound(format!(
                "Package '{}' not found in any registry",
                name
            )));
        }
        out.sort();
        Ok(out)
    }

    fn deps(&self, name: &str, version: &Version) -> Result<Vec<(String, String)>, PackageError> {
        // A Git package already fetched during resolution.
        let memo_dir = {
            let memo = self.git_resolved.borrow();
            memo.values()
                .find(|i| i.name == name && i.version == *version)
                .map(|i| i.dir.clone())
        };
        if let Some(dir) = memo_dir {
            let path = dir.join(PACKAGE_FILE);
            if path.is_file() {
                let meta = read_meta(&path)?;
                return Ok(meta.dependencies);
            }
        }
        // §99 default order: Local Cache → Registry.
        if let Some(dir) = self.sources.cache.entry(name, version) {
            let path = dir.join(PACKAGE_FILE);
            if path.is_file() {
                // `Cache::entry` already matched package.zson's name/version.
                let meta = read_meta(&path)?;
                return Ok(meta.dependencies);
            }
        }
        for reg in &self.sources.registries {
            match reg.source(name, version)? {
                Some(VersionSource::Path(dir)) => {
                    if !dir.join(PACKAGE_FILE).is_file() {
                        // No metadata here; the next source may have it.
                        continue;
                    }
                    let meta = check_meta(&dir, name, version)?;
                    return Ok(meta.dependencies);
                }
                Some(VersionSource::Git(g)) => {
                    let info = self.fetch_git(name, &g)?;
                    if info.version != *version {
                        return Err(PackageError::Metadata {
                            package: name.to_string(),
                            detail: format!(
                                "registry indexes {} but the Git package is {}",
                                version, info.version
                            ),
                        });
                    }
                    let path = info.dir.join(PACKAGE_FILE);
                    let meta = read_meta(&path)?;
                    return Ok(meta.dependencies);
                }
                None => continue,
            }
        }
        Err(PackageError::NotFound(format!(
            "Package '{}' version '{}' not found",
            name, version
        )))
    }

    fn git(&self, name: &str, source: &GitSource) -> Result<GitInfo, PackageError> {
        self.fetch_git(name, source)
    }

    fn source(&self, name: &str, version: &Version) -> Result<Option<String>, PackageError> {
        self.index_source(name, version)
    }

    fn hash(&self, name: &str, version: &Version) -> Option<String> {
        self.index_hash(name, version)
    }
}

/// Pool over what is already installed (§87, §90): used by the module
/// loader to recover the dependency scopes of a §90 multi-version install
/// without consulting registries or the network.
pub struct InstalledPool<'a> {
    project: &'a Project,
    lock: Option<&'a Lockfile>,
}

impl<'a> InstalledPool<'a> {
    pub fn new(project: &'a Project) -> Self {
        InstalledPool {
            project,
            lock: project.lock.as_ref(),
        }
    }

    /// The installed directory of exactly `name@version`, if present.
    fn dir_for(&self, name: &str, version: &Version) -> Option<PathBuf> {
        let copies = self.project.installed_copies(name).ok()?;
        copies
            .into_iter()
            .find(|(v, _)| v == version)
            .map(|(_, dir)| dir)
    }
}

impl Pool for InstalledPool<'_> {
    fn candidates(&self, name: &str) -> Result<Vec<Version>, PackageError> {
        let copies = self.project.installed_copies(name)?;
        if !copies.is_empty() {
            let mut out: Vec<Version> = copies.into_iter().map(|(v, _)| v).collect();
            out.sort();
            return Ok(out);
        }
        // A package installed without package.zson declares nothing (§89),
        // so its version can only come from the lockfile.
        if self.project.dep_dir(name).is_dir() {
            if let Some(v) = self.lock.and_then(|l| l.get(name)) {
                return Ok(vec![v.clone()]);
            }
        }
        Err(PackageError::NotFound(format!(
            "Package '{}' is not installed",
            name
        )))
    }

    fn deps(&self, name: &str, version: &Version) -> Result<Vec<(String, String)>, PackageError> {
        let dir = match self.dir_for(name, version) {
            Some(dir) => dir,
            None => {
                let flat = self.project.dep_dir(name);
                if flat.is_dir() && !flat.join(PACKAGE_FILE).is_file() {
                    // Metadata-free install (§89): declares no dependencies.
                    return Ok(Vec::new());
                }
                return Err(PackageError::NotFound(format!(
                    "Package '{}' version '{}' is not installed",
                    name, version
                )));
            }
        };
        let path = dir.join(PACKAGE_FILE);
        if !path.is_file() {
            return Ok(Vec::new());
        }
        let meta = check_meta(&dir, name, version)?;
        Ok(meta.dependencies)
    }

    fn git(&self, name: &str, source: &GitSource) -> Result<GitInfo, PackageError> {
        let spec = source.spec();
        let locked = self
            .lock
            .and_then(|l| l.entry_by_source(&spec))
            .filter(|(key, _)| lock_key_name(key) == name)
            .map(|(_, e)| e.clone());
        if let Some(entry) = locked {
            let dir = self.dir_for(name, &entry.version).ok_or_else(|| {
                PackageError::NotFound(format!(
                    "Package '{}' version '{}' is not installed — run `zpm install`",
                    name, entry.version
                ))
            })?;
            return Ok(GitInfo {
                name: name.to_string(),
                version: entry.version,
                commit: entry.commit,
                dir,
                hash: entry.hash,
            });
        }
        // No lock entry: the single installed copy is the source of truth.
        let copies = self.project.installed_copies(name)?;
        match copies.len() {
            0 => Err(PackageError::NotFound(format!(
                "Package '{}' is not installed",
                name
            ))),
            1 => {
                let (version, dir) = copies.into_iter().next().unwrap();
                let commit = self
                    .lock
                    .and_then(|l| l.entry(name, &version))
                    .and_then(|e| e.commit.clone());
                Ok(GitInfo {
                    name: name.to_string(),
                    version,
                    commit,
                    dir,
                    hash: None,
                })
            }
            _ => Err(PackageError::Resolution(format!(
                "Package '{}' has several installed versions but the lockfile records no git source {}",
                name, spec
            ))),
        }
    }

    fn source(&self, _name: &str, _version: &Version) -> Result<Option<String>, PackageError> {
        Ok(None)
    }

    fn hash(&self, _name: &str, _version: &Version) -> Option<String> {
        None
    }
}


/// A requirement flowing through resolution: `(scope, package, demand)`.
type Req = (String, String, DepReq);

/// §90: "Konflik yang tidak dapat diselesaikan → ERROR" — a requirement set
/// that keeps changing after this many rounds is treated as unresolvable.
const MAX_RESOLUTION_ROUNDS: usize = 100;

/// The project manifest's dependencies as seed requirements (§82).
fn seed_reqs(project: &Project) -> Result<Vec<Req>, PackageError> {
    let mut out = Vec::new();
    for (name, raw) in &project.manifest.dependencies {
        let req = DepReq::parse(raw).map_err(|e| {
            PackageError::Manifest(format!("{}: dependency '{}': {}", MANIFEST_FILE, name, e))
        })?;
        out.push((PROJECT_SCOPE.to_string(), name.clone(), req));
    }
    Ok(out)
}

fn push_unique(packages: &mut Vec<(String, Version)>, name: &str, version: &Version) {
    if !packages.iter().any(|(n, v)| n == name && v == version) {
        packages.push((name.to_string(), version.clone()));
    }
}

/// One round of §90 placement: choose a version for every requirement,
/// then expand the placed packages into their own scopes.
struct Placed {
    /// Unique `(name, version)` pairs in resolution (BFS) order.
    packages: Vec<(String, Version)>,
    /// scope id → its dependencies with the version placed for them.
    scopes: HashMap<String, Vec<(String, Version)>>,
    /// Explicit Git sources behind some placed version (§93, §99).
    git: Vec<(String, Version, GitSource)>,
    /// Requirements for the next round: seed + the placed packages' own
    /// demands (scopes that lost all their requirements drop out).
    reqs: Vec<Req>,
}

fn place(
    pool: &dyn Pool,
    lock: Option<&Lockfile>,
    seed: &[Req],
    reqs: &[Req],
) -> Result<Placed, PackageError> {
    // First-seen package order = resolution (BFS) order (§88's tree).
    let mut names: Vec<String> = Vec::new();
    for (_, name, _) in reqs {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }

    let mut placements: Vec<(String, String, Version)> = Vec::new();
    let mut packages: Vec<(String, Version)> = Vec::new();
    let mut git_sources: Vec<(String, Version, GitSource)> = Vec::new();

    for name in &names {
        let demands: Vec<&Req> = reqs.iter().filter(|r| &r.1 == name).collect();
        if demands.is_empty() {
            continue;
        }

        // §90/§99: explicit Git requirements never consult a registry and
        // never merge with selector requirements — each distinct source is
        // its own group; selector demands share a version whenever one
        // satisfies all of them and split only when no version can.
        let mut git_groups: Vec<(GitSource, Vec<String>)> = Vec::new();
        let mut sel_groups: Vec<(Vec<Selector>, Vec<String>)> = Vec::new();
        let mut group_of: Vec<(bool, usize)> = Vec::new();
        let mut candidates: Option<Vec<Version>> = None;

        for (scope, _, req) in &demands {
            match req {
                DepReq::Git(g) => {
                    let existing = git_groups.iter().position(|(s, _)| s.spec() == g.spec());
                    let idx = match existing {
                        Some(gi) => {
                            let scopes = &mut git_groups[gi].1;
                            if !scopes.contains(scope) {
                                scopes.push(scope.clone());
                            }
                            gi
                        }
                        None => {
                            git_groups.push((g.clone(), vec![scope.clone()]));
                            git_groups.len() - 1
                        }
                    };
                    group_of.push((true, idx));
                }
                DepReq::Selector(sel) => {
                    if candidates.is_none() {
                        candidates = Some(pool.candidates(name)?);
                    }
                    let cand = candidates.as_ref().unwrap();
                    let mut target = None;
                    for (gi, (sels, _)) in sel_groups.iter().enumerate() {
                        let mut combo = sels.clone();
                        combo.push(sel.clone());
                        if highest_matching(cand, &combo).is_some() {
                            target = Some(gi);
                            break;
                        }
                    }
                    let idx = match target {
                        Some(gi) => {
                            let (sels, scopes) = &mut sel_groups[gi];
                            sels.push(sel.clone());
                            if !scopes.contains(scope) {
                                scopes.push(scope.clone());
                            }
                            gi
                        }
                        None => {
                            sel_groups.push((vec![sel.clone()], vec![scope.clone()]));
                            sel_groups.len() - 1
                        }
                    };
                    group_of.push((false, idx));
                }
            }
        }

        // §83/§84/§90: each group's version — the highest that satisfies
        // every selector of the group, with a still-valid lockfile entry
        // winning over newer releases ("zpm install" never auto-upgrades).
        let mut sel_versions: Vec<Version> = Vec::new();
        for (sels, _) in &sel_groups {
            let cand = candidates.as_ref().expect("loaded with the first selector");
            sel_versions.push(compute_version(name, sels, cand, lock)?);
        }
        let mut git_versions: Vec<Version> = Vec::new();
        for (source, _) in &git_groups {
            let info = pool.git(name, source)?;
            if info.name != *name {
                return Err(PackageError::Metadata {
                    package: name.to_string(),
                    detail: format!(
                        "git source declares package '{}' but '{}' requires it",
                        info.name, name
                    ),
                });
            }
            git_versions.push(info.version.clone());
            let version = info.version;
            if !git_sources
                .iter()
                .any(|(n, v, _)| n == name && *v == version)
            {
                git_sources.push((name.clone(), version, source.clone()));
            }
        }

        // Record placements for every demand plus the flattened package list
        // (selector groups first, then Git groups — deterministic order).
        for (di, (scope, _, req)) in demands.iter().enumerate() {
            let version = match req {
                DepReq::Git(_) => &git_versions[group_of[di].1],
                DepReq::Selector(_) => &sel_versions[group_of[di].1],
            };
            placements.push((scope.clone(), name.clone(), version.clone()));
        }
        for v in &sel_versions {
            push_unique(&mut packages, name, v);
        }
        for v in &git_versions {
            push_unique(&mut packages, name, v);
        }
    }

    // Expand every placed package into its own scope (§90: each version
    // stays inside the scope of the dependency that requires it; §100:
    // "Dependency transitif memiliki scope sendiri").
    let mut scopes: HashMap<String, Vec<(String, Version)>> = HashMap::new();
    let mut next: Vec<Req> = seed.to_vec();
    for (pname, pversion) in &packages {
        let sid = scope_id(pname, pversion);
        if scopes.contains_key(&sid) {
            continue;
        }
        let deps = pool.deps(pname, pversion)?;
        let mut entries = Vec::new();
        let mut complete = true;
        for (dname, draw) in deps {
            let dreq = DepReq::parse(&draw).map_err(|e| PackageError::Metadata {
                package: pname.clone(),
                detail: format!("dependency '{}': {}", draw, e),
            })?;
            match placements
                .iter()
                .find(|(s, n, _)| s == &sid && n == &dname)
                .map(|(_, _, v)| v.clone())
            {
                Some(dversion) => {
                    entries.push((dname.clone(), dversion));
                    next.push((sid.clone(), dname, dreq));
                }
                None => {
                    // Not placed yet (first round): the demand flows to the
                    // next round, which places it; at convergence every
                    // demand is placed, so the final scopes are complete.
                    complete = false;
                    next.push((sid.clone(), dname, dreq));
                }
            }
        }
        if complete {
            scopes.insert(sid, entries);
        }
    }

    Ok(Placed {
        packages,
        scopes,
        git: git_sources,
        reqs: next,
    })
}

/// §83 + §84 + §90: highest version satisfying all selectors of one group;
/// a still-valid lockfile version wins over newer releases (§92: "zpm
/// install tidak melakukan upgrade otomatis"; §99: lockfile priority while
/// the locked package is still valid).
fn compute_version(
    name: &str,
    selectors: &[Selector],
    candidates: &[Version],
    lock: Option<&Lockfile>,
) -> Result<Version, PackageError> {
    if let Some(lock) = lock {
        let locked: Vec<Version> = lock
            .entries_for(name)
            .into_iter()
            .map(|e| e.version.clone())
            .collect();
        if let Some(best) = candidates
            .iter()
            .cloned()
            .filter(|v| locked.contains(v))
            .filter(|v| selectors.iter().all(|s| s.matches(v)))
            .max()
        {
            return Ok(best);
        }
    }
    highest_matching(candidates, selectors).cloned().ok_or_else(|| {
        PackageError::NoVersion {
            package: name.to_string(),
            selectors: selectors
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(", "),
        }
    })
}

/// §88: "Circular dependency antar-package → ERROR" — evaluated on the §90
/// scope graph, where every package version carries its own scope.
fn check_scope_cycles(scopes: &HashMap<String, Vec<(String, Version)>>) -> Result<(), PackageError> {
    let mut edges: HashMap<String, Vec<String>> = HashMap::new();
    let mut others: Vec<String> = Vec::new();
    for (sid, deps) in scopes {
        edges.insert(
            sid.clone(),
            deps.iter().map(|(n, v)| scope_id(n, v)).collect(),
        );
        if sid != PROJECT_SCOPE {
            others.push(sid.clone());
        }
    }
    others.sort();
    let mut roots = vec![PROJECT_SCOPE.to_string()];
    roots.extend(others);
    detect_cycle(&roots, &edges)
}

/// Run the §90 fixpoint: place → expand → repeat until the requirement set
/// stops changing, then check the scope graph for cycles (§88).
fn resolve_graph(
    project: &Project,
    pool: &dyn Pool,
    lock: Option<&Lockfile>,
) -> Result<Resolution, PackageError> {
    let seed = seed_reqs(project)?;
    let mut reqs = seed.clone();
    let placed;
    let mut rounds = 0usize;
    loop {
        rounds += 1;
        if rounds > MAX_RESOLUTION_ROUNDS {
            return Err(PackageError::Resolution(
                "dependency resolution did not converge".to_string(),
            ));
        }
        let round = place(pool, lock, &seed, &reqs)?;
        if round.reqs == reqs {
            placed = round;
            break;
        }
        reqs = round.reqs;
    }
    check_scope_cycles(&placed.scopes)?;

    let packages = placed
        .packages
        .into_iter()
        .map(|(name, version)| {
            let source = placed
                .git
                .iter()
                .find(|(n, v, _)| *n == name && *v == version)
                .map(|(_, _, g)| g.clone());
            ResolvedPackage {
                name,
                version,
                git: source,
            }
        })
        .collect();
    Ok(Resolution {
        packages,
        scopes: placed.scopes,
    })
}

/// Resolve `root`'s dependencies against the configured sources (§82 → §95).
pub fn resolve(root: &Path, sources: &Sources) -> Result<Resolution, PackageError> {
    let project = Project::open(root)?;
    let pool = InstallPool::new(&project, sources);
    resolve_graph(&project, &pool, project.lock.as_ref())
}

/// Recover the §90 dependency scopes of what is already installed (§87,
/// §90) — the module loader's view of a multi-version `.zsc_m/`.
pub fn resolve_installed(project: &Project) -> Result<Resolution, PackageError> {
    let pool = InstalledPool::new(project);
    resolve_graph(project, &pool, project.lock.as_ref())
}


/// Content located for one resolved package: a cache/registry directory or
/// a Git checkout, verified against the recorded identity (§97).
struct Acquired {
    dir: PathBuf,
    /// Git checkout: the concrete commit the lockfile records (§93).
    commit: Option<String>,
    /// Digest of the content (§97).
    hash: String,
}

/// §99 source priority for one package: explicit Git first (it bypasses
/// the registry by design, §95/§99), then the local cache, then the
/// registries in configuration order — every step gated by §97 verification
/// when an identity is recorded, and by §96/§94 metadata validation.
fn acquire(
    pool: &InstallPool<'_>,
    pkg: &ResolvedPackage,
    old: Option<&LockEntry>,
) -> Result<Acquired, PackageError> {
    let name = &pkg.name;
    let version = &pkg.version;
    // §97: a hash recorded in the lockfile wins over the registry index.
    let expected: Option<String> = old
        .and_then(|e| e.hash.clone())
        .or_else(|| pool.hash(name, version));

    // Explicit Git source (§93, §99): never looked up in a registry.
    if let Some(g) = &pkg.git {
        let info = pool.git(name, g)?;
        let digest = verify_integrity(&info.dir, expected.as_deref())?;
        return Ok(Acquired {
            dir: info.dir,
            commit: info.commit,
            hash: digest,
        });
    }

    // §99 default order: Local Cache → Registry. A cache entry that fails
    // verification is unusable (§98) and does not override the lockfile
    // (§98: "Cache yang tidak sesuai dengan lockfile tidak boleh
    // menggantikan dependency yang dikunci") — fall through to the source.
    if let Some(dir) = pool.sources.cache.entry(name, version) {
        if let Ok(digest) = verify_integrity(&dir, expected.as_deref()) {
            return Ok(Acquired {
                dir,
                commit: old.and_then(|e| e.commit.clone()),
                hash: digest,
            });
        }
    }

    for reg in &pool.sources.registries {
        let source = match reg.source(name, version)? {
            Some(s) => s,
            None => continue,
        };
        match source {
            VersionSource::Path(dir) => {
                if !dir.is_dir() {
                    continue;
                }
                // §96 first (a directory without valid package.zson is not a
                // package), then §97: "Hash yang berbeda → ERROR".
                check_meta(&dir, name, version)?;
                let digest = verify_integrity(&dir, expected.as_deref())?;
                // §98: "Setelah berhasil diambil dan diverifikasi, package
                // dapat disimpan ke cache" — the verified registry copy
                // becomes reusable content for the next install.
                let cached = pool.sources.cache.store(name, version, &dir)?;
                return Ok(Acquired {
                    dir: cached,
                    commit: old.and_then(|e| e.commit.clone()),
                    hash: digest,
                });
            }
            VersionSource::Git(g) => {
                let info = pool.git(name, &g)?;
                if info.version != *version {
                    return Err(PackageError::Metadata {
                        package: name.clone(),
                        detail: format!(
                            "registry indexes {} but the Git package is {}",
                            version, info.version
                        ),
                    });
                }
                let digest = verify_integrity(&info.dir, expected.as_deref())?;
                return Ok(Acquired {
                    dir: info.dir,
                    commit: info.commit,
                    hash: digest,
                });
            }
        }
    }
    Err(PackageError::NotFound(format!(
        "Package '{}' version '{}' not found",
        name, version
    )))
}

/// The lockfile identity to record for `name@version`: the explicit Git
/// spec, else the index source (§95/§97), preferring what the previous
/// lockfile already knew (§84 stability).
fn lock_identity(
    pool: &InstallPool<'_>,
    pkg: &ResolvedPackage,
    acquired: Option<&Acquired>,
    old: Option<&LockEntry>,
) -> Result<LockEntry, PackageError> {
    let source = if let Some(g) = &pkg.git {
        Some(g.spec())
    } else if let Some(s) = old.and_then(|e| e.source.clone()) {
        Some(s)
    } else {
        pool.source(&pkg.name, &pkg.version)?
    };
    let commit = acquired
        .and_then(|a| a.commit.clone())
        .or_else(|| old.and_then(|e| e.commit.clone()));
    // A hash is recorded only when the source provides one (§97: "Registry
    // dapat menyediakan hash"; the §97 lockfile example shows source +
    // commit + hash for Git). Without one the entry keeps §84's plain
    // `"name": "version"` form instead of inventing a digest the spec does
    // not ask for.
    let hash = match old.and_then(|e| e.hash.clone()) {
        Some(h) => Some(h),
        None => match pool.hash(&pkg.name, &pkg.version) {
            Some(h) => Some(h),
            None if pkg.git.is_some() => acquired.map(|a| a.hash.clone()),
            None => None,
        },
    };
    Ok(LockEntry {
        version: pkg.version.clone(),
        source,
        commit,
        hash,
    })
}

/// `zpm install` (§85): resolve (honouring the lockfile, §84/§99) → verify
/// (§97) → populate `.zsc_m/` (§90 layout) → prune what is no longer part
/// of the tree (§91) → write the lockfile (§84) — the lockfile only after
/// a completely successful run.
pub fn install(root: &Path, sources: &Sources) -> Result<InstallReport, PackageError> {
    let project = Project::open(root)?;
    let pool = InstallPool::new(&project, sources);
    let resolution = resolve_graph(&project, &pool, project.lock.as_ref())?;
    materialize(&project, &pool, &resolution)
}

/// `zpm update` (§92): the same pipeline without lockfile pinning, so the
/// newest versions that still match the selectors are chosen (transitive
/// dependencies included) and re-locked. With nothing newer available the
/// resolution stays at the version already in use.
pub fn update(root: &Path, sources: &Sources) -> Result<InstallReport, PackageError> {
    let project = Project::open(root)?;
    let pool = InstallPool::for_update(sources);
    let resolution = resolve_graph(&project, &pool, None)?;
    materialize(&project, &pool, &resolution)
}

fn materialize(
    project: &Project,
    pool: &InstallPool<'_>,
    resolution: &Resolution,
) -> Result<InstallReport, PackageError> {
    // Versions per name decide each package's layout (§90): several
    // coexisting versions live under `versions/<v>/`, a single copy keeps
    // the flat §82/§85 layout.
    let mut versions_by_name: HashMap<&str, Vec<&Version>> = HashMap::new();
    for pkg in &resolution.packages {
        versions_by_name
            .entry(pkg.name.as_str())
            .or_default()
            .push(&pkg.version);
    }

    // Layout conversion: remove the shape that no longer applies before
    // copying, so a package that just gained or lost a conflicting version
    // ends up in the right place.
    for (name, versions) in &versions_by_name {
        let dir = project.dep_dir(name);
        if versions.len() > 1 {
            if dir.is_dir() {
                for entry in std::fs::read_dir(&dir).map_err(io_err)? {
                    let entry = entry.map_err(io_err)?;
                    if entry.file_name() != VERSIONS_DIR {
                        let path = entry.path();
                        if path.is_dir() {
                            std::fs::remove_dir_all(&path).map_err(io_err)?;
                        } else {
                            std::fs::remove_file(&path).map_err(io_err)?;
                        }
                    }
                }
            }
        } else {
            let versions_dir = dir.join(VERSIONS_DIR);
            if versions_dir.is_dir() {
                std::fs::remove_dir_all(&versions_dir).map_err(io_err)?;
            }
        }
    }

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for pkg in &resolution.packages {
        *counts.entry(pkg.name.as_str()).or_insert(0) += 1;
    }

    let mut resolved: Vec<(String, Version)> = Vec::new();
    let mut installed: Vec<(String, Version)> = Vec::new();
    let mut lock_entries: Vec<(String, LockEntry)> = Vec::new();

    for pkg in &resolution.packages {
        let versioned = counts.get(pkg.name.as_str()).copied().unwrap_or(0) > 1;
        let dest = if versioned {
            project
                .dep_dir(&pkg.name)
                .join(VERSIONS_DIR)
                .join(pkg.version.to_string())
        } else {
            project.dep_dir(&pkg.name)
        };
        resolved.push((pkg.name.clone(), pkg.version.clone()));

        let old = pool.locked_entry(&pkg.name, &pkg.version).cloned();

        if installed_matches(&dest, &pkg.name, &pkg.version) {
            // §85: already present at that version — no re-download, and a
            // local edit inside `.zsc_m/` survives.
            let entry = lock_identity(pool, pkg, None, old.as_ref())?;
            lock_entries.push((lock_key(&pkg.name, &pkg.version, &counts), entry));
            continue;
        }

        let acquired = acquire(pool, pkg, old.as_ref())?;
        if dest.exists() {
            if dest.is_dir() {
                std::fs::remove_dir_all(&dest).map_err(io_err)?;
            } else {
                std::fs::remove_file(&dest).map_err(io_err)?;
            }
        }
        copy_dir_skipping_vcs(&acquired.dir, &dest)?;
        installed.push((pkg.name.clone(), pkg.version.clone()));
        let entry = lock_identity(pool, pkg, Some(&acquired), old.as_ref())?;
        lock_entries.push((lock_key(&pkg.name, &pkg.version, &counts), entry));
    }

    if !resolved.is_empty() {
        std::fs::create_dir_all(project.deps_dir()).map_err(io_err)?;
    }

    // §85/§91: `.zsc_m/` holds exactly what the resolution needs.
    prune(project, resolution)?;

    let lock = Lockfile {
        packages: lock_entries,
    };
    std::fs::write(project.root.join(LOCK_FILE), render_lockfile(&lock))
        .map_err(|e| PackageError::Io(format!("Cannot write {}: {}", LOCK_FILE, e)))?;

    Ok(InstallReport {
        resolved,
        installed,
    })
}

/// Lockfile key: `name` while one version is installed, `name@version`
/// once §90 keeps several (mechanical key choice: names may not contain
/// `@`).
fn lock_key(name: &str, version: &Version, counts: &HashMap<&str, usize>) -> String {
    match counts.get(name).copied().unwrap_or(1) {
        1 | 0 => name.to_string(),
        _ => format!("{}@{}", name, version),
    }
}

/// §85/§91: remove packages that are no longer part of the dependency tree
/// and stale versions of the packages that stay; a package still required
/// by another dependency is never removed (§91).
fn prune(project: &Project, resolution: &Resolution) -> Result<(), PackageError> {
    let deps_dir = project.deps_dir();
    if !deps_dir.is_dir() {
        return Ok(());
    }
    let mut wanted: HashMap<&str, Vec<&Version>> = HashMap::new();
    for pkg in &resolution.packages {
        wanted
            .entry(pkg.name.as_str())
            .or_default()
            .push(&pkg.version);
    }
    for entry in std::fs::read_dir(&deps_dir).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
        if !entry.file_type().map_err(io_err)?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        match wanted.get(name.as_str()) {
            None => {
                std::fs::remove_dir_all(entry.path()).map_err(io_err)?;
            }
            Some(versions) if versions.len() > 1 => {
                let versions_dir = entry.path().join(VERSIONS_DIR);
                if versions_dir.is_dir() {
                    for v_entry in std::fs::read_dir(&versions_dir).map_err(io_err)? {
                        let v_entry = v_entry.map_err(io_err)?;
                        let keep = v_entry
                            .file_name()
                            .to_string_lossy()
                            .parse::<Version>()
                            .map(|v| versions.iter().any(|w| **w == v))
                            .unwrap_or(false);
                        if !keep {
                            let path = v_entry.path();
                            if path.is_dir() {
                                std::fs::remove_dir_all(&path).map_err(io_err)?;
                            } else {
                                std::fs::remove_file(&path).map_err(io_err)?;
                            }
                        }
                    }
                }
            }
            Some(_) => {}
        }
    }
    Ok(())
}

/// §91: drop `name` from `project.zson`'s `dependencies`. Removing a
/// package that is not a direct dependency of the project → ERROR.
pub fn undeclare_dependency(root: &Path, name: &str) -> Result<(), PackageError> {
    let project = Project::open(root)?;
    if !project
        .manifest
        .dependencies
        .iter()
        .any(|(n, _)| n == name)
    {
        return Err(PackageError::NotDeclared(format!(
            "Package '{}' is not a direct dependency of the project",
            name
        )));
    }
    let path = root.join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| PackageError::Io(format!("Cannot read {}: {}", MANIFEST_FILE, e)))?;
    let mut v = json::parse_zson(&text).map_err(|detail| PackageError::Parse {
        file: MANIFEST_FILE.to_string(),
        detail,
    })?;
    match &mut v {
        JsonValue::Object(fields) => {
            let deps = fields
                .iter_mut()
                .find(|(k, _)| k == "dependencies")
                .map(|(_, v)| v);
            match deps {
                Some(JsonValue::Object(entries)) => {
                    entries.retain(|(n, _)| n != name);
                }
                _ => {
                    return Err(PackageError::Manifest(format!(
                        "{}: field 'dependencies' must be an object",
                        MANIFEST_FILE
                    )))
                }
            }
        }
        _ => {
            return Err(PackageError::Manifest(format!(
                "{}: root must be an object",
                MANIFEST_FILE
            )))
        }
    }
    std::fs::write(&path, json::render_zson(&v).map_err(|e| {
        PackageError::Manifest(format!("{}: {}", MANIFEST_FILE, e))
    })?)
    .map_err(|e| PackageError::Io(format!("Cannot write {}: {}", MANIFEST_FILE, e)))?;
    Ok(())
}

/// `zpm remove <package>` (§91): undeclare the direct dependency, then
/// re-resolve so packages that are no longer required leave `.zsc_m/`,
/// still-required transitive dependencies stay, and the lockfile is
/// updated. Project source files are never touched (§91).
pub fn remove_dependency(
    root: &Path,
    name: &str,
    sources: &Sources,
) -> Result<InstallReport, PackageError> {
    undeclare_dependency(root, name)?;
    install(root, sources)
}

/// Declare `name: requirement` in `project.zson` (§100: "Dependency harus
/// dideklarasikan secara eksplisit"). Returns `true` when the declaration
/// was added; an existing declaration with the same requirement is left
/// untouched (`false`), one with a different requirement is not silently
/// replaced.
pub fn add_dependency(root: &Path, name: &str, requirement: &str) -> Result<bool, PackageError> {
    safe_package_name(name)
        .map_err(|e| PackageError::Manifest(format!("{}: {}", MANIFEST_FILE, e)))?;
    DepReq::parse(requirement).map_err(|e| {
        PackageError::Manifest(format!("{}: dependency '{}': {}", MANIFEST_FILE, name, e))
    })?;
    let project = Project::open(root)?;
    if let Some((_, existing)) = project
        .manifest
        .dependencies
        .iter()
        .find(|(n, _)| n == name)
    {
        return Ok(existing == requirement);
    }
    let path = root.join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| PackageError::Io(format!("Cannot read {}: {}", MANIFEST_FILE, e)))?;
    let mut v = json::parse_zson(&text).map_err(|detail| PackageError::Parse {
        file: MANIFEST_FILE.to_string(),
        detail,
    })?;
    match &mut v {
        JsonValue::Object(fields) => {
            let deps = fields
                .iter_mut()
                .find(|(k, _)| k == "dependencies")
                .map(|(_, v)| v);
            match deps {
                Some(JsonValue::Object(entries)) => {
                    entries.push((
                        name.to_string(),
                        JsonValue::String(requirement.to_string()),
                    ));
                }
                _ => {
                    return Err(PackageError::Manifest(format!(
                        "{}: field 'dependencies' must be an object",
                        MANIFEST_FILE
                    )))
                }
            }
        }
        _ => {
            return Err(PackageError::Manifest(format!(
                "{}: root must be an object",
                MANIFEST_FILE
            )))
        }
    }
    std::fs::write(&path, json::render_zson(&v).map_err(|e| {
        PackageError::Manifest(format!("{}: {}", MANIFEST_FILE, e))
    })?)
    .map_err(|e| PackageError::Io(format!("Cannot write {}: {}", MANIFEST_FILE, e)))?;
    Ok(true)
}

/// Walk up from `start` to the nearest directory holding a
/// `package.zson` (§94: every publishable package has one).
fn find_package_dir(start: &Path) -> Result<PathBuf, PackageError> {
    let mut cur = if start.is_dir() {
        start.to_path_buf()
    } else {
        start
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    };
    loop {
        if cur.join(PACKAGE_FILE).is_file() {
            return Ok(cur);
        }
        if !cur.pop() {
            break;
        }
    }
    Err(PackageError::Publish(format!(
        "No {} found in {} or any parent directory",
        PACKAGE_FILE,
        start.display()
    )))
}

/// `zpm publish` (§94): validate the package and distribute it into the
/// registry directory — the local, testable form of "Package dapat
/// dipublikasikan melalui Git repository" (§94), since the spec defines no
/// publishing transport. When the package lives in a Git repository the
/// release is marked with the `v<version>` tag (§94's tag example) and an
/// existing tag means the version was already published (§94: a published
/// version may not change). Returns the directory the package landed in.
pub fn publish(dir: &Path, registry: Option<&Path>) -> Result<PathBuf, PackageError> {
    let pkg_dir = find_package_dir(dir)?;
    let meta = read_meta(&pkg_dir.join(PACKAGE_FILE))?;
    let version: Version = meta.version.parse().map_err(|e| {
        PackageError::Publish(format!("{}: field 'version': {}", PACKAGE_FILE, e))
    })?;
    let registry = match registry {
        Some(r) => r.to_path_buf(),
        None => {
            return Err(PackageError::Publish(
                "publish requires a registry (pass --registry <dir>)".to_string(),
            ))
        }
    };

    // §94: "Versi package tidak boleh diubah setelah versi tersebut
    // dipublikasikan" — a version present in the registry (directory or
    // index) is final.
    if Registry::new(&registry).source(&meta.name, &version)?.is_some() {
        return Err(PackageError::Publish(format!(
            "Version {} of '{}' is already published",
            version, meta.name
        )));
    }
    let repo = git::repository_root(&pkg_dir);
    let tag = format!("v{}", version);
    if let Some(repo) = &repo {
        if git::has_tag(repo, &tag)? {
            return Err(PackageError::Publish(format!(
                "Tag {} already exists — version {} was already published",
                tag, version
            )));
        }
    }

    // zpm/VCS state is not package content (§96's structure shows source
    // files only); never copy the registry into itself when it lives
    // inside the package tree.
    let registry_component: Option<String> = registry
        .strip_prefix(&pkg_dir)
        .ok()
        .and_then(|rel| rel.components().next())
        .map(|c| c.as_os_str().to_string_lossy().into_owned());
    let mut skip: Vec<&str> = vec![DEPS_DIR, CACHE_DIR, ".git"];
    if let Some(c) = &registry_component {
        skip.push(c.as_str());
    }

    let target = registry.join(&meta.name).join(version.to_string());
    copy_filtered(&pkg_dir, &target, &skip)?;
    if let Some(repo) = &repo {
        git::create_tag(repo, &tag)?;
    }
    Ok(target)
}

/// Read `(name, version)` from a Git source without keeping the checkout
/// (§95: an explicit Git source is discovered directly, without a registry).
pub fn probe_git(source: &GitSource) -> Result<(String, Version), PackageError> {
    let staging = staging_dir(&std::env::temp_dir());
    let outcome = (|| -> Result<(String, Version), PackageError> {
        let checkout = git::checkout(source, &staging)?;
        let meta = read_meta(&staging.join(PACKAGE_FILE))?;
        Ok((meta.name, checkout.version))
    })();
    let _ = std::fs::remove_dir_all(&staging);
    outcome
}

// ---------------------------------------------------------------------------
// Project scripts (§86)
// ---------------------------------------------------------------------------

/// Shell-quote one argument for appending to a script command.
fn shell_quote(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./:=@%+,#".contains(&b));
    if plain {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

/// Run `zpm run <name>` (§86): the command text is executed verbatim from
/// the project root and `extra_args` are appended as additional arguments.
/// Returns the child's exit code.
pub fn run_script(
    root: &Path,
    name: &str,
    extra_args: &[String],
    path_override: Option<String>,
) -> Result<i32, PackageError> {
    let project = Project::open(root)?;
    let command = project
        .manifest
        .scripts
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, c)| c.clone())
        .ok_or_else(|| PackageError::ScriptNotFound(name.to_string()))?;

    // The command text itself is never rewritten (§86); extra arguments are
    // appended so `zpm run start -- --debug` reaches the script's command.
    let script = if extra_args.is_empty() {
        command
    } else {
        let mut s = command;
        for arg in extra_args {
            s.push(' ');
            s.push_str(&shell_quote(arg));
        }
        s
    };

    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c").arg(script);
    cmd.current_dir(root);
    if let Some(path) = path_override {
        cmd.env("PATH", path);
    }

    let status = cmd.status().map_err(|e| {
        PackageError::Command(format!("Cannot run script '{}': {}", name, e))
    })?;
    Ok(status.code().unwrap_or(-1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SEQ: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "zeroscript_job017_{}_{}_{}",
            tag,
            std::process::id(),
            n
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    /// Add `<registry>/<name>/<version>/` with a package.zson plus extra files.
    fn add_pkg(
        registry: &Path,
        name: &str,
        version: &str,
        deps: &[(&str, &str)],
        files: &[(&str, &str)],
    ) {
        let dir = registry.join(name).join(version);
        std::fs::create_dir_all(&dir).unwrap();
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
        write_file(&dir.join(PACKAGE_FILE), &meta);
        for (rel, content) in files {
            write_file(&dir.join(rel), content);
        }
    }

    fn new_project(tag: &str, manifest: &str) -> PathBuf {
        let dir = temp_dir(tag);
        write_file(&dir.join(MANIFEST_FILE), manifest);
        dir
    }

    /// Per-test `Sources`: the given registry plus a project-local cache,
    /// so no test ever reads the shared `$HOME` cache (§98 isolation).
    fn sources(root: &Path, registry: &Path) -> Sources {
        Sources::new()
            .registry(Registry::new(registry))
            .cache(Cache::new(root.join(CACHE_DIR)))
    }

    fn install_err(root: &Path, registry: &Path) -> String {
        install(root, &sources(root, registry))
            .unwrap_err()
            .to_string()
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    fn lock_entries(root: &Path) -> Vec<(String, String)> {
        let text = read(&root.join(LOCK_FILE));
        let lock = parse_lockfile(&text, LOCK_FILE).unwrap();
        lock.packages
            .iter()
            .map(|(n, v)| (n.clone(), v.to_string()))
            .collect()
    }

    // --- 1. project.zson parsing (testing requirement 1) ---

    #[test]
    fn test_job017_manifest_valid_minimal() {
        let m = parse_manifest(
            r#"{ name: "my-app", version: "1.0.0", dependencies: {} }"#,
            MANIFEST_FILE,
        )
        .unwrap();
        assert_eq!(m.name, "my-app");
        assert_eq!(m.version, "1.0.0");
        assert!(m.dependencies.is_empty());
        assert!(m.scripts.is_empty());
    }

    #[test]
    fn test_job017_manifest_spec_example() {
        // §82 example, verbatim.
        let m = parse_manifest(
            r#"{
    name: "my-app",
    version: "1.0.0",

    dependencies: {
        "http-utils": "1.2.0",
        "database": "2.0.1"
    }
}"#,
            MANIFEST_FILE,
        )
        .unwrap();
        assert_eq!(
            m.dependencies,
            vec![
                ("http-utils".to_string(), "1.2.0".to_string()),
                ("database".to_string(), "2.0.1".to_string())
            ]
        );
    }

    #[test]
    fn test_job017_manifest_missing_required_field() {
        for (src, field) in [
            (r#"{ version: "1.0.0", dependencies: {} }"#, "name"),
            (r#"{ name: "a", dependencies: {} }"#, "version"),
            (r#"{ name: "a", version: "1.0.0" }"#, "dependencies"),
        ] {
            let err = parse_manifest(src, MANIFEST_FILE).unwrap_err().to_string();
            assert!(err.contains(&format!("missing required field '{}'", field)), "{}", err);
        }
    }

    #[test]
    fn test_job017_manifest_wrong_type() {
        let err = parse_manifest(
            r#"{ name: 1, version: "1.0.0", dependencies: {} }"#,
            MANIFEST_FILE,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("must be a string"), "{}", err);

        let err = parse_manifest(
            r#"{ name: "a", version: "1.0.0", dependencies: [] }"#,
            MANIFEST_FILE,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("must be an object"), "{}", err);

        let err = parse_manifest(
            r#"{ name: "a", version: "1.0.0", dependencies: { p: 2 } }"#,
            MANIFEST_FILE,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("must be a string"), "{}", err);
    }

    #[test]
    fn test_job017_manifest_root_must_be_object() {
        let err = parse_manifest("[1,2]", MANIFEST_FILE).unwrap_err().to_string();
        assert!(err.contains("root must be an object"), "{}", err);
        let err = parse_manifest("not zson at all", MANIFEST_FILE)
            .unwrap_err()
            .to_string();
        assert!(err.contains("project.zson"), "{}", err);
    }

    #[test]
    fn test_job017_manifest_invalid_selector_rejected_at_parse() {
        let err = parse_manifest(
            r#"{ name: "a", version: "1.0.0", dependencies: { p: "^1.0.0" } }"#,
            MANIFEST_FILE,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("invalid version selector"), "{}", err);
    }

    #[test]
    fn test_job017_manifest_unsafe_dependency_name_rejected() {
        for bad in [r#"../evil"#, r#"a/b"#, "..", "."] {
            let src = format!(
                r#"{{ name: "a", version: "1.0.0", dependencies: {{ "{}": "1.0.0" }} }}"#,
                bad
            );
            assert!(
                parse_manifest(&src, MANIFEST_FILE).is_err(),
                "expected '{}' to be rejected",
                bad
            );
        }
    }

    #[test]
    fn test_job017_manifest_scripts_validation() {
        // §86 example.
        let m = parse_manifest(
            r#"{
    name: "my-app",
    version: "1.0.0",
    scripts: {
        start: "zsc index.zs"
    },
    dependencies: { "http-utils": "1.2.x" }
}"#,
            MANIFEST_FILE,
        )
        .unwrap();
        assert_eq!(m.scripts, vec![("start".to_string(), "zsc index.zs".to_string())]);

        let err = parse_manifest(
            r#"{ name: "a", version: "1.0.0", dependencies: {}, scripts: [] }"#,
            MANIFEST_FILE,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("'scripts' must be an object"), "{}", err);

        let err = parse_manifest(
            r#"{ name: "a", version: "1.0.0", dependencies: {}, scripts: { s: 1 } }"#,
            MANIFEST_FILE,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("must be a string command"), "{}", err);
    }

    #[test]
    fn test_job017_manifest_unknown_fields_ignored() {
        let m = parse_manifest(
            r#"{ name: "a", version: "1.0.0", dependencies: {}, author: "x", license: "MIT" }"#,
            MANIFEST_FILE,
        )
        .unwrap();
        assert_eq!(m.name, "a");
    }

    // --- package.zson (§96) ---

    #[test]
    fn test_job017_package_meta_valid() {
        // §94 example.
        let meta = parse_package_meta(
            r#"{
    name: "http-utils",
    version: "1.2.0",
    description: "HTTP utilities for ZeroScript",

    dependencies: {}
}"#,
            PACKAGE_FILE,
        )
        .unwrap();
        assert_eq!(meta.name, "http-utils");
        assert_eq!(meta.version, "1.2.0");
        assert!(meta.dependencies.is_empty());
    }

    #[test]
    fn test_job017_package_meta_dependencies_optional() {
        let meta = parse_package_meta(
            r#"{ name: "a", version: "1.0.0" }"#,
            PACKAGE_FILE,
        )
        .unwrap();
        assert!(meta.dependencies.is_empty());
    }

    #[test]
    fn test_job017_package_meta_version_must_be_semver() {
        // §96: "version" harus berupa string versi SemVer yang valid.
        let err = parse_package_meta(
            r#"{ name: "a", version: "banana" }"#,
            PACKAGE_FILE,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("field 'version'"), "{}", err);

        let err = parse_package_meta(r#"{ name: "a" }"#, PACKAGE_FILE)
            .unwrap_err()
            .to_string();
        assert!(err.contains("missing required field 'version'"), "{}", err);
    }

    // --- lockfile (§84) ---

    #[test]
    fn test_job017_lockfile_spec_example_roundtrip() {
        let text = r#"{ packages: { "http-utils": "1.2.10", "database": "2.0.0" } }"#;
        let lock = parse_lockfile(text, LOCK_FILE).unwrap();
        assert_eq!(lock.get("http-utils").unwrap().to_string(), "1.2.10");
        assert_eq!(lock.get("database").unwrap().to_string(), "2.0.0");

        let rendered = render_lockfile(&lock);
        let again = parse_lockfile(&rendered, LOCK_FILE).unwrap();
        assert_eq!(again, lock);
        assert_eq!(
            rendered,
            "{\n    packages: {\n        \"http-utils\": \"1.2.10\",\n        \"database\": \"2.0.0\"\n    }\n}\n"
        );
    }

    #[test]
    fn test_job017_lockfile_invalid_shapes() {
        for bad in [
            "",
            "{}",
            r#"{ packages: [] }"#,
            r#"{ packages: { a: 1 } }"#,
            r#"{ packages: { a: "not-a-version" } }"#,
            r#"{ packages: { "../x": "1.0.0" } }"#,
        ] {
            assert!(
                parse_lockfile(bad, LOCK_FILE).is_err(),
                "expected '{}' to be rejected",
                bad
            );
        }
    }

    #[test]
    fn test_job017_render_empty_lockfile() {
        let rendered = render_lockfile(&Lockfile { packages: vec![] });
        assert_eq!(rendered, "{\n    packages: {}\n}\n");
        parse_lockfile(&rendered, LOCK_FILE).unwrap();
    }

    // --- 2. zpm install full flow (testing requirement 2) ---

    #[test]
    fn test_job017_install_full_flow() {
        let reg = temp_dir("reg_flow");
        // §83 available versions.
        for v in ["1.2.0", "1.2.5", "1.2.10", "1.3.2"] {
            add_pkg(&reg, "http-utils", v, &[], &[("client.zs", "export const V = 1")]);
        }
        for v in ["1.0.0", "1.5.0", "2.0.0"] {
            add_pkg(&reg, "database", v, &[], &[("db.zs", "export const D = 1")]);
        }
        let root = new_project(
            "flow",
            r#"{
    name: "my-app",
    version: "1.0.0",

    dependencies: {
        "http-utils": "1.2.x",
        "database": "x"
    }
}"#,
        );

        let report = install(&root, &sources(&root, &reg)).unwrap();

        // §83: resolver memilih http-utils → 1.2.10, database → 2.0.0.
        assert_eq!(
            report.resolved,
            vec![
                ("http-utils".to_string(), "1.2.10".parse().unwrap()),
                ("database".to_string(), "2.0.0".parse().unwrap())
            ]
        );
        assert_eq!(report.installed.len(), 2);

        // Package contents landed in .zsc_m/ (§82/§85).
        assert!(root.join(".zsc_m/http-utils/client.zs").is_file());
        assert!(root.join(".zsc_m/database/db.zs").is_file());
        assert!(root.join(".zsc_m/http-utils/package.zson").is_file());

        // Lockfile records the concrete versions.
        assert_eq!(
            lock_entries(&root),
            vec![
                ("http-utils".to_string(), "1.2.10".to_string()),
                ("database".to_string(), "2.0.0".to_string())
            ]
        );
    }

    // --- 3. valid lockfile pins the version (testing requirement 3) ---

    #[test]
    fn test_job017_install_honours_lockfile() {
        let reg = temp_dir("reg_lock");
        for v in ["1.2.0", "1.2.5", "1.2.10"] {
            add_pkg(&reg, "http-utils", v, &[], &[("client.zs", "export const V = 1")]);
        }
        let root = new_project(
            "lock",
            r#"{ name: "my-app", version: "1.0.0", dependencies: { "http-utils": "1.2.x" } }"#,
        );
        install(&root, &sources(&root, &reg)).unwrap();
        assert_eq!(lock_entries(&root)[0].1, "1.2.10");

        // A newer matching version appears in the registry …
        add_pkg(&reg, "http-utils", "1.2.11", &[], &[("client.zs", "export const V = 2")]);
        // … and install must NOT upgrade (§84, §92).
        let report = install(&root, &sources(&root, &reg)).unwrap();
        assert_eq!(report.resolved[0].1.to_string(), "1.2.10");
        assert_eq!(lock_entries(&root)[0].1, "1.2.10");
        assert!(report.installed.is_empty(), "no re-download");
    }

    #[test]
    fn test_job017_stale_lockfile_entry_is_re_resolved() {
        // Locked version no longer satisfies the selector → resolve anew.
        let reg = temp_dir("reg_stale");
        add_pkg(&reg, "http-utils", "1.2.10", &[], &[("client.zs", "x")]);
        add_pkg(&reg, "http-utils", "2.0.0", &[], &[("client.zs", "x")]);
        let root = new_project(
            "stale",
            r#"{ name: "my-app", version: "1.0.0", dependencies: { "http-utils": "1.2.x" } }"#,
        );
        install(&root, &sources(&root, &reg)).unwrap();
        // Simulate a selector change without re-running install.
        write_file(
            &root.join(MANIFEST_FILE),
            r#"{ name: "my-app", version: "1.0.0", dependencies: { "http-utils": "2.x" } }"#,
        );
        let report = install(&root, &sources(&root, &reg)).unwrap();
        assert_eq!(report.resolved[0].1.to_string(), "2.0.0");
    }

    #[test]
    fn test_job017_invalid_lockfile_is_ignored_not_fatal() {
        let reg = temp_dir("reg_badlock");
        add_pkg(&reg, "http-utils", "1.2.5", &[], &[("client.zs", "x")]);
        add_pkg(&reg, "http-utils", "1.2.10", &[], &[("client.zs", "x")]);
        let root = new_project(
            "badlock",
            r#"{ name: "my-app", version: "1.0.0", dependencies: { "http-utils": "1.2.x" } }"#,
        );
        write_file(&root.join(LOCK_FILE), "{ this is not a lockfile");
        let report = install(&root, &sources(&root, &reg)).unwrap();
        assert_eq!(report.resolved[0].1.to_string(), "1.2.10");
        // …and a valid lockfile replaces it.
        assert_eq!(lock_entries(&root)[0].1, "1.2.10");
    }

    // --- 4. lockfile removed → newest matching version (testing requirement 4) ---

    #[test]
    fn test_job017_lockfile_removed_resolves_newest() {
        let reg = temp_dir("reg_rm");
        for v in ["1.2.0", "1.2.5", "1.2.10"] {
            add_pkg(&reg, "http-utils", v, &[], &[("client.zs", "x")]);
        }
        let root = new_project(
            "rmlock",
            r#"{ name: "my-app", version: "1.0.0", dependencies: { "http-utils": "1.2.x" } }"#,
        );
        install(&root, &sources(&root, &reg)).unwrap();
        assert_eq!(lock_entries(&root)[0].1, "1.2.10");

        add_pkg(&reg, "http-utils", "1.2.11", &[], &[("client.zs", "x")]);
        std::fs::remove_file(root.join(LOCK_FILE)).unwrap();
        let report = install(&root, &sources(&root, &reg)).unwrap();
        assert_eq!(report.resolved[0].1.to_string(), "1.2.11");
    }

    // --- 5. selector matching (covered in version.rs, exercised through install) ---

    #[test]
    fn test_job017_install_selector_matrix() {
        let reg = temp_dir("reg_sel");
        for v in ["1.2.0", "1.2.5", "1.2.10", "1.3.2", "2.0.0"] {
            add_pkg(&reg, "pkg", v, &[], &[("m.zs", "x")]);
        }
        for (selector, expected) in [
            ("1.2.0", "1.2.0"),
            ("1.2.x", "1.2.10"),
            ("1.x", "1.3.2"),
            ("x", "2.0.0"),
        ] {
            let root = new_project(
                "sel",
                &format!(
                    r#"{{ name: "app", version: "1.0.0", dependencies: {{ "pkg": "{}" }} }}"#,
                    selector
                ),
            );
            let report = install(&root, &sources(&root, &reg)).unwrap();
            assert_eq!(
                report.resolved[0].1.to_string(),
                expected,
                "selector {}",
                selector
            );
        }
    }

    // --- 6. package not found / no matching version → ERROR ---

    #[test]
    fn test_job017_install_package_not_found_error() {
        let reg = temp_dir("reg_missing");
        let root = new_project(
            "missing",
            r#"{ name: "app", version: "1.0.0", dependencies: { "nope": "1.0.0" } }"#,
        );
        let err = install_err(&root, &reg);
        assert!(err.starts_with("ERROR:"), "{}", err);
        assert!(err.contains("not found"), "{}", err);
        assert!(!root.join(LOCK_FILE).exists(), "no lockfile on failure");
    }

    #[test]
    fn test_job017_install_no_version_matches_selector_error() {
        let reg = temp_dir("reg_nover");
        add_pkg(&reg, "pkg", "1.2.0", &[], &[("m.zs", "x")]);
        let root = new_project(
            "nover",
            r#"{ name: "app", version: "1.0.0", dependencies: { "pkg": "2.x" } }"#,
        );
        let err = install_err(&root, &reg);
        assert!(err.contains("No version of 'pkg' matches selector '2.x'"), "{}", err);
    }

    #[test]
    fn test_job017_install_missing_manifest_error() {
        let dir = temp_dir("nomanifest");
        let reg = temp_dir("reg_nomanifest");
        let err = install(&dir, &sources(&dir, &reg)).unwrap_err().to_string();
        assert!(err.contains(MANIFEST_FILE), "{}", err);
    }

    // --- 7/8. already installed → no re-download (§85) ---

    #[test]
    fn test_job017_installed_package_not_downloaded_again() {
        let reg = temp_dir("reg_skip");
        add_pkg(&reg, "pkg", "1.0.0", &[], &[("m.zs", "export const M = 1")]);
        let root = new_project(
            "skip",
            r#"{ name: "app", version: "1.0.0", dependencies: { "pkg": "1.0.0" } }"#,
        );
        let report = install(&root, &sources(&root, &reg)).unwrap();
        assert_eq!(report.installed.len(), 1);

        // Local edit inside .zsc_m/ must survive a re-install (no re-download).
        write_file(&root.join(".zsc_m/pkg/m.zs"), "export const M = 999");
        let report = install(&root, &sources(&root, &reg)).unwrap();
        assert!(report.installed.is_empty());
        assert_eq!(
            read(&root.join(".zsc_m/pkg/m.zs")),
            "export const M = 999"
        );
    }

    #[test]
    fn test_job017_version_change_replaces_installed_copy() {
        let reg = temp_dir("reg_replace");
        add_pkg(&reg, "pkg", "1.0.0", &[], &[("m.zs", "v1")]);
        add_pkg(&reg, "pkg", "2.0.0", &[], &[("m.zs", "v2")]);
        let root = new_project(
            "replace",
            r#"{ name: "app", version: "1.0.0", dependencies: { "pkg": "1.x" } }"#,
        );
        install(&root, &sources(&root, &reg)).unwrap();
        assert_eq!(read(&root.join(".zsc_m/pkg/m.zs")), "v1");

        write_file(
            &root.join(MANIFEST_FILE),
            r#"{ name: "app", version: "1.0.0", dependencies: { "pkg": "2.x" } }"#,
        );
        install(&root, &sources(&root, &reg)).unwrap();
        assert_eq!(read(&root.join(".zsc_m/pkg/m.zs")), "v2");
    }

    // --- 10. dependency tree (§88) ---

    #[test]
    fn test_job017_nested_dependency_tree() {
        // §88 example: my-app → http-utils → url-parser,
        //                my-app → database → crypto-utils
        let reg = temp_dir("reg_tree");
        add_pkg(&reg, "http-utils", "1.2.0", &[("url-parser", "1.x")], &[("client.zs", "x")]);
        add_pkg(&reg, "url-parser", "1.5.0", &[], &[("parse.zs", "x")]);
        add_pkg(&reg, "database", "2.0.0", &[("crypto-utils", "x")], &[("db.zs", "x")]);
        add_pkg(&reg, "crypto-utils", "1.0.0", &[], &[("crypto.zs", "x")]);
        let root = new_project(
            "tree",
            r#"{
    name: "my-app",
    version: "1.0.0",
    dependencies: {
        "http-utils": "1.2.x",
        "database": "2.x"
    }
}"#,
        );

        let report = install(&root, &sources(&root, &reg)).unwrap();
        // Resolution order: manifest order first, then each package's own
        // dependencies in declaration order (breadth-first, deterministic).
        assert_eq!(
            report
                .resolved
                .iter()
                .map(|(n, v)| format!("{}@{}", n, v))
                .collect::<Vec<_>>(),
            vec![
                "http-utils@1.2.0",
                "database@2.0.0",
                "url-parser@1.5.0",
                "crypto-utils@1.0.0"
            ]
        );
        for name in ["http-utils", "url-parser", "database", "crypto-utils"] {
            assert!(
                root.join(".zsc_m").join(name).join(PACKAGE_FILE).is_file(),
                "{} not installed",
                name
            );
        }
        let locked = lock_entries(&root);
        assert_eq!(locked.len(), 4, "lockfile records transitive deps: {:?}", locked);
    }

    #[test]
    fn test_job017_shared_dependency_installed_once() {
        // §88: "Resolver harus menghindari instalasi duplikat ketika versi
        // yang sama dapat digunakan bersama."
        let reg = temp_dir("reg_shared");
        add_pkg(&reg, "package-a", "1.0.0", &[("shared-utils", "1.2.x")], &[("a.zs", "x")]);
        add_pkg(&reg, "package-b", "1.0.0", &[("shared-utils", "1.2.x")], &[("b.zs", "x")]);
        add_pkg(&reg, "shared-utils", "1.2.5", &[], &[("s.zs", "x")]);
        add_pkg(&reg, "shared-utils", "1.2.10", &[], &[("s.zs", "x")]);
        let root = new_project(
            "shared",
            r#"{ name: "app", version: "1.0.0", dependencies: { "package-a": "1.0.0", "package-b": "1.0.0" } }"#,
        );
        let report = install(&root, &sources(&root, &reg)).unwrap();
        let shared: Vec<_> = report
            .resolved
            .iter()
            .filter(|(n, _)| n == "shared-utils")
            .collect();
        assert_eq!(shared.len(), 1);
        assert_eq!(shared[0].1.to_string(), "1.2.10");
        // Flat layout: exactly one copy under .zsc_m/.
        assert_eq!(
            std::fs::read_dir(root.join(".zsc_m")).unwrap().count(),
            3
        );
    }

    #[test]
    fn test_job017_shared_version_intersects_selectors() {
        // package-a → shared-utils "1.x", package-b → "1.2.5":
        // the shared version must satisfy both, not just the first.
        let reg = temp_dir("reg_intersect");
        add_pkg(&reg, "package-a", "1.0.0", &[("shared-utils", "1.x")], &[("a.zs", "x")]);
        add_pkg(&reg, "package-b", "1.0.0", &[("shared-utils", "1.2.5")], &[("b.zs", "x")]);
        add_pkg(&reg, "shared-utils", "1.2.5", &[], &[("s.zs", "x")]);
        add_pkg(&reg, "shared-utils", "1.3.2", &[], &[("s.zs", "x")]);
        let root = new_project(
            "intersect",
            r#"{ name: "app", version: "1.0.0", dependencies: { "package-a": "1.0.0", "package-b": "1.0.0" } }"#,
        );
        let report = install(&root, &sources(&root, &reg)).unwrap();
        let shared: Vec<_> = report
            .resolved
            .iter()
            .filter(|(n, _)| n == "shared-utils")
            .collect();
        assert_eq!(shared.len(), 1);
        assert_eq!(shared[0].1.to_string(), "1.2.5");
    }

    #[test]
    fn test_job018_disjoint_selectors_resolve_to_two_versions() {
        // §90: disjoint selectors are no longer an error (supersedes Job
        // 017's expectation, which reflected the flat one-copy install) —
        // both versions of the shared package are installed side by side
        // under `versions/<v>/`, each in its own scope.
        let reg = temp_dir("reg_conflict_v2");
        add_pkg(&reg, "package-a", "1.0.0", &[("shared-utils", "1.x")], &[("a.zs", "x")]);
        add_pkg(&reg, "package-b", "1.0.0", &[("shared-utils", "2.x")], &[("b.zs", "x")]);
        add_pkg(&reg, "shared-utils", "1.5.0", &[], &[("s.zs", "x")]);
        add_pkg(&reg, "shared-utils", "2.1.0", &[], &[("s.zs", "x")]);
        let root = new_project(
            "conflict",
            r#"{ name: "app", version: "1.0.0", dependencies: { "package-a": "1.0.0", "package-b": "1.0.0" } }"#,
        );
        let report = install(&root, &sources(&root, &reg)).unwrap();
        // BFS resolution order: both roots, then one entry per version.
        let resolved: Vec<String> = report
            .resolved
            .iter()
            .map(|(n, v)| format!("{}@{}", n, v))
            .collect();
        assert_eq!(
            resolved,
            vec![
                "package-a@1.0.0".to_string(),
                "package-b@1.0.0".to_string(),
                "shared-utils@1.5.0".to_string(),
                "shared-utils@2.1.0".to_string(),
            ]
        );
        // §90 layout: a name with several versions lives under versions/<v>/.
        let base = root.join(DEPS_DIR).join("shared-utils");
        assert!(base.join(VERSIONS_DIR).join("1.5.0").join("s.zs").is_file());
        assert!(base.join(VERSIONS_DIR).join("2.1.0").join("s.zs").is_file());
        assert!(!base.join("s.zs").exists());
        assert!(root.join(DEPS_DIR).join("package-a").join("a.zs").is_file());
        assert!(root.join(DEPS_DIR).join("package-b").join("b.zs").is_file());
        // §84: both copies are recorded under their versioned keys.
        let lock = read(&root.join(LOCK_FILE));
        assert!(lock.contains("\"shared-utils@1.5.0\""), "{}", lock);
        assert!(lock.contains("\"shared-utils@2.1.0\""), "{}", lock);
    }

    #[test]
    fn test_job017_circular_package_dependency_is_error() {
        // §88: "Circular dependency antar-package → ERROR."
        let reg = temp_dir("reg_circ");
        add_pkg(&reg, "a", "1.0.0", &[("b", "1.x")], &[("a.zs", "x")]);
        add_pkg(&reg, "b", "1.0.0", &[("a", "1.x")], &[("b.zs", "x")]);
        let root = new_project(
            "circ",
            r#"{ name: "app", version: "1.0.0", dependencies: { "a": "1.x" } }"#,
        );
        let err = install_err(&root, &reg);
        assert!(err.contains("Circular package dependency detected"), "{}", err);
        assert!(err.contains("a"), "{}", err);
        assert!(err.contains("b"), "{}", err);
    }

    #[test]
    fn test_job017_self_dependency_is_circular() {
        let reg = temp_dir("reg_self");
        add_pkg(&reg, "a", "1.0.0", &[("a", "1.x")], &[("a.zs", "x")]);
        let root = new_project(
            "selfdep",
            r#"{ name: "app", version: "1.0.0", dependencies: { "a": "1.x" } }"#,
        );
        let err = install_err(&root, &reg);
        assert!(err.contains("Circular package dependency detected"), "{}", err);
    }

    #[test]
    fn test_job017_metadata_mismatch_is_error() {
        // §94: nama package pada package.zson harus sesuai nama resolver.
        let reg = temp_dir("reg_meta");
        let dir = reg.join("pkg").join("1.0.0");
        std::fs::create_dir_all(&dir).unwrap();
        write_file(
            &dir.join(PACKAGE_FILE),
            r#"{ name: "other-name", version: "1.0.0" }"#,
        );
        let root = new_project(
            "meta",
            r#"{ name: "app", version: "1.0.0", dependencies: { "pkg": "1.0.0" } }"#,
        );
        let err = install_err(&root, &reg);
        assert!(err.contains("Invalid package metadata"), "{}", err);

        // Version declared in package.zson must match the directory version.
        let dir = reg.join("pkg2").join("1.0.0");
        std::fs::create_dir_all(&dir).unwrap();
        write_file(
            &dir.join(PACKAGE_FILE),
            r#"{ name: "pkg2", version: "9.9.9" }"#,
        );
        let root = new_project(
            "meta2",
            r#"{ name: "app", version: "1.0.0", dependencies: { "pkg2": "1.0.0" } }"#,
        );
        let err = install_err(&root, &reg);
        assert!(err.contains("Invalid package metadata"), "{}", err);
    }

    #[test]
    fn test_job017_missing_package_zson_in_registry_is_error() {
        let reg = temp_dir("reg_nometa");
        std::fs::create_dir_all(reg.join("pkg").join("1.0.0")).unwrap();
        write_file(&reg.join("pkg/1.0.0/lib.zs"), "x");
        let root = new_project(
            "nometa",
            r#"{ name: "app", version: "1.0.0", dependencies: { "pkg": "1.0.0" } }"#,
        );
        let err = install_err(&root, &reg);
        assert!(err.contains("not found"), "{}", err);
    }

    #[test]
    fn test_job017_install_no_dependencies_writes_empty_lockfile() {
        let root = new_project(
            "nodeps",
            r#"{ name: "app", version: "1.0.0", dependencies: {} }"#,
        );
        let reg = temp_dir("reg_nodeps");
        let report = install(&root, &sources(&root, &reg)).unwrap();
        assert!(report.resolved.is_empty());
        assert!(!root.join(DEPS_DIR).exists());
        assert_eq!(lock_entries(&root), Vec::<(String, String)>::new());
    }

    // --- 9. zpm run <script> (testing requirement 9) ---

    /// Escape a value for embedding inside a ZSON string literal.
    fn zson_str(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    }

    fn project_with_script(tag: &str, script_cmd: &str) -> PathBuf {
        let dir = new_project(
            tag,
            &format!(
                r#"{{ name: "app", version: "1.0.0", scripts: {{ "test": "{}" }}, dependencies: {{}} }}"#,
                zson_str(script_cmd)
            ),
        );
        dir
    }

    #[test]
    fn test_job017_run_script_executes_command() {
        let root = project_with_script("run1", "printf '%s\\n' alpha beta > out.txt");
        let code = run_script(&root, "test", &[], None).unwrap();
        assert_eq!(code, 0);
        assert_eq!(read(&root.join("out.txt")), "alpha\nbeta\n");
    }

    #[test]
    fn test_job017_run_script_unknown_name_is_error() {
        let root = project_with_script("run2", "true");
        let err = run_script(&root, "start", &[], None).unwrap_err().to_string();
        assert!(err.contains("Script not found: start"), "{}", err);
    }

    #[test]
    fn test_job017_run_script_from_project_root() {
        // §86: "Script dijalankan dari project root."
        let root = project_with_script("run3", "test -f project.zson");
        let code = run_script(&root, "test", &[], None).unwrap();
        assert_eq!(code, 0);
    }

    #[test]
    fn test_job017_run_script_forwards_extra_args() {
        // §86: `zpm run start -- --debug` → arguments reach the command.
        let root = project_with_script("run4", "printf '%s\\n' > args.txt");
        let extra = vec!["--debug".to_string(), "x y".to_string(), "it's".to_string()];
        let code = run_script(&root, "test", &extra, None).unwrap();
        assert_eq!(code, 0);
        assert_eq!(read(&root.join("args.txt")), "--debug\nx y\nit's\n");
    }

    #[test]
    fn test_job017_run_script_args_after_redirection_still_reach_command() {
        // Shell redirections are position-independent; appended arguments
        // must land in the command's argv, not be swallowed by `> file`.
        let root = project_with_script("run4b", "printf '%s\\n' > out.txt");
        let code = run_script(&root, "test", &["hello".to_string()], None).unwrap();
        assert_eq!(code, 0);
        assert_eq!(read(&root.join("out.txt")), "hello\n");
    }

    #[test]
    fn test_job017_run_script_without_extra_args_leaves_command_untouched() {
        // §86: "zpm tidak mengubah isi command yang ditulis programmer."
        // `$#` is 0 only when zpm appended nothing to the command.
        let root = project_with_script("run5", "test $# -eq 0 && echo ok > ok.txt");
        let code = run_script(&root, "test", &[], None).unwrap();
        assert_eq!(code, 0);
        assert_eq!(read(&root.join("ok.txt")), "ok\n");
    }

    #[test]
    fn test_job017_run_script_nonzero_exit_code_propagates() {
        let root = project_with_script("run6", "exit 7");
        let code = run_script(&root, "test", &[], None).unwrap();
        assert_eq!(code, 7);
    }

    #[test]
    fn test_job017_run_script_example_shape_with_zsc() {
        // §86's own example: scripts: { start: "zsc index.zs" }
        let root = new_project(
            "run7",
            r#"{
    name: "my-app",
    version: "1.0.0",
    scripts: {
        start: "zsc index.zs"
    },
    dependencies: {}
}"#,
        );
        write_file(&root.join("index.zs"), "print(\"hi\")");

        // Stand-in `zsc` on an injected PATH (tests must not depend on the
        // real binary being installed).
        let bin = temp_dir("bin");
        let zsc = bin.join("zsc");
        write_file(
            &zsc,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > zsc_args.txt\nprintf 'ran' > zsc_ran.txt\n",
        );
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&zsc, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );

        let code = run_script(&root, "start", &[], Some(path)).unwrap();
        assert_eq!(code, 0);
        assert_eq!(read(&root.join("zsc_ran.txt")), "ran");
        assert_eq!(read(&root.join("zsc_args.txt")), "index.zs\n");
    }

    // --- Project discovery (§87: CWD-independent) ---

    #[test]
    fn test_job017_project_discovery_walks_up() {
        let root = new_project("disc", r#"{ name: "app", version: "1.0.0", dependencies: {} }"#);
        write_file(&root.join("src/deep/file.zs"), "print(1)");
        let found = Project::discover(&root.join("src/deep/file.zs")).unwrap().unwrap();
        assert_eq!(found.root, root.canonicalize().unwrap());
    }

    #[test]
    fn test_job017_project_discovery_none_without_manifest() {
        let dir = temp_dir("nodisc");
        write_file(&dir.join("file.zs"), "print(1)");
        assert!(Project::discover(&dir.join("file.zs")).unwrap().is_none());
    }

    #[test]
    fn test_job017_project_discovery_reports_broken_manifest() {
        let root = temp_dir("brokendisc");
        write_file(&root.join(MANIFEST_FILE), "{ nope");
        write_file(&root.join("file.zs"), "print(1)");
        let err = Project::discover(&root.join("file.zs"))
            .unwrap_err()
            .to_string();
        assert!(err.contains(MANIFEST_FILE), "{}", err);
    }

    #[test]
    fn test_job017_project_open_ignores_invalid_lockfile() {
        let root = new_project(
            "lockopt",
            r#"{ name: "app", version: "1.0.0", dependencies: { "p": "1.0.0" } }"#,
        );
        write_file(&root.join(LOCK_FILE), "garbage");
        let project = Project::open(&root).unwrap();
        assert!(project.lock.is_none());
    }
}
