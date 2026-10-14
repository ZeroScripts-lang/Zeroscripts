//! Job 036 (ISSUES §75f): runtime error `e.location` — a
//! `{file, line, column}` object (Language.md:1020) filled from the
//! bytecode debug tables (Runtime.md §154 "jika informasi tersebut
//! tersedia"), or `null` when no position was recorded. The bytecode
//! format version bumped 1 → 2 for the side tables; v1 files are
//! rejected with a clear message, never a panic.

use std::fs;
use std::process::Command;
use zeroscript::compiler::{self, BytecodeHeader, BytecodeProgram, FunctionInfo, Opcode};
use zeroscript::hash::Sha256;
use zeroscript::module::{ModuleBundle, ModuleMeta};

// --- 1. Five error categories carry a real position via the CLI. -------

fn run_eval(code: &str) -> (String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_zsc"))
        .arg("-e")
        .arg(code)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn index_error_location_line_column() {
    let (stdout, stderr) = run_eval(
        "try { let A = [10]\n print(A[99]) } catch e { print(e.type) print(e.location.line) print(e.location.column) }",
    );
    assert!(stderr.is_empty(), "{}", stderr);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "IndexError");
    assert_eq!(lines[1], "2");
    assert!(!lines[2].is_empty() && lines[2] != "0");
}

#[test]
fn key_error_location_line_column() {
    let (stdout, _) = run_eval(
        "try { let o = {a: 1}\n print(o[\"b\"]) } catch e { print(e.type) print(e.location.line) }",
    );
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "KeyError");
    assert_eq!(lines[1], "2");
}

#[test]
fn division_error_location_line_column() {
    let (stdout, _) = run_eval(
        "try { let z = 0\n print(1 / z) } catch e { print(e.type) print(e.location.line) }",
    );
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "DivisionError");
    assert_eq!(lines[1], "2");
}

#[test]
fn type_error_location_line_column() {
    let (stdout, _) = run_eval(
        "try { let n = 1\n print(\"a\" + n) } catch e { print(e.type) print(e.location.line) }",
    );
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "TypeError");
    assert_eq!(lines[1], "2");
}

#[test]
fn reference_error_location_line_column() {
    let (stdout, _) = run_eval(
        "try { print(undefined_name_z) } catch e { print(e.type) print(e.location.line) }",
    );
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "ReferenceError");
    assert_eq!(lines[1], "1");
}

// --- 2. `file` is the entry path exactly as typed. ----------------------

fn write_script(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, body).unwrap();
    path
}

#[test]
fn location_file_is_input_path_verbatim() {
    let dir = std::env::temp_dir().join("zs036_verbatim");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    write_script(
        &dir,
        "loc_err.zs",
        "try {\n    let A = [1]\n    print(A[9])\n} catch e {\n    print(e.location.file)\n}\n",
    );

    // Run with the exact relative spelling; `location.file` must echo it
    // back unchanged (decision 1: path as typed, no canonicalization).
    let out = Command::new(env!("CARGO_BIN_EXE_zsc"))
        .current_dir(&dir)
        .arg("loc_err.zs")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "loc_err.zs\n");

    // "./" prefix survives too.
    let out = Command::new(env!("CARGO_BIN_EXE_zsc"))
        .current_dir(&dir)
        .arg("./loc_err.zs")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "./loc_err.zs\n");
}

// --- 3. Location survives a .zsbc compile → save → load round trip. -----

#[test]
fn location_survives_zsbc_roundtrip() {
    let dir = std::env::temp_dir().join("zs036_roundtrip");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    write_script(
        &dir,
        "rt.zs",
        "let x = 1\ntry {\n    print(\"a\" + x)\n} catch e {\n    print(e.type)\n    print(e.location.file)\n    print(e.location.line)\n}\n",
    );

    let compile = Command::new(env!("CARGO_BIN_EXE_zsc"))
        .current_dir(&dir)
        .args(["compile", "rt.zs"])
        .output()
        .unwrap();
    assert!(compile.status.success(), "{:?}", compile);

    let run = Command::new(env!("CARGO_BIN_EXE_zsc"))
        .current_dir(&dir)
        .args(["run", "rt.zsbc"])
        .output()
        .unwrap();
    assert!(run.status.success(), "{:?}", run);
    let stdout = String::from_utf8_lossy(&run.stdout);
    // The debug side table (including the source path recorded at compile
    // time) round-trips through the v2 file format.
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "TypeError");
    assert_eq!(lines[1], "rt.zs");
    assert_eq!(lines[2], "3");
}

// --- 4. A v1 file is rejected with a clear message, no panic. -----------

fn minimal_bundle() -> ModuleBundle {
    ModuleBundle {
        program: BytecodeProgram {
            header: BytecodeHeader::current(),
            entry_point: 0,
            constants: vec![],
            instructions: vec![Opcode::Halt],
            functions: vec![FunctionInfo {
                name: "f".to_string(),
                params: vec![],
                param_defaults: vec![],
                entry_point: 0,
                local_count: 0,
                captured_vars: vec![],
                is_async: false,
                source_file: String::new(),
                debug: vec![],
                default_debug: vec![],
            }],
            function_bodies: vec![vec![]],
            default_bodies: vec![],
            debug: vec![],
        },
        modules: vec![ModuleMeta {
            path: "main.zs".to_string(),
            display_path: "main.zs".to_string(),
            start: 0,
            end: 1,
            exports: vec![],
            imports: vec![],
            re_exports: vec![],
        }],
    }
}

#[test]
fn v1_file_rejected_with_clear_message_not_panic() {
    assert_eq!(compiler::BYTECODE_VERSION, 2, "Job 036 bumped the format");
    let mut bytes = zeroscript::bytecode_io::save(&minimal_bundle());

    // Rewrite the version field to 1 (layout: u64 magic-len, magic, u32
    // version) and re-hash so the checksum stays valid — isolating the
    // version check from integrity.
    bytes[12..16].copy_from_slice(&1u32.to_le_bytes());
    let payload_len = bytes.len() - 32;
    let mut hasher = Sha256::new();
    hasher.update(&bytes[..payload_len]);
    bytes[payload_len..].copy_from_slice(&hasher.finish());

    let dir = std::env::temp_dir().join("zs036_v1reject");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("old.zsbc");
    fs::write(&file, &bytes).unwrap();

    let run = Command::new(env!("CARGO_BIN_EXE_zsc"))
        .current_dir(&dir)
        .args(["run", "old.zsbc"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success(), "v1 must not execute");
    assert!(
        stderr.contains("Unsupported bytecode version") && stderr.contains('1'),
        "clear rejection message, got: {}",
        stderr
    );
}

// --- 5. Unit-level: constants and the loaded program carry the tables. --

#[test]
fn compile_stamps_debug_table() {
    use zeroscript::lexer::Lexer;
    use zeroscript::parser::Parser;

    let source = "print(1 + 2)";
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);
    let program = parser.parse().unwrap();
    let bc = compiler::compile(&program).unwrap();
    // Sparse side table: one entry per located instruction, keyed by pc.
    assert!(!bc.debug.is_empty());
    assert!(bc.debug.windows(2).all(|w| w[0].pc < w[1].pc));
    let entry = &bc.debug[0];
    assert_eq!(entry.line, 1);
    assert!(entry.column >= 1);
}
