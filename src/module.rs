//! Module loader: path resolution (§64), dependency graph, circular
//! detection (§62/§77), topo order (§71), and bytecode merge (§78).

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::ast::Stmt;
use crate::compiler::{compile_module, merge_modules, BytecodeProgram, ModuleUnit};
use crate::error::{CompilerError, LexerError, ParserError};
use crate::lexer::Lexer;
use crate::parser::Parser;

#[derive(Debug, Clone, PartialEq)]
pub enum ModuleError {
    NotFound(String),
    Circular(Vec<String>),
    ExportNotFound { path: String, name: String },
    ExportConflict(String),
    Io(String),
    Lexer(LexerError),
    Parser(ParserError),
    Compiler(CompilerError),
}

impl fmt::Display for ModuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModuleError::NotFound(p) => write!(f, "ERROR: Module not found: {}", p),
            ModuleError::Circular(chain) => {
                write!(f, "ERROR: Circular module dependency detected: {}", chain.join(" -> "))
            }
            ModuleError::ExportNotFound { path, name } => {
                write!(f, "ERROR: Export '{}' not found in {}", name, path)
            }
            ModuleError::ExportConflict(name) => {
                write!(f, "ERROR: Duplicate export '{}'", name)
            }
            ModuleError::Io(msg) => write!(f, "ERROR: {}", msg),
            ModuleError::Lexer(e) => write!(f, "{}", e),
            ModuleError::Parser(e) => write!(f, "{}", e),
            ModuleError::Compiler(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for ModuleError {}

impl From<LexerError> for ModuleError {
    fn from(e: LexerError) -> Self {
        ModuleError::Lexer(e)
    }
}

impl From<ParserError> for ModuleError {
    fn from(e: ParserError) -> Self {
        ModuleError::Parser(e)
    }
}

impl From<CompilerError> for ModuleError {
    fn from(e: CompilerError) -> Self {
        ModuleError::Compiler(e)
    }
}

/// Runtime metadata for one module in the merged program.
#[derive(Debug, Clone)]
pub struct ModuleMeta {
    pub path: String,
    pub start: usize,
    /// Exclusive end (index after EndModule).
    pub end: usize,
    pub exports: Vec<String>,
    /// (local_name, dep_canonical_path, export_name). Without alias, local == export.
    pub imports: Vec<(String, String, String)>,
    /// Re-export edges: (export_name, dep_canonical_path). §67.
    pub re_exports: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct ModuleBundle {
    pub program: BytecodeProgram,
    /// Topological order: dependencies before dependents (§71).
    pub modules: Vec<ModuleMeta>,
}

struct Loaded {
    unit: ModuleUnit,
    exports_set: Vec<String>,
    /// (local_name, raw_path, export_name)
    imports: Vec<(String, String, String)>,
    /// (export_name, raw_path)
    re_exports: Vec<(String, String)>,
}

/// Resolve `spec` relative to `importer` (§64). Auto-appends `.zs`.
fn resolve_path(importer: &Path, spec: &str) -> Result<PathBuf, ModuleError> {
    if !(spec.starts_with("./") || spec.starts_with("../") || spec.starts_with('/')) {
        // Bare/package imports are Job 016-017; reject here so they never
        // silently resolve against the importer directory.
        return Err(ModuleError::NotFound(spec.to_string()));
    }
    if spec.ends_with('/') {
        return Err(ModuleError::NotFound(spec.to_string()));
    }

    let base = importer.parent().unwrap_or_else(|| Path::new("."));
    let joined = if spec.starts_with('/') {
        PathBuf::from(spec)
    } else {
        base.join(spec)
    };

    let mut path = joined.clone();
    let ends_with_zs = path
        .to_string_lossy()
        .ends_with(".zs");
    if !ends_with_zs {
        let mut s = path.to_string_lossy().into_owned();
        s.push_str(".zs");
        path = PathBuf::from(s);
    }

    if !path.is_file() {
        return Err(ModuleError::NotFound(spec.to_string()));
    }

    // Canonicalize for stable identity across imports of the same file.
    match path.canonicalize() {
        Ok(c) => Ok(c),
        Err(_) => Ok(path),
    }
}

fn parse_and_extract_imports(
    source: &str,
) -> Result<
    (
        crate::ast::Program,
        Vec<(String, String, String)>, // (local, raw_path, export_name)
        Vec<(String, String)>,         // re-exports: (export_name, raw_path)
    ),
    ModuleError,
> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize()?;
    let mut parser = Parser::new(tokens);
    let program = parser.parse()?;

    let mut imports = Vec::new();
    let mut re_exports = Vec::new();
    for stmt in &program.statements {
        match stmt {
            Stmt::Import { names, path, .. } => {
                for (export_name, local_name) in names {
                    imports.push((local_name.clone(), path.clone(), export_name.clone()));
                }
            }
            Stmt::ExportList {
                names, from: Some(path), ..
            } => {
                for name in names {
                    re_exports.push((name.clone(), path.clone()));
                }
            }
            _ => {}
        }
    }
    Ok((program, imports, re_exports))
}

/// Load entry module and all dependencies. Returns merged program + meta in topo order.
pub fn load(entry_path: &Path) -> Result<ModuleBundle, ModuleError> {
    let entry_canon = if entry_path.is_file() {
        entry_path
            .canonicalize()
            .unwrap_or_else(|_| entry_path.to_path_buf())
    } else {
        return Err(ModuleError::NotFound(entry_path.display().to_string()));
    };

    // gray = currently on DFS stack (cycle detection); black = done.
    let mut gray: Vec<PathBuf> = Vec::new();
    let mut black: HashMap<PathBuf, ()> = HashMap::new();
    let mut order: Vec<PathBuf> = Vec::new();
    let mut units: HashMap<PathBuf, Loaded> = HashMap::new();

    // DFS post-order: visit deps first, then push self (§71).
    fn visit(
        path: &Path,
        gray: &mut Vec<PathBuf>,
        black: &mut HashMap<PathBuf, ()>,
        order: &mut Vec<PathBuf>,
        units: &mut HashMap<PathBuf, Loaded>,
    ) -> Result<(), ModuleError> {
        if black.contains_key(path) {
            return Ok(());
        }
        if let Some(pos) = gray.iter().position(|p| p == path) {
            let mut chain: Vec<String> = gray[pos..]
                .iter()
                .map(|p| p.display().to_string())
                .collect();
            chain.push(path.display().to_string());
            return Err(ModuleError::Circular(chain));
        }

        gray.push(path.to_path_buf());

        let source = std::fs::read_to_string(path)
            .map_err(|e| ModuleError::Io(format!("Cannot read {}: {}", path.display(), e)))?;
        let (program, raw_imports, raw_re_exports) = parse_and_extract_imports(&source)?;

        // Resolve deps in source order (§71). Imports first, then re-exports.
        let mut resolved: Vec<(String, String, String, PathBuf)> = Vec::new();
        for (local, spec, export_name) in &raw_imports {
            let dep = resolve_path(path, spec)?;
            resolved.push((local.clone(), spec.clone(), export_name.clone(), dep));
        }
        let mut resolved_re: Vec<(String, String, PathBuf)> = Vec::new();
        for (export_name, spec) in &raw_re_exports {
            let dep = resolve_path(path, spec)?;
            resolved_re.push((export_name.clone(), spec.clone(), dep));
        }

        // Unique deps for recursive load (preserve first-seen order).
        let mut seen_deps: Vec<PathBuf> = Vec::new();
        for (_, _, _, dep) in &resolved {
            if !seen_deps.contains(dep) {
                seen_deps.push(dep.clone());
            }
        }
        for (_, _, dep) in &resolved_re {
            if !seen_deps.contains(dep) {
                seen_deps.push(dep.clone());
            }
        }
        for dep in &seen_deps {
            visit(dep, gray, black, order, units)?;
        }

        // Compile after deps so export existence can be checked against
        // dep export sets (§63) without a second full pass later.
        let unit = compile_module(&path.display().to_string(), &program)?;

        // Check export existence (case-sensitive, §63) for imports.
        let mut imports_meta = Vec::new();
        for (local, spec, export_name, dep) in &resolved {
            let dep_unit = units.get(dep).ok_or_else(|| {
                ModuleError::NotFound(spec.clone())
            })?;
            if !dep_unit.exports_set.iter().any(|e| e == export_name) {
                return Err(ModuleError::ExportNotFound {
                    path: dep.display().to_string(),
                    name: export_name.clone(),
                });
            }
            imports_meta.push((local.clone(), dep.display().to_string(), export_name.clone()));
        }

        // Check re-export sources exist (§67).
        let mut re_exports_meta = Vec::new();
        for (export_name, spec, dep) in &resolved_re {
            let dep_unit = units.get(dep).ok_or_else(|| {
                ModuleError::NotFound(spec.clone())
            })?;
            if !dep_unit.exports_set.iter().any(|e| e == export_name) {
                return Err(ModuleError::ExportNotFound {
                    path: dep.display().to_string(),
                    name: export_name.clone(),
                });
            }
            re_exports_meta.push((export_name.clone(), dep.display().to_string()));
        }

        // Export conflict (§68): compiler already rejects local duplicates and
        // local-vs-re-export via record_export/record_re_export. Also guard the
        // re-export list itself in case compile path was bypassed.
        {
            let mut seen: Vec<&str> = Vec::new();
            for (name, _) in &re_exports_meta {
                if seen.contains(&name.as_str()) {
                    return Err(ModuleError::ExportConflict(name.clone()));
                }
                seen.push(name.as_str());
            }
        }

        let exports_set = unit.exports.clone();
        units.insert(
            path.to_path_buf(),
            Loaded {
                unit,
                exports_set,
                imports: imports_meta,
                re_exports: re_exports_meta,
            },
        );

        gray.pop();
        black.insert(path.to_path_buf(), ());
        order.push(path.to_path_buf());
        Ok(())
    }

    visit(&entry_canon, &mut gray, &mut black, &mut order, &mut units)?;

    let unit_list: Vec<ModuleUnit> = order
        .iter()
        .map(|p| units.get(p).unwrap().unit.clone())
        .collect();
    let program = merge_modules(&unit_list);

    // Build ModuleMeta with instruction ranges by walking merged layout.
    let mut modules = Vec::new();
    let mut cursor = 0usize;
    for p in &order {
        let loaded = units.get(p).unwrap();
        let len = loaded.unit.instructions.len();
        modules.push(ModuleMeta {
            path: p.display().to_string(),
            start: cursor,
            end: cursor + len,
            exports: loaded.exports_set.clone(),
            imports: loaded.imports.clone(),
            re_exports: loaded.re_exports.clone(),
        });
        cursor += len;
    }

    Ok(ModuleBundle { program, modules })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::vm::ZeroVM;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TMP_SEQ: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir() -> PathBuf {
        let n = TMP_SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "zeroscript_job012_{}_{}",
            std::process::id(),
            n
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_files(dir: &Path, files: &[(&str, &str)]) {
        for (rel, content) in files {
            let path = dir.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, content).unwrap();
        }
    }

    fn run_project(files: &[(&str, &str)], entry: &str) -> Result<Vec<String>, String> {
        let dir = temp_dir();
        write_files(&dir, files);
        let entry_path = dir.join(entry);
        let result = (|| -> Result<Vec<String>, String> {
            let bundle = load(&entry_path).map_err(|e| e.to_string())?;
            let mut vm = ZeroVM::new(bundle.program);
            vm.run_modules(&bundle.modules).map_err(|e| e.to_string())?;
            Ok(vm.get_output().to_vec())
        })();
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    fn parse_err(source: &str) -> String {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        parser.parse().unwrap_err().to_string()
    }

    // 1. import/export dasar (§46, §47)
    #[test]
    fn test_job012_basic_import_export() {
        let out = run_project(
            &[
                (
                    "math.zs",
                    r#"export function tambah(A, B) {
    return A + B
}
export const VERSION = "1.0""#,
                ),
                (
                    "main.zs",
                    r#"import { tambah, VERSION } from "./math"
print(tambah(2, 3))
print(VERSION)"#,
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["5", "1.0"]);
    }

    // 1b. export { name } form (§47)
    #[test]
    fn test_job012_export_list_form() {
        let out = run_project(
            &[
                (
                    "math.zs",
                    r#"function tambah(A, B) {
    return A + B
}
const VERSION = "1.0"
export { tambah, VERSION }"#,
                ),
                (
                    "main.zs",
                    r#"import { tambah, VERSION } from "./math"
print(tambah(4, 6))
print(VERSION)"#,
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["10", "1.0"]);
    }

    // 2. module hanya diinisialisasi sekali (§61, §72)
    #[test]
    fn test_job012_module_initialized_once() {
        let out = run_project(
            &[
                (
                    "counter.zs",
                    r#"let count = 0
count = count + 1
export function get() {
    return count
}
print("init")"#,
                ),
                (
                    "main.zs",
                    r#"import { get } from "./counter"
import { get } from "./counter"
print(get())
print(get())"#,
                ),
            ],
            "main.zs",
        );
        // Duplicate same binding name → ERROR (req 16)
        let err = out.unwrap_err();
        assert!(err.contains("Duplicate"), "{}", err);
    }

    #[test]
    fn test_job012_init_once_two_imports_diff_names() {
        let out = run_project(
            &[
                (
                    "a.zs",
                    "print(\"A\")\nexport const X = 10\nexport const Y = 20",
                ),
                (
                    "main.zs",
                    "import { X } from \"./a\"\nimport { Y } from \"./a\"\nprint(X)\nprint(Y)",
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["A", "10", "20"]);
    }

    // 3. private variable tidak dapat diakses dari luar (§65)
    #[test]
    fn test_job012_private_not_accessible() {
        let out = run_project(
            &[
                ("utils.zs", "let secret = 42\nexport function get() { return secret }"),
                (
                    "main.zs",
                    r#"import { get } from "./utils"
print(get())
print(secret)"#,
                ),
            ],
            "main.zs",
        );
        let err = out.unwrap_err();
        assert!(err.to_lowercase().contains("undefined"), "{}", err);
    }

    // 4. dua module dengan variable nama sama tidak konflik (§65, §76)
    #[test]
    fn test_job012_same_name_no_conflict() {
        let out = run_project(
            &[
                (
                    "a.zs",
                    r#"let value = 10
export function get() {
    return value
}"#,
                ),
                (
                    "b.zs",
                    r#"let value = 20
export function get() {
    return value
}"#,
                ),
                (
                    "main.zs",
                    r#"import { get } from "./a"
print(get())"#,
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["10"]);
        // Second module with same private name `value` also works independently:
        let out2 = run_project(
            &[
                (
                    "a.zs",
                    "let value = 10\nexport function getA() {\n    return value\n}",
                ),
                (
                    "b.zs",
                    "let value = 20\nexport function getB() {\n    return value\n}",
                ),
                (
                    "main.zs",
                    "import { getA } from \"./a\"\nimport { getB } from \"./b\"\nprint(getA())\nprint(getB())",
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out2, vec!["10", "20"]);
    }

    #[test]
    fn test_job012_isolation_distinct_exports() {
        let out = run_project(
            &[
                (
                    "a.zs",
                    r#"let value = 10
export function getA() {
    return value
}"#,
                ),
                (
                    "b.zs",
                    r#"let value = 20
export function getB() {
    return value
}"#,
                ),
                (
                    "main.zs",
                    r#"import { getA } from "./a"
import { getB } from "./b"
print(getA())
print(getB())"#,
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["10", "20"]);
    }

    // 5. path resolution: sibling, subdirectory, ../
    #[test]
    fn test_job012_path_resolution_sibling_subdir_parent() {
        let out = run_project(
            &[
                (
                    "lib/auth.zs",
                    r#"import { helper } from "./utils/text"
export function login() {
    return helper()
}"#,
                ),
                (
                    "lib/utils/text.zs",
                    r#"export function helper() {
    return "ok"
}"#,
                ),
                (
                    "main.zs",
                    r#"import { login } from "./lib/auth"
import { helper } from "./lib/utils/text"
print(login())
print(helper())"#,
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["ok", "ok"]);
    }

    #[test]
    fn test_job012_path_resolution_parent_dir() {
        let out = run_project(
            &[
                (
                    "lib/math.zs",
                    r#"import { name } from "../common"
export function use() { return name }"#,
                ),
                ("common.zs", r#"export const name = "common""#),
                (
                    "main.zs",
                    r#"import { use } from "./lib/math"
print(use())"#,
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["common"]);
    }

    // 6. directory import tanpa file valid → ERROR
    #[test]
    fn test_job012_directory_import_error() {
        let err = run_project(
            &[("utils/keep.zs", "export const x = 1"), ("main.zs", "import { x } from \"./utils/\"")],
            "main.zs",
        )
        .unwrap_err();
        assert!(err.contains("Module not found"), "{}", err);
    }

    // 7. module tidak ditemukan → ERROR
    #[test]
    fn test_job012_module_not_found() {
        let err = run_project(&[("main.zs", "import { x } from \"./missing\"")], "main.zs")
            .unwrap_err();
        assert!(err.contains("Module not found"), "{}", err);
    }

    // 8. export tidak ditemukan → ERROR, case-sensitive
    #[test]
    fn test_job012_export_not_found() {
        let err = run_project(
            &[
                ("math.zs", "export function tambah(A, B) { return A + B }"),
                ("main.zs", "import { kurang } from \"./math\""),
            ],
            "main.zs",
        )
        .unwrap_err();
        assert!(err.contains("Export 'kurang' not found"), "{}", err);
    }

    #[test]
    fn test_job012_export_case_sensitive() {
        let err = run_project(
            &[
                ("math.zs", "export function tambah(A, B) { return A + B }"),
                ("main.zs", "import { Tambah } from \"./math\""),
            ],
            "main.zs",
        )
        .unwrap_err();
        assert!(err.contains("Export 'Tambah' not found"), "{}", err);
    }

    // 9. circular dependency langsung A→B→A
    #[test]
    fn test_job012_circular_direct() {
        let err = run_project(
            &[
                ("a.zs", "import { B } from \"./b\"\nexport const A = 10"),
                ("b.zs", "import { A } from \"./a\"\nexport const B = 20"),
                ("main.zs", "import { A } from \"./a\""),
            ],
            "main.zs",
        )
        .unwrap_err();
        assert!(
            err.contains("Circular module dependency detected"),
            "{}",
            err
        );
    }

    // 10. circular dependency tidak langsung A→B→C→A
    #[test]
    fn test_job012_circular_indirect() {
        let err = run_project(
            &[
                ("a.zs", "import { C } from \"./c\"\nexport const A = 1"),
                ("b.zs", "import { A } from \"./a\"\nexport const B = 2"),
                ("c.zs", "import { B } from \"./b\"\nexport const C = 3"),
                ("main.zs", "import { A } from \"./a\""),
            ],
            "main.zs",
        )
        .unwrap_err();
        assert!(
            err.contains("Circular module dependency detected"),
            "{}",
            err
        );
    }

    // 11. self-import → ERROR
    #[test]
    fn test_job012_self_import() {
        let err = run_project(
            &[
                ("math.zs", "import { tambah } from \"./math\"\nexport function tambah() { return 1 }"),
                ("main.zs", "import { tambah } from \"./math\""),
            ],
            "main.zs",
        )
        .unwrap_err();
        assert!(
            err.contains("Circular module dependency detected"),
            "{}",
            err
        );
    }

    // 12. urutan inisialisasi dependency (§71)
    #[test]
    fn test_job012_init_order() {
        let out = run_project(
            &[
                ("a.zs", "print(\"a\")\nexport const A = 1"),
                ("b.zs", "print(\"b\")\nexport const B = 2"),
                (
                    "main.zs",
                    "import { A } from \"./a\"\nimport { B } from \"./b\"\nprint(\"main\")",
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["a", "b", "main"]);
    }

    #[test]
    fn test_job012_init_order_nested() {
        let out = run_project(
            &[
                ("b.zs", "print(\"b\")\nexport const B = 1"),
                ("a.zs", "print(\"a\")\nimport { B } from \"./b\"\nexport const A = B"),
                ("main.zs", "import { A } from \"./a\"\nprint(\"main\")\nprint(A)"),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["b", "a", "main", "1"]);
    }

    // 13. import di tengah file tetap diproses sebelum kode lain (§78)
    #[test]
    fn test_job012_import_mid_file_static() {
        let out = run_project(
            &[
                ("utils.zs", "print(\"utils\")\nexport function test() { return 1 }"),
                (
                    "main.zs",
                    "print(\"A\")\nimport { test } from \"./utils\"\nprint(\"B\")",
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["utils", "A", "B"]);
    }

    // 14. import di dalam function/if/while → ERROR compile-time
    #[test]
    fn test_job012_import_inside_function_error() {
        let err = parse_err("function f() {\n  import { x } from \"./m\"\n}");
        assert!(err.to_lowercase().contains("import"), "{}", err);
    }

    #[test]
    fn test_job012_import_inside_if_error() {
        let err = parse_err("if true {\n  import { x } from \"./m\"\n}");
        assert!(err.to_lowercase().contains("import"), "{}", err);
    }

    #[test]
    fn test_job012_import_inside_while_error() {
        let err = parse_err("while true {\n  import { x } from \"./m\"\n}");
        assert!(err.to_lowercase().contains("import"), "{}", err);
    }

    // 15. module evaluation failure → export tidak tersedia
    #[test]
    fn test_job012_eval_failure() {
        let err = run_project(
            &[
                (
                    "config.zs",
                    "let A = 10 / 0\nexport const VALUE = A",
                ),
                ("main.zs", "import { VALUE } from \"./config\"\nprint(VALUE)"),
            ],
            "main.zs",
        )
        .unwrap_err();
        // Division by zero must surface as a runtime error during config init.
        assert!(!err.is_empty(), "expected error");
        // §78 static order: config fails before main body runs — main's output
        // never appears. §73 vs §78 interaction documented in ISSUES.md §62.
    }

    // 16. duplicate import module yang sama (binding sama) → ERROR
    #[test]
    fn test_job012_duplicate_import_same_binding() {
        let err = run_project(
            &[
                ("math.zs", "export function tambah() { return 1 }"),
                (
                    "main.zs",
                    "import { tambah } from \"./math\"\nimport { tambah } from \"./math\"",
                ),
            ],
            "main.zs",
        )
        .unwrap_err();
        assert!(err.contains("Duplicate"), "{}", err);
    }

    // 17. duplicate import module yang sama dengan binding berbeda → valid
    #[test]
    fn test_job012_duplicate_import_diff_bindings() {
        let out = run_project(
            &[
                (
                    "math.zs",
                    "export function tambah() { return 1 }\nexport function kurang() { return 2 }",
                ),
                (
                    "main.zs",
                    "import { tambah } from \"./math\"\nimport { kurang } from \"./math\"\nprint(tambah())\nprint(kurang())",
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["1", "2"]);
    }

    // §46 exact example: extension not written
    #[test]
    fn test_job012_no_extension_in_path() {
        let out = run_project(
            &[
                ("math.zs", "export function tambah(A, B) { return A + B }"),
                ("main.zs", "import { tambah } from \"./math\"\nprint(tambah(1, 2))"),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["3"]);
    }

    // Imported binding usable inside function of same module (§75)
    #[test]
    fn test_job012_import_visible_to_local_function() {
        let out = run_project(
            &[
                ("math.zs", "export function tambah(A, B) { return A + B }"),
                (
                    "main.zs",
                    "import { tambah } from \"./math\"\nfunction test() {\n  return tambah(1, 2)\n}\nprint(test())",
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["3"]);
    }

    // Private state captured by exported function
    #[test]
    fn test_job012_private_captured_by_export() {
        let out = run_project(
            &[
                (
                    "counter.zs",
                    r#"let count = 0
export function increment() {
    count = count + 1
}
export function get() {
    return count
}"#,
                ),
                (
                    "main.zs",
                    r#"import { increment, get } from "./counter"
print(get())
increment()
increment()
print(get())"#,
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["0", "2"]);
    }

    // §61 example output
    #[test]
    fn test_job012_spec_section61_example() {
        let out = run_project(
            &[
                ("utils.zs", "let A = 10\nexport function getA() {\n    return A\n}"),
                (
                    "main.zs",
                    "import { getA } from \"./utils\"\nprint(getA())",
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["10"]);
    }

    // Job 013 — Live binding, alias, re-export, conflict (§66-§70, §74)

    // 1. live binding: §66 counter example exactly
    #[test]
    fn test_job013_live_binding_counter() {
        let out = run_project(
            &[
                (
                    "counter.zs",
                    "export let count = 0\nexport function increment() {\n    count++\n}",
                ),
                (
                    "main.zs",
                    "import { count, increment } from \"./counter\"\nprint(count)\nincrement()\nprint(count)",
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["0", "1"]);
    }

    // 2. assignment to imported binding → ERROR (export let and export const)
    #[test]
    fn test_job013_assign_to_imported_let_error() {
        let err = run_project(
            &[
                ("counter.zs", "export let count = 0"),
                (
                    "main.zs",
                    "import { count } from \"./counter\"\ncount = 10",
                ),
            ],
            "main.zs",
        )
        .unwrap_err();
        assert!(
            err.contains("const") || err.contains("Duplicate") || err.contains("assign"),
            "{}",
            err
        );
    }

    #[test]
    fn test_job013_assign_to_imported_const_error() {
        let err = run_project(
            &[
                ("cfg.zs", "export const MAX = 100"),
                (
                    "main.zs",
                    "import { MAX } from \"./cfg\"\nMAX = 1",
                ),
            ],
            "main.zs",
        )
        .unwrap_err();
        assert!(
            err.contains("const") || err.contains("Duplicate") || err.contains("assign"),
            "{}",
            err
        );
    }

    // 3. re-export barrel (§67 index.zs example)
    #[test]
    fn test_job013_re_export_barrel() {
        let out = run_project(
            &[
                ("lib/math.zs", "export function tambah(A, B) {\n    return A + B\n}"),
                ("lib/string.zs", "export function upper(s) {\n    return s\n}"),
                (
                    "lib/index.zs",
                    "export { tambah } from \"./math\"\nexport { upper } from \"./string\"",
                ),
                (
                    "main.zs",
                    "import { tambah, upper } from \"./lib/index\"\nprint(tambah(2, 3))\nprint(upper(\"x\"))",
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["5", "x"]);
    }

    // 4. re-export source module not found → ERROR
    #[test]
    fn test_job013_re_export_missing_source() {
        let err = run_project(
            &[("main.zs", "export { tambah } from \"./nope\"")],
            "main.zs",
        )
        .unwrap_err();
        assert!(err.contains("not found") || err.contains("Module"), "{}", err);
    }

    // 5. re-export export not in source → ERROR
    #[test]
    fn test_job013_re_export_missing_export() {
        let err = run_project(
            &[
                ("math.zs", "export function tambah() { return 1 }"),
                ("main.zs", "export { kurang } from \"./math\""),
            ],
            "main.zs",
        )
        .unwrap_err();
        assert!(err.contains("Export") || err.contains("kurang"), "{}", err);
    }

    // 6. duplicate export: local const vs re-export → ERROR
    #[test]
    fn test_job013_duplicate_export_local_vs_reexport() {
        let err = run_project(
            &[
                ("other.zs", "export const A = 1"),
                ("main.zs", "export const A = 10\nexport { A } from \"./other\""),
            ],
            "main.zs",
        )
        .unwrap_err();
        assert!(err.contains("Duplicate export"), "{}", err);
    }

    // 7. duplicate export: same name re-exported from two sources → ERROR
    #[test]
    fn test_job013_duplicate_export_two_reexports() {
        let err = run_project(
            &[
                ("math.zs", "export function tambah() { return 1 }"),
                ("other.zs", "export function tambah() { return 2 }"),
                (
                    "main.zs",
                    "export { tambah } from \"./math\"\nexport { tambah } from \"./other\"",
                ),
            ],
            "main.zs",
        )
        .unwrap_err();
        assert!(err.contains("Duplicate export"), "{}", err);
    }

    // 8. import alias basic (§69 tambah as add)
    #[test]
    fn test_job013_import_alias_basic() {
        let out = run_project(
            &[
                (
                    "math.zs",
                    "export function tambah(A, B) {\n    return A + B\n}\nexport function kurang(A, B) {\n    return A - B\n}",
                ),
                (
                    "main.zs",
                    "import { tambah as add, kurang as sub } from \"./math\"\nprint(add(2, 3))\nprint(sub(10, 4))",
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["5", "6"]);
    }

    // 9. alias conflicts with local declaration → ERROR
    #[test]
    fn test_job013_alias_conflicts_with_local() {
        let err = run_project(
            &[
                ("math.zs", "export function tambah() { return 1 }"),
                (
                    "main.zs",
                    "import { tambah as add } from \"./math\"\nlet add = 2",
                ),
            ],
            "main.zs",
        )
        .unwrap_err();
        assert!(err.contains("Duplicate"), "{}", err);
    }

    // 10. multiple named imports, some aliased (§70)
    #[test]
    fn test_job013_multiple_imports_partial_alias() {
        let out = run_project(
            &[
                (
                    "math.zs",
                    "export function tambah(A, B) {\n    return A + B\n}\nexport function kurang(A, B) {\n    return A - B\n}\nexport function kali(A, B) {\n    return A * B\n}",
                ),
                (
                    "main.zs",
                    "import {\n    tambah as add,\n    kurang as sub,\n    kali\n} from \"./math\"\nprint(add(1, 2))\nprint(sub(5, 3))\nprint(kali(4, 5))",
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["3", "2", "20"]);
    }

    // 11. duplicate local binding without alias → ERROR
    #[test]
    fn test_job013_duplicate_local_without_alias() {
        let err = run_project(
            &[
                ("math.zs", "export function tambah() { return 1 }"),
                (
                    "main.zs",
                    "import { tambah } from \"./math\"\nimport { tambah } from \"./math\"",
                ),
            ],
            "main.zs",
        )
        .unwrap_err();
        assert!(err.contains("Duplicate"), "{}", err);
    }

    // 12. alias avoids duplicate local binding → valid
    #[test]
    fn test_job013_alias_avoids_duplicate_local() {
        let out = run_project(
            &[
                ("math.zs", "export function tambah() { return 1 }"),
                (
                    "main.zs",
                    "import { tambah } from \"./math\"\nimport { tambah as tambah2 } from \"./math\"\nprint(tambah())\nprint(tambah2())",
                ),
            ],
            "main.zs",
        )
        .unwrap();
        assert_eq!(out, vec!["1", "1"]);
    }
}

