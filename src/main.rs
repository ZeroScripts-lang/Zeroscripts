//! `zsc` — ZeroScript compiler/runner CLI.
//!
//! Usage: `zsc <file.zs> [args...]`, `zsc -e <code> [args...]`,
//! `zsc compile <file.zs> [-o <out>]`, `zsc run <file.zsbc> [args...]`,
//! `zsc init`.
//!
//! Program arguments exclude the interpreter and the script name (§52), so
//! `zsc app.zs hello world` yields `system.args() == ["hello", "world"]`,
//! and so does `zsc run app.zsbc hello world` (§158). In eval mode (§100)
//! `system.args()` is the list after the code.

use zeroscript::bytecode_io;
use zeroscript::json::{self, JsonValue};
use zeroscript::module;
use zeroscript::package;
use zeroscript::vm::ZeroVM;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

const VERSION: &str = "ZeroScript VM v0.1.0";

fn usage() -> &'static str {
    "usage: zsc <file.zs> [args...]\n       zsc -e <code> [args...]\n       zsc compile <file.zs> [-o <out.zsbc>]\n       zsc run <file.zsbc> [args...]\n       zsc init\n       zsc --version"
}

fn run(argv: &[String]) -> Result<(), String> {
    let first = match argv.first() {
        Some(s) if s == "--help" || s == "-h" => {
            println!("{}", usage());
            return Ok(());
        }
        Some(s) if s == "--version" || s == "-V" => {
            println!("{}", VERSION);
            return Ok(());
        }
        Some(s) => s,
        None => return Err(format!("missing script file\n{}", usage())),
    };

    // Subcommand dispatch (§158 / §100): only the literal first argument
    // selects a mode; a file with this name runs as `./compile` / `./run` /
    // `./-e`.
    match first.as_str() {
        "compile" => cmd_compile(&argv[1..]),
        "run" => cmd_run(&argv[1..]),
        "init" => cmd_init(&argv[1..]),
        "-e" | "--eval" => cmd_eval(&argv[1..]),
        _ => run_file(first, &argv[1..]),
    }
}

/// Print captured stdout/stderr lines after a successful run.
fn print_output(vm: &ZeroVM) {
    for line in vm.get_output() {
        println!("{}", line);
    }
    for line in vm.get_error_output() {
        eprintln!("{}", line);
    }
}

fn run_file(script: &str, args: &[String]) -> Result<(), String> {
    let path = Path::new(script);
    if !path.is_file() {
        return Err(format!("ERROR: Cannot read {}", script));
    }

    let bundle = module::load(path).map_err(|e| e.to_string())?;
    let mut vm = ZeroVM::new(bundle.program);
    vm.set_program_args(args.to_vec());
    vm.run_modules(&bundle.modules).map_err(|e| e.to_string())?;
    print_output(&vm);
    Ok(())
}

/// `zsc -e <code> [args...]` (§100 "Mode Evaluasi", ISSUES §72a): compile
/// and run `code` without a script file. The eval entry is a virtual file
/// at the current working directory, so relative imports resolve from cwd
/// (§64); everything after `code` is program args (§52) and is never
/// re-parsed as zsc flags.
fn cmd_eval(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "ERROR: usage: zsc -e <code> [args...]";
    let code = args.first().ok_or(USAGE)?;
    let cwd = std::env::current_dir().map_err(|e| format!("ERROR: {}", e))?;
    let virtual_path = cwd.join("__zsc_eval__.zs");
    let bundle = module::load_source(&virtual_path, code).map_err(|e| e.to_string())?;
    let mut vm = ZeroVM::new(bundle.program);
    vm.set_program_args(args[1..].to_vec());
    vm.run_modules(&bundle.modules).map_err(|e| e.to_string())?;
    print_output(&vm);
    Ok(())
}

