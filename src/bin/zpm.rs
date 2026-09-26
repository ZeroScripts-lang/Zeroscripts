//! `zpm` — ZeroScript Package Manager (§85, §86).
//!
//! Job 017 implements `install` and `run`. `remove`, `update`, git sources
//! and remote registries are Job 018 (§91-§95).

use std::path::Path;

use zeroscript::package::{install, run_script, Project, Registry, MANIFEST_FILE};

const USAGE: &str = "usage: zpm [--registry <dir>] <command> [args]

commands:
  install                 resolve dependencies and update .zsc_m/ + project.lock.zson
  run <script> [args...]  run a script from project.zson (§86)

options:
  --registry <dir>        local package source (default: $ZPM_REGISTRY or <project>/registry)
  -h, --help              show this help";

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

fn registry_for(project_root: &Path, flag: Option<String>) -> Registry {
    if let Some(dir) = flag {
        return Registry::new(dir);
    }
    if let Ok(dir) = std::env::var("ZPM_REGISTRY") {
        if !dir.is_empty() {
            return Registry::new(dir);
        }
    }
    Registry::new(project_root.join("registry"))
}

fn cmd_install(registry_flag: Option<String>, extra: &[String]) -> ! {
    if let Some(arg) = extra.first() {
        fail(&format!(
            "ERROR: unexpected argument '{}' — installing a named package is Job 018",
            arg
        ));
    }
    let project = find_project();
    let registry = registry_for(&project.root, registry_flag);
    match install(&project.root, &registry) {
        Ok(report) => {
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

    let mut registry_flag: Option<String> = None;
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
                registry_flag = Some(value.clone());
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
        "install" => cmd_install(registry_flag, &rest[1..]),
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
