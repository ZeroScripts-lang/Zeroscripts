//! Package & dependency system core (Dependencies.md §81-§89).
//!
//! Layout produced by `zpm install` (§82, §85):
//!
//! ```text
//! my-app/
//! ├── project.zson
//! ├── project.lock.zson
//! └── .zsc_m/
//!     ├── http-utils/
//!     └── database/
//! ```

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::json::{self, JsonValue};
use crate::version::{highest_matching, Selector, Version};

pub const MANIFEST_FILE: &str = "project.zson";
pub const LOCK_FILE: &str = "project.lock.zson";
pub const PACKAGE_FILE: &str = "package.zson";
pub const DEPS_DIR: &str = ".zsc_m";

#[derive(Debug, Clone, PartialEq)]
pub enum PackageError {
    Io(String),
    /// Structural ZSON/JSON parse failure in a `.zson` file.
    Parse { file: String, detail: String },
    /// Manifest field missing or of the wrong type.
    Manifest(String),
    /// Package (or package version) does not exist at its source.
    NotFound(String),
    /// Package exists but no version satisfies the selector (§83).
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
    if name.contains('\0') {
        return Err("package name must not contain NUL".to_string());
    }
    Ok(())
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
        Selector::parse(sel).map_err(|e| {
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
/// `version` must be valid SemVer, `dependencies` optional.
pub fn parse_package_meta(text: &str, file: &str) -> Result<PackageMeta, PackageError> {
    let v = root_object(text, file)?;
    let name = require_string(&v, "name", file)?;
    safe_package_name(&name).map_err(|e| PackageError::Manifest(format!("{}: {}", file, e)))?;
    let version = require_string(&v, "version", file)?;
    let parsed: Version = version.parse().map_err(|e| {
        PackageError::Manifest(format!("{}: field 'version': {}", file, e))
    })?;
    let dependencies = parse_dependencies(&v, file, false)?;
    Ok(PackageMeta {
        name,
        version: parsed.to_string(),
        dependencies,
    })
}

// ---------------------------------------------------------------------------
// Lockfile (§84)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Lockfile {
    /// Resolution order: project dependencies first, then transitive ones.
    pub packages: Vec<(String, Version)>,
}

impl Lockfile {
    pub fn get(&self, name: &str) -> Option<&Version> {
        self.packages
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v)
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
        .ok_or_else(|| {
            PackageError::Parse {
                file: file.to_string(),
                detail: "field 'packages' must be an object".to_string(),
            }
        })?;
    let mut out = Vec::new();
    for (name, value) in packages {
        safe_package_name(name).map_err(|e| PackageError::Parse {
            file: file.to_string(),
            detail: e,
        })?;
        let s = value.as_str().ok_or_else(|| PackageError::Parse {
            file: file.to_string(),
            detail: format!("package '{}' must map to a version string", name),
        })?;
        let version: Version = s.parse().map_err(|e| PackageError::Parse {
            file: file.to_string(),
            detail: format!("package '{}': {}", name, e),
        })?;
        out.push((name.clone(), version));
    }
    Ok(Lockfile { packages: out })
}

/// Render the lockfile in the shape of §84's example (bare `packages` key,
/// quoted package names, 4-space indent).
pub fn render_lockfile(lock: &Lockfile) -> String {
    let mut out = String::from("{\n    packages: ");
    if lock.packages.is_empty() {
        out.push_str("{}\n}\n");
        return out;
    }
    out.push_str("{\n");
    for (i, (name, version)) in lock.packages.iter().enumerate() {
        out.push_str(&format!(
            "        \"{}\": \"{}\"",
            name, version
        ));
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
        let path = self.dep_dir(name).join(PACKAGE_FILE);
        if !path.is_file() {
            return Ok(Vec::new());
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| PackageError::Io(format!("Cannot read {}: {}", PACKAGE_FILE, e)))?;
        let meta = parse_package_meta(&text, &path.display().to_string())?;
        Ok(meta.dependencies.into_iter().map(|(n, _)| n).collect())
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

// ---------------------------------------------------------------------------
// Registry (local filesystem source for Job 017)
// ---------------------------------------------------------------------------

/// Local package source. Directory layout mirrors §95's registry example
/// (`<registry>/<package>/<version>/`). Remote registries are Job 018.
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

    /// All SemVer versions available for `name`, ascending.
    pub fn versions(&self, name: &str) -> Result<Vec<Version>, PackageError> {
        let dir = self.root.join(name);
        if !dir.is_dir() {
            return Err(PackageError::NotFound(format!(
                "Package '{}' not found in registry {}",
                name,
                self.root.display()
            )));
        }
        let mut out = Vec::new();
        let entries = std::fs::read_dir(&dir).map_err(io_err)?;
        for entry in entries {
            let entry = entry.map_err(io_err)?;
            if !entry.file_type().map_err(io_err)?.is_dir() {
                continue;
            }
            let fname = entry.file_name().to_string_lossy().into_owned();
            if let Ok(v) = fname.parse::<Version>() {
                out.push(v);
            }
        }
        out.sort();
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Installation (§85)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct InstallReport {
    /// Every package in resolution order: (name, version).
    pub resolved: Vec<(String, Version)>,
    /// Subset that was actually copied into .zsc_m/ this run.
    pub installed: Vec<(String, Version)>,
}

fn read_meta(path: &Path) -> Result<PackageMeta, PackageError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| PackageError::Io(format!("Cannot read {}: {}", path.display(), e)))?;
    parse_package_meta(&text, &path.display().to_string())
}

/// Metadata for `name@version`: the installed copy when it really is that
/// version, otherwise the registry copy (§85 "tidak perlu di-download ulang").
fn source_meta(
    project: &Project,
    registry: &Registry,
    name: &str,
    version: &Version,
) -> Result<PackageMeta, PackageError> {
    let installed_path = project.dep_dir(name).join(PACKAGE_FILE);
    if installed_path.is_file() {
        if let Ok(text) = std::fs::read_to_string(&installed_path) {
            if let Ok(meta) = parse_package_meta(&text, &installed_path.display().to_string()) {
                if meta.name == name && meta.version == version.to_string() {
                    return Ok(meta);
                }
            }
        }
    }

    let reg_path = registry.package_dir(name, version).join(PACKAGE_FILE);
    if !reg_path.is_file() {
        return Err(PackageError::NotFound(format!(
            "Package '{}' version '{}' not found",
            name, version
        )));
    }
    let meta = read_meta(&reg_path)?;
    if meta.name != name {
        return Err(PackageError::Metadata {
            package: name.to_string(),
            detail: format!("package.zson declares name '{}' but is stored as '{}'", meta.name, name),
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

fn choose_version(
    project: &Project,
    registry: &Registry,
    name: &str,
    selectors: &[Selector],
) -> Result<Version, PackageError> {
    // §84: a still-valid locked version wins — no automatic upgrade.
    if let Some(locked) = project.lock.as_ref().and_then(|l| l.get(name)) {
        if selectors.iter().all(|s| s.matches(locked)) {
            return Ok(locked.clone());
        }
    }
    let available = registry.versions(name)?;
    highest_matching(&available, selectors).cloned().ok_or_else(|| {
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

fn copy_dir(src: &Path, dst: &Path) -> Result<(), PackageError> {
    std::fs::create_dir_all(dst).map_err(io_err)?;
    for entry in std::fs::read_dir(src).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
        let file_type = entry.file_type().map_err(io_err)?;
        let to = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), &to).map_err(io_err)?;
        }
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

/// `zpm install` (§85): read manifest → honour lockfile → resolve versions →
/// copy into `.zsc_m/` → write lockfile.
pub fn install(root: &Path, registry: &Registry) -> Result<InstallReport, PackageError> {
    let project = Project::open(root)?;

    // Requirement accumulation: name → all selectors seen so far.
    let mut reqs: HashMap<String, Vec<Selector>> = HashMap::new();
    let mut versions: HashMap<String, Version> = HashMap::new();
    let mut resolved_order: Vec<String> = Vec::new();
    let mut expanded: HashMap<String, Version> = HashMap::new();
    let mut edges: HashMap<String, Vec<String>> = HashMap::new();
    let mut pending: VecDeque<String> = VecDeque::new();

    let root_deps: Vec<String> = project
        .manifest
        .dependencies
        .iter()
        .map(|(n, _)| n.clone())
        .collect();

    for (name, sel_str) in &project.manifest.dependencies {
        let sel = Selector::parse(sel_str).map_err(|e| {
            PackageError::Manifest(format!("{}: dependency '{}': {}", MANIFEST_FILE, name, e))
        })?;
        reqs.entry(name.clone()).or_default().push(sel);
        pending.push_back(name.clone());
    }

    let mut guard = 0usize;
    while let Some(name) = pending.pop_front() {
        guard += 1;
        if guard > 10_000 {
            return Err(PackageError::Resolution(format!(
                "dependency resolution did not converge for '{}'",
                name
            )));
        }
        let selectors = reqs.get(&name).cloned().unwrap_or_default();
        if let Some(prev) = expanded.get(&name) {
            if selectors.iter().all(|s| s.matches(prev)) {
                // Already expanded at a version that still satisfies every
                // requirement — nothing to do (also breaks cycles).
                continue;
            }
        }
        let version = choose_version(&project, registry, &name, &selectors)?;
        let meta = source_meta(&project, registry, &name, &version)?;
        versions.insert(name.clone(), version.clone());
        if !resolved_order.contains(&name) {
            resolved_order.push(name.clone());
        }
        expanded.insert(name.clone(), version.clone());
        edges.insert(
            name.clone(),
            meta.dependencies.iter().map(|(n, _)| n.clone()).collect(),
        );
        for (dep, dep_sel_str) in &meta.dependencies {
            let dep_sel = Selector::parse(dep_sel_str).map_err(|e| PackageError::Metadata {
                package: name.clone(),
                detail: format!("dependency '{}': {}", dep_sel_str, e),
            })?;
            reqs.entry(dep.clone()).or_default().push(dep_sel);
            pending.push_back(dep.clone());
        }
    }

    // §88: circular dependency between packages → ERROR.
    detect_cycle(&root_deps, &edges)?;

    // Copy into .zsc_m/ (§85): skip packages already present at that version.
    let mut resolved = Vec::new();
    let mut installed = Vec::new();
    for name in &resolved_order {
        let version = versions[name].clone();
        resolved.push((name.clone(), version.clone()));
        let dest = project.dep_dir(name);
        if installed_matches(&dest, name, &version) {
            continue;
        }
        let src = registry.package_dir(name, &version);
        if !src.is_dir() {
            return Err(PackageError::NotFound(format!(
                "Package '{}' version '{}' not found",
                name, version
            )));
        }
        if dest.exists() {
            std::fs::remove_dir_all(&dest).map_err(io_err)?;
        }
        copy_dir(&src, &dest)?;
        installed.push((name.clone(), version));
    }

    if !resolved.is_empty() {
        std::fs::create_dir_all(project.deps_dir()).map_err(io_err)?;
    }

    // §84: lockfile written/updated after a successful install.
    let lock = Lockfile {
        packages: resolved.clone(),
    };
    std::fs::write(
        project.root.join(LOCK_FILE),
        render_lockfile(&lock),
    )
    .map_err(|e| {
        PackageError::Io(format!("Cannot write {}: {}", LOCK_FILE, e))
    })?;

    Ok(InstallReport {
        resolved,
        installed,
    })
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

    fn install_err(root: &Path, registry: &Path) -> String {
        install(root, &Registry::new(registry))
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

        let report = install(&root, &Registry::new(&reg)).unwrap();

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
        install(&root, &Registry::new(&reg)).unwrap();
        assert_eq!(lock_entries(&root)[0].1, "1.2.10");

        // A newer matching version appears in the registry …
        add_pkg(&reg, "http-utils", "1.2.11", &[], &[("client.zs", "export const V = 2")]);
        // … and install must NOT upgrade (§84, §92).
        let report = install(&root, &Registry::new(&reg)).unwrap();
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
        install(&root, &Registry::new(&reg)).unwrap();
        // Simulate a selector change without re-running install.
        write_file(
            &root.join(MANIFEST_FILE),
            r#"{ name: "my-app", version: "1.0.0", dependencies: { "http-utils": "2.x" } }"#,
        );
        let report = install(&root, &Registry::new(&reg)).unwrap();
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
        let report = install(&root, &Registry::new(&reg)).unwrap();
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
        install(&root, &Registry::new(&reg)).unwrap();
        assert_eq!(lock_entries(&root)[0].1, "1.2.10");

        add_pkg(&reg, "http-utils", "1.2.11", &[], &[("client.zs", "x")]);
        std::fs::remove_file(root.join(LOCK_FILE)).unwrap();
        let report = install(&root, &Registry::new(&reg)).unwrap();
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
            let report = install(&root, &Registry::new(&reg)).unwrap();
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
        let err = install(&dir, &Registry::new(&reg)).unwrap_err().to_string();
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
        let report = install(&root, &Registry::new(&reg)).unwrap();
        assert_eq!(report.installed.len(), 1);

        // Local edit inside .zsc_m/ must survive a re-install (no re-download).
        write_file(&root.join(".zsc_m/pkg/m.zs"), "export const M = 999");
        let report = install(&root, &Registry::new(&reg)).unwrap();
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
        install(&root, &Registry::new(&reg)).unwrap();
        assert_eq!(read(&root.join(".zsc_m/pkg/m.zs")), "v1");

        write_file(
            &root.join(MANIFEST_FILE),
            r#"{ name: "app", version: "1.0.0", dependencies: { "pkg": "2.x" } }"#,
        );
        install(&root, &Registry::new(&reg)).unwrap();
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

        let report = install(&root, &Registry::new(&reg)).unwrap();
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
        let report = install(&root, &Registry::new(&reg)).unwrap();
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
        let report = install(&root, &Registry::new(&reg)).unwrap();
        let shared: Vec<_> = report
            .resolved
            .iter()
            .filter(|(n, _)| n == "shared-utils")
            .collect();
        assert_eq!(shared.len(), 1);
        assert_eq!(shared[0].1.to_string(), "1.2.5");
    }

    #[test]
    fn test_job017_unresolvable_version_conflict_is_error() {
        // §90 allows several coexisting versions — that resolver is Job 018.
        // Job 017 keeps one flat copy, so disjoint requirements are an ERROR.
        let reg = temp_dir("reg_conflict");
        add_pkg(&reg, "package-a", "1.0.0", &[("shared-utils", "1.x")], &[("a.zs", "x")]);
        add_pkg(&reg, "package-b", "1.0.0", &[("shared-utils", "2.x")], &[("b.zs", "x")]);
        add_pkg(&reg, "shared-utils", "1.5.0", &[], &[("s.zs", "x")]);
        add_pkg(&reg, "shared-utils", "2.1.0", &[], &[("s.zs", "x")]);
        let root = new_project(
            "conflict",
            r#"{ name: "app", version: "1.0.0", dependencies: { "package-a": "1.0.0", "package-b": "1.0.0" } }"#,
        );
        let err = install_err(&root, &reg);
        assert!(err.starts_with("ERROR:"), "{}", err);
        assert!(err.contains("shared-utils"), "{}", err);
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
        let report = install(&root, &Registry::new(&reg)).unwrap();
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
