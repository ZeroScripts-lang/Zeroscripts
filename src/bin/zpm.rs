//! `zpm` — ZeroScript Package Manager (§85, §91–§95).
//!
//! Commands: `install` (bare, by name, or by `git:` source), `remove`,
//! `update`, `publish`, `run`.

use std::path::{Path, PathBuf};

use zeroscript::git::GitSource;
use zeroscript::package::{
    install, install_with_declared, probe_git, publish, remove_dependency, run_script, update,
    InstallReport, Project, Registry, Sources, MANIFEST_FILE,
};

const USAGE: &str = "usage: zpm [--registry <dir>] <command> [args]

commands:
  install [pkg|git:<url>[#ref]]  resolve dependencies into .zsc_m/ + project.lock.zson;
                                 with an argument, declare it first (§100)
  remove <pkg>                undeclare a direct dependency and prune .zsc_m/ (§91)
  update                      re-resolve to the newest matching versions (§92)
  publish                     copy the package into the registry (§94)
  run <script> [args...]      run a script from project.zson (§86)

options:
  --registry <dir>            local package source, repeatable (default: $ZPM_REGISTRY or <project>/registry)
  -h, --help                  show this help";

fn fail(msg: &str) -> ! {
    eprintln!("{}", msg);
    std::process::exit(1);
}

/// Locate the project by walking up from the current directory — the CLI's
/// only reference point (§87 governs module/package resolution, not zpm's
/// own entry point).
fn find_project() -> Project {
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => fail(&format!("ERROR: Cannot determine current directory: {}", e)),
    };
    match Project::discover(&cwd) {
        Ok(Some(project)) => project,
        Ok(None) => fail(&format!(
            "ERROR: No {} found in this directory or any parent",
            MANIFEST_FILE
        )),
        Err(e) => fail(&e.to_string()),
    }
}

/// §95: registries in configuration order — explicit `--registry` flags win
/// over `$ZPM_REGISTRY`, which wins over the project's `registry/`.
fn registries_for(project_root: &Path, flags: &[String]) -> Vec<Registry> {
    if !flags.is_empty() {
        return flags.iter().map(|d| Registry::new(d.clone())).collect();
    }
    if let Ok(dir) = std::env::var("ZPM_REGISTRY") {
        if !dir.is_empty() {
            return vec![Registry::new(dir)];
        }
    }
    vec![Registry::new(project_root.join("registry"))]
}

/// Everything `zpm install` / `update` / `remove` reads from (§99): the
/// configured registries plus the cache (`$ZPM_CACHE` or `$HOME/.zsc_cache`,
/// §98).
fn sources_for(project_root: &Path, flags: &[String]) -> Sources {
    Sources::new().with_registries(registries_for(project_root, flags))
}

/// The registry a publish targets: `--registry` flags, then `$ZPM_REGISTRY`,
/// then none — §94 names no default publishing location, so publishing
/// without one is an error rather than a guess.
fn publish_registry(flags: &[String]) -> Option<PathBuf> {
    if let Some(dir) = flags.first() {
        return Some(PathBuf::from(dir));
    }
    match std::env::var("ZPM_REGISTRY") {
        Ok(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
        _ => None,
    }
}

fn print_report(report: &InstallReport) {
    if report.resolved.is_empty() {
        println!("No dependencies.");
    }
    for (name, version) in &report.resolved {
        let newly = report
            .installed
            .iter()
            .any(|(n, v)| n == name && v == version);
        if newly {
            println!("+ {}@{}", name, version);
        } else {
            println!("  {}@{} (up to date)", name, version);
        }
    }
}

fn cmd_install(registry_flags: &[String], extra: &[String]) -> ! {
    let project = find_project();
    // §67k (prepare-then-commit): the declaration is staged in memory by
    // install_with_declared and only written to project.zson after the
    // install fully succeeds — a failure at any stage leaves the manifest
    // byte-identical. probe_git runs before staging (it needs no manifest).
    let declared: Option<(String, String)> = match extra {
        [] => None,
        [spec] => {
            // §100: "Dependency harus dideklarasikan secara eksplisit" —
            // an argument to `install` declares the dependency.
            if spec.starts_with("git:") {
                let source = match GitSource::parse(spec) {
                    Ok(s) => s,
                    Err(e) => fail(&format!("ERROR: {}", e)),
                };
                let (name, _) = match probe_git(&source) {
                    Ok(id) => id,
                    Err(e) => fail(&e.to_string()),
                };
                Some((name, spec.clone()))
            } else {
                Some((spec.clone(), "x".to_string()))
            }
        }
        _ => fail("ERROR: usage: zpm install [package|git:<url>[#ref]]"),
    };
    let sources = sources_for(&project.root, registry_flags);
    let result = match &declared {
        Some((name, req)) => install_with_declared(&project.root, &sources, name, req),
        None => install(&project.root, &sources),
    };
    match result {
        Ok(report) => {
            print_report(&report);
            std::process::exit(0);
        }
        Err(e) => fail(&e.to_string()),
    }
}

fn cmd_remove(registry_flags: &[String], extra: &[String]) -> ! {
    let name = match extra {
        [name] => name.as_str(),
        _ => fail("ERROR: usage: zpm remove <package>"),
    };
    let project = find_project();
    let sources = sources_for(&project.root, registry_flags);
    match remove_dependency(&project.root, name, &sources) {
        Ok(report) => {
            print_report(&report);
            std::process::exit(0);
        }
        Err(e) => fail(&e.to_string()),
    }
}

fn cmd_update(registry_flags: &[String], extra: &[String]) -> ! {
    if !extra.is_empty() {
        fail("ERROR: usage: zpm update");
    }
    let project = find_project();
    let sources = sources_for(&project.root, registry_flags);
    match update(&project.root, &sources) {
        Ok(report) => {
            print_report(&report);
            std::process::exit(0);
        }
        Err(e) => fail(&e.to_string()),
    }
}

fn cmd_publish(registry_flags: &[String], extra: &[String]) -> ! {
    if !extra.is_empty() {
        fail("ERROR: usage: zpm publish");
    }
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => fail(&format!("ERROR: Cannot determine current directory: {}", e)),
    };
    match publish(&cwd, publish_registry(registry_flags).as_deref()) {
        Ok(dir) => {
            println!("Published to {}", dir.display());
            std::process::exit(0);
        }
        Err(e) => fail(&e.to_string()),
    }
}

fn cmd_run(args: &[String]) -> ! {
    let name = match args.first() {
        Some(n) => n,
        None => fail("ERROR: usage: zpm run <script> [args...]"),
    };
    // `zpm run start -- --debug`: a single leading `--` is dropped, the rest
    // is forwarded verbatim (§86).
    let mut extra: Vec<String> = args[1..].to_vec();
    if extra.first().map(|s| s.as_str()) == Some("--") {
        extra.remove(0);
    }
    let project = find_project();
    match run_script(&project.root, name, &extra, None) {
        Ok(code) => std::process::exit(code),
        Err(e) => fail(&e.to_string()),
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    let mut registry_flags: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < argv.len() {
        match argv[i].as_str() {
            "-h" | "--help" => {
                println!("{}", USAGE);
                std::process::exit(0);
            }
            "--registry" => {
                let value = argv.get(i + 1).unwrap_or_else(|| {
                    fail("ERROR: --registry requires a directory argument")
                });
                registry_flags.push(value.clone());
                i += 2;
            }
            _ => break,
        }
    }

    let rest = &argv[i..];
    let command = match rest.first() {
        Some(c) => c.as_str(),
        None => fail(&USAGE),
    };

    match command {
        "install" => cmd_install(&registry_flags, &rest[1..]),
        "remove" => cmd_remove(&registry_flags, &rest[1..]),
        "update" => cmd_update(&registry_flags, &rest[1..]),
        "publish" => cmd_publish(&registry_flags, &rest[1..]),
        "run" => cmd_run(&rest[1..]),
        "--help" | "-h" => {
            println!("{}", USAGE);
            std::process::exit(0);
        }
        other => fail(&format!(
            "ERROR: unknown command '{}'\n{}",
            other, USAGE
        )),
    }
}
