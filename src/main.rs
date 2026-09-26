//! `zsc` — ZeroScript compiler/runner CLI.
//!
//! Usage: `zsc <file.zs> [args...]`
//!
//! Program arguments exclude the interpreter and the script name (§52), so
//! `zsc app.zs hello world` yields `system.args() == ["hello", "world"]`.

use zeroscript::module;
use zeroscript::vm::ZeroVM;

const VERSION: &str = "ZeroScript VM v0.1.0";

fn usage() -> &'static str {
    "usage: zsc <file.zs> [args...]\n       zsc --version"
}

fn run(argv: &[String]) -> Result<(), String> {
    let script = match argv.first() {
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

    let path = std::path::Path::new(script);
    if !path.is_file() {
        return Err(format!("ERROR: Cannot read {}", script));
    }

    let bundle = module::load(path).map_err(|e| e.to_string())?;
    let mut vm = ZeroVM::new(bundle.program);
    vm.set_program_args(argv[1..].to_vec());
    vm.run_modules(&bundle.modules).map_err(|e| e.to_string())?;

    for line in vm.get_output() {
        println!("{}", line);
    }
    for line in vm.get_error_output() {
        eprintln!("{}", line);
    }
    Ok(())
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = run(&argv) {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}
