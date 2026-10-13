# ZeroScript

**ZeroScript** is an experimental programming language and runtime, implemented from scratch in Rust with **zero external dependencies** — hand-rolled lexer, parser, compiler, bytecode virtual machine, garbage collector, package manager, and even SHA-256.

Development is **spec-first**: a detailed written specification defines every language rule and runtime behavior before (or together with) the implementation, and the implementation is audited against it continuously.

> **Status:** experimental, under active development. The core language, toolchain, and package ecosystem work end-to-end, with **948 tests passing and 0 failing**.

---

## Highlights

- **Complete toolchain in one binary set** — source → lexer → parser → compiler → bytecode → VM, all implemented in this repository.
- **Spec-first engineering** — every feature maps to numbered specification sections; ambiguities are recorded and resolved explicitly rather than improvised.
- **Async by design** — `async`/`await`, lightweight tasks, `task.all()`, `task.race()`, `task.wait()`, `task.timeout()`, and warnings for failures that were never observed.
- **Real modules** — `import`/`export`/re-export/aliases, circular-import detection, package boundaries, and project-scoped dependencies.
- **Bytecode artifacts** — compile a whole project to a single `.zsbc` file with a SHA-256 checksum; run it anywhere without the original source.
- **Package manager (`zpm`)** — `project.zson` manifest, lockfile, SemVer selectors, Git sources, local registries, content-integrity verification, and publishing with Git release tags.
- **Safety rails** — strict boolean conditions (no truthy/falsy), `const` enforcement, execution limits (time, instructions, memory, call depth), and structural GC rooting.
- **Zero dependencies** — the entire runtime, standard library, HTTP client, JSON/ZSON parser, and SHA-256 are written in-house.

---

## Install

Prebuilt binaries for Linux (x86_64/arm64, static musl), macOS (x64/arm64), and Windows (x86_64) ship with every GitHub release. On Linux/macOS, one command — no need to pick a file:

```bash
curl -fsSL https://raw.githubusercontent.com/ZeroScripts-lang/Zeroscripts/main/install.sh | sh
```

The script auto-detects OS/architecture, installs `zsc` + `zpm` to `~/.zeroscript/bin`, and prints the PATH line to add. Override the location with `ZSC_INSTALL_DIR`; pin a version with `ZSC_TAG`. Windows: download the `.zip` asset from the release page.

## Quick Start

