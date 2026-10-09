//! `zsc` — ZeroScript compiler/runner CLI.
//!
//! Usage: `zsc <file.zs> [args...]`, `zsc compile <file.zs> [-o <out>]`,
//! `zsc run <file.zsbc> [args...]`.
//!
//! Program arguments exclude the interpreter and the script name (§52), so
//! `zsc app.zs hello world` yields `system.args() == ["hello", "world"]`,
//! and so does `zsc run app.zsbc hello world` (§158).

use zeroscript::bytecode_io;
use zeroscript::module;
use zeroscript::vm::ZeroVM;
use std::path::{Path, PathBuf};

const VERSION: &str = "ZeroScript VM v0.1.0";

fn usage() -> &'static str {
    "usage: zsc <file.zs> [args...]\n       zsc compile <file.zs> [-o <out.zsbc>]\n       zsc run <file.zsbc> [args...]\n       zsc --version"
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

    // Subcommand dispatch (§158): only the literal first argument selects a
    // mode; a file with this name runs as `./compile` / `./run`.
    match first.as_str() {
        "compile" => cmd_compile(&argv[1..]),
        "run" => cmd_run(&argv[1..]),
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