/// `zsc init` (ISSUES §74, §82, §86, §96): interactive scaffold, `npm init`
/// style. Prompts for name/version/description/author/license/keywords and
/// the entry script; empty answer = default (EOF behaves like all-defaults).
/// Writes `project.zson` (§82 core fields + §96-style optional metadata +
/// keywords as a string array) and a hello template for the entry script
/// (default `main.zs`, matching §82's example project structure). Never
/// overwrites an existing `project.zson`; an existing entry script is left
/// untouched.
fn cmd_init(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "ERROR: usage: zsc init";
    if !args.is_empty() {
        return Err(USAGE.to_string());
    }
    let cwd = std::env::current_dir().map_err(|e| format!("ERROR: {}", e))?;
    let dest = cwd.join("project.zson");
    if dest.exists() {
        return Err(format!(
            "ERROR: project.zson already exists in {}",
            cwd.display()
        ));
    }
    let folder = cwd
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .ok_or("ERROR: cannot determine project name from current directory")?
        .to_string();

    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let name = prompt(&mut stdin, "Project name", Some(&folder))?;
    let version = prompt(&mut stdin, "Version", Some("0.1.0"))?;
    let description = prompt(&mut stdin, "Description (optional)", None)?;
    let author = prompt(&mut stdin, "Author (optional)", None)?;
    let license = prompt(&mut stdin, "License (optional)", None)?;
    let keywords_raw = prompt(&mut stdin, "Keywords, comma-separated (optional)", None)?;
    let entry = prompt(&mut stdin, "Entry script", Some("main.zs"))?;

    if name.is_empty() {
        return Err("ERROR: project name must not be empty".to_string());
    }
    if entry.is_empty() {
        return Err("ERROR: entry script must not be empty".to_string());
    }

    let mut fields: Vec<(String, JsonValue)> = vec![
        ("name".to_string(), JsonValue::String(name.clone())),
        ("version".to_string(), JsonValue::String(version)),
    ];
    if !description.is_empty() {
        fields.push((
            "description".to_string(),
            JsonValue::String(description),
        ));
    }
    if !author.is_empty() {
        fields.push(("author".to_string(), JsonValue::String(author)));
    }
    if !license.is_empty() {
        fields.push(("license".to_string(), JsonValue::String(license)));
    }
    let keywords: Vec<JsonValue> = keywords_raw
        .split(',')
        .map(|k| k.trim())
        .filter(|k| !k.is_empty())
        .map(|k| JsonValue::String(k.to_string()))
        .collect();
    if !keywords.is_empty() {
        fields.push(("keywords".to_string(), JsonValue::Array(keywords)));
    }
    fields.push(("dependencies".to_string(), JsonValue::Object(Vec::new())));
    // §86 `zpm run` scripts — default `start` runs the entry script,
    // mirroring §86's example `start: "zsc index.zs"` (ISSUES §74c).
    fields.push((
        "scripts".to_string(),
        JsonValue::Object(vec![(
            "start".to_string(),
            JsonValue::String(format!("zsc {}", entry)),
        )]),
    ));
    let manifest = JsonValue::Object(fields);

    let text = json::render_zson(&manifest).map_err(|e| format!("ERROR: {}", e))?;
    // Never emit a manifest our own §82 parser would reject.
    package::parse_manifest(&text, "project.zson").map_err(|e| e.to_string())?;

    let entry_path = cwd.join(&entry);
    if !entry_path.exists() {
        if let Some(parent) = entry_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("ERROR: Cannot create {}: {}", parent.display(), e))?;
        }
        std::fs::write(&entry_path, format!("print(\"Hello from {}!\")\n", name))
            .map_err(|e| format!("ERROR: Cannot write {}: {}", entry_path.display(), e))?;
    }
    std::fs::write(&dest, text)
        .map_err(|e| format!("ERROR: Cannot write {}: {}", dest.display(), e))?;
    Ok(())
}

/// Print a prompt on stdout (flushed), read one line from stdin; empty
/// answer or EOF yields `default` (empty string when no default).
fn prompt(stdin: &mut impl BufRead, label: &str, default: Option<&str>) -> Result<String, String> {
    match default {
        Some(d) => print!("{} ({}): ", label, d),
        None => print!("{}: ", label),
    }
    io::stdout()
        .flush()
        .map_err(|e| format!("ERROR: {}", e))?;
    let mut line = String::new();
    let n = stdin
        .read_line(&mut line)
        .map_err(|e| format!("ERROR: {}", e))?;
    if n == 0 {
        return Ok(default.unwrap_or("").to_string());
    }
    let answer = line.trim().to_string();
    if answer.is_empty() {
        Ok(default.unwrap_or("").to_string())
    } else {
        Ok(answer)
    }
}

/// `zsc compile <file.zs> [-o <out.zsbc>]` (§158): compile (never execute)
/// the entry file with all of its imports, then write the bundle image.
fn cmd_compile(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "ERROR: usage: zsc compile <file.zs> [-o <out.zsbc>]";
    let mut file: Option<&String> = None;
    let mut out: Option<&String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                let value = args.get(i + 1).ok_or(USAGE)?;
                if out.is_some() {
                    return Err(USAGE.to_string());
                }
                out = Some(value);
                i += 2;
            }
            _ => {
                if file.is_some() {
                    return Err(USAGE.to_string());
                }
                file = Some(&args[i]);
                i += 1;
            }
        }
    }
    let file = file.ok_or(USAGE)?;
    let path = Path::new(file);
    if !path.is_file() {
        return Err(format!("ERROR: Cannot read {}", file));
    }

    let bundle = module::load(path).map_err(|e| e.to_string())?;
    let bytes = bytecode_io::save(&bundle);
    let dest: PathBuf = match out {
        Some(dest) => PathBuf::from(dest),
        // Same directory as the source; `with_extension` replaces `.zs`
        // or appends when there is none (§158 default `.zsbc`).
        None => path.with_extension("zsbc"),
    };
    std::fs::write(&dest, bytes)
        .map_err(|e| format!("ERROR: Cannot write {}: {}", dest.display(), e))?;
    Ok(())
}

/// `zsc run <file.zsbc> [args...]` (§158): verify checksum, parse payload,
/// hand the bundle to the VM — §124/§125 validation runs inside `run_modules`
/// before the first instruction.
fn cmd_run(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "ERROR: usage: zsc run <file.zsbc> [args...]";
    let file = args.first().ok_or(USAGE)?;
    let path = Path::new(file);
    if !path.is_file() {
        return Err(format!("ERROR: Cannot read {}", file));
    }
    let bytes = std::fs::read(path)
        .map_err(|e| format!("ERROR: Cannot read {}: {}", file, e))?;
    let bundle =
        bytecode_io::load(&bytes).map_err(|e| format!("ERROR: {}", e))?;
    let mut vm = ZeroVM::new(bundle.program);
    vm.set_program_args(args[1..].to_vec());
    vm.run_modules(&bundle.modules).map_err(|e| e.to_string())?;
    print_output(&vm);
    Ok(())
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = run(&argv) {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}