Requires a stable [Rust toolchain](https://rustup.rs/).

```bash
git clone <your-repo-url>
cd ZeroScript
cargo build --release

# Run a script
./target/release/zsc examples/hello.zs        # → 42

# Guided tour of the whole language (one file)
./target/release/zsc examples/demo.zs

# Run code without a file
./target/release/zsc -e 'print(1 + 2)'        # → 3

# Scaffold a new project (interactive: name, version, license, ...)
mkdir my-app && cd my-app
../target/release/zsc init
# creates project.zson + main.zs, then:
../target/release/zsc main.zs

# Compile to bytecode and run the artifact
../target/release/zsc compile main.zs
../target/release/zsc run main.zsbc
```

Run the full test suite:

```bash
cargo test        # 948 tests, 0 failed
```

---

## Language Tour

Everything below uses syntax that is exercised by the test suite.

**Variables, functions, collections**

```zs
const user = {name: "Ada"}
let items = [1, 2, 3]

function add(a, b) {
    return a + b
}

print(add(20, 22))          // 42
print(user.name)            // Ada
```

**Modules**

```zs
// math.zs
export function add(a, b) {
    return a + b
}

// main.zs
import { add } from "./math"
print(add(20, 22))          // 42
```

**Async / await**

```zs
async function quick() {
    let values = [77, 88]
    return values
}

async function main() {
    let t = quick()
    await task.sleep(1)
    print((await t)[1])     // 88
}

main()
```

Tasks compose with `task.all()`, `task.race()`, `task.wait()`, and `task.timeout()`; a task failure that is never awaited is reported at shutdown instead of being silently dropped.

**Error handling**

```zs
try {
    let l = [1, 2]
    let x = l[9]
} catch e {
    print.err(e)
}
```

`finally` blocks run when leaving a `try` region, including across `break`/`continue`.

---

## The `zsc` Command

| Command | Description |
|---|---|
| `zsc <file.zs> [args...]` | Compile and run a script (`system.args()` = `[args...]`) |
| `zsc -e '<code>' [args...]` | Run code from a string; relative imports resolve from the current directory |
| `zsc compile <file.zs> [-o out.zsbc]` | Compile a whole program (entry + all imports) to one bytecode artifact — never executes |
| `zsc run <file.zsbc> [args...]` | Verify the checksum, validate, and run a bytecode artifact without source |
| `zsc init` | Interactive project scaffold: manifest + starter script + default `zpm run` scripts |
| `zsc --version` | Print version |

Exit codes follow the specification: `0` on success, `1` on usage/compile/runtime failure.

## The `zpm` Package Manager

| Command | Description |
|---|---|
| `zpm install [name]` | Declare and install a dependency (registry or `git:<url>[#ref]`) |
| `zpm remove <pkg>` | Undeclare a dependency, prune `.zsc_m/`, rewrite the lockfile |
| `zpm update [pkg]` | Re-resolve to the newest matching versions and re-lock |
| `zpm run <script> [args]` | Run a script from `project.zson`, forwarding the exit code |
| `zpm publish` | Validate metadata and publish the version to the registry (Git-tagged) |

Dependencies live in `project.zson`, are locked in `project.lock.zson`, and are installed into `.zsc_m/`. Version selectors support exact versions and `x` wildcards; content is SHA-256 verified against the lockfile/registry index before use, with a verified local cache.

---

## Architecture

```
source (.zs)
   │  lexer.rs / token.rs      — tokens (keywords, symbolic operators)
   │  parser.rs / ast.rs       — AST, import/re-export extraction
   │  compiler.rs              — single-pass compiler → bytecode
   │                            (constants, functions, default-parameter bodies,
   │                             per-module units merged into one program)
   ▼
BytecodeProgram  ──►  bytecode_io.rs  ──►  app.zsbc  (payload ‖ SHA-256)
   │
   │  vm/                      — stack VM: validation before execution,
   │                            frames, scopes, closures on the heap,
   │                            async scheduler, execution limits
   │  heap.rs / gc.rs          — ref-counted heap + mark-sweep GC
   ▼
output (stdout / stderr / exit code)
```

Supporting layers: `module.rs` (module graph, package resolution), `package.rs` + `git.rs` + `version.rs` + `hash.rs` (`zpm` internals), `json.rs` (JSON + ZSON), `lexer`-level stdlib builtins for strings, lists, objects, colors, HTTP, and system info.

---

## Repository Layout

```
src/          language implementation (compiler, VM, stdlib, zsc, zpm) — 30 files
tests/        end-to-end CLI test suites (zsc compile/run/eval/init, zpm)
examples/     demo.zs feature tour, runnable example scripts, and a sample project
Cargo.toml    build manifest — zero external crates
LICENSE       MIT
```

The full language specification (in Indonesian, ~158 numbered sections across the core documents) is the source of truth for semantics and is what every implementation job is audited against; it is maintained alongside the project but is not part of this repository snapshot.

---

## Current Progress

**Implemented and covered by the 948-test suite (0 failing):**

- **Frontend** — complete lexer (keywords, string/number literals, symbolic `&&`/`||` operators), parser (21 grammar areas), and AST, all fully covered by tests.
- **Compiler** — 37/37 tracked compiler/opcode areas complete: expressions, control flow (unlimited `else if` depth, `switch`, ternary, compound assignment, postfix `++/--`), functions (named/anonymous/arrow), default parameters evaluated at call time (including inside `async` functions), closures compiled to shared environments, `const` binding checks, block/function scoping.
- **Runtime values & heap** — numbers, strings, booleans, null, lists, objects, functions/closures; all collections heap-allocated with shared references (no accidental cloning on assignment); `HeapRefHandle` with deterministic drop.
- **VM execution** — full instruction set with pre-execution validation (§124/§125 header + version checks), per-frame value stacks (§116), bytecode serialization with checksummed artifacts, execution limits (wall-time, instructions, memory, call depth, active-task policy), GC with structurally complete roots.
- **Modules** — static import graph, re-export/alias/conflict rules, circular detection, single-initialization guarantee, package import boundaries (`pkg/sub` resolution, escape protection, dependency scoping).
- **Async** — `async`/`await`, task lifecycle, `all`/`race`/`wait`/`timeout` combinators with input-order results and fail-fast semantics, unobserved-failure warnings, scheduler integration with execution limits.
- **Error handling** — `try`/`catch`/`finally` (including `break`/`continue` crossing `finally`), error classes, stderr channel, exit-code conventions.
- **Standard library** — string/list/object helpers, ANSI colors, JSON + ZSON parsing, HTTP client (automatic redirects with a capped limit), `system.*` (program arguments), `task.*`.
- **Dependencies & packaging** — `project.zson`, `project.lock.zson`, `package.zson`, SemVer resolution with selectors, Git sources, local filesystem registry with optional index files, integrity verification, caching, multi-version layouts, removal/update/publish flows, dependency scope and cycle checks.
- **Tooling** — `zsc` (run / eval / compile / run-bytecode / init), `zpm` (install / remove / update / run / publish), documented exit-code behavior.

**Disclosed gaps currently tracked:**

- Debug/source-location info inside `.zsbc` artifacts (would be a bytecode version bump).
- A networked package registry (registries today are local directories + Git sources, which is what the spec requires of the registry layer).

**Development discipline:** 20 tracked implementation jobs (008–027) have been completed in dependency order, each with its own job file, test batch, and spec-coverage audit. Specification ambiguities are never resolved by guessing — they are recorded as explicit decisions.

---

## Testing

```bash
cargo test          # full suite: 948 passed, 0 failed
```

The suite spans lexer, parser, compiler, VM, heap/GC, modules, async, stdlib, dependencies, and end-to-end CLI behavior (including bytecode round-trips, corruption rejection, eval-mode semantics, and `zpm` flows). Every feature lands with tests, and existing tests are never weakened to make a change pass.

---

## License

[MIT](LICENSE) © 2026 ZeroScripts-lang
