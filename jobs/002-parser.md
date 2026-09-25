Job 002 — Parser

Objective

Implement the ZeroScript parser according to the language specification.

The parser must consume the token stream produced by the existing lexer and construct an Abstract Syntax Tree (AST) representing valid ZeroScript source code.

Required Reading

Before implementing:

1. Read "AGENTS.md".
2. Read the relevant parser, syntax, expression, statement, function, collection, object, control-flow, and error-handling sections in "Language.md".
3. Read "INDEX.md" to locate the relevant specification sections.
4. Read the existing lexer implementation:
   - "src/token.rs"
   - "src/lexer.rs"
   - "src/error.rs"
5. Inspect the existing tests and project structure before making changes.

The specification is the source of truth. Do not invent syntax or semantics that are not defined by the specification.

Scope

Implement only the parser and AST layer required by the current ZeroScript specification.

1. AST

Create an appropriate AST representation for the syntax supported by the specification.

The AST should represent, where defined:

- Program/source unit
- Variable declarations
- Literals
- Identifiers
- Expressions
- Unary expressions
- Binary expressions
- Assignment expressions
- Compound assignment
- Postfix increment/decrement
- Function calls
- Member/property access
- Index access
- Lists
- Objects
- Function definitions
- Anonymous functions
- Arrow functions
- "return"
- "if / else"
- Ternary expressions
- "loop"
- "while"
- "break"
- "continue"
- "switch / case"
- "try / catch / finally"
- Other syntax explicitly defined by the relevant "Language.md" sections

Only implement constructs that are actually specified.

2. Expression Parsing

Implement correct operator precedence and associativity according to "Language.md".

At minimum, account for the specified:

- Arithmetic operators
- Comparison operators
- Equality operators
- Boolean operators
- Unary operators
- Assignment operators
- Postfix operators
- Function calls
- Member access
- Index access
- Ternary expressions

Do not change the semantics of operators while implementing precedence.

3. Statements

Parse the statement forms defined by the specification, including declarations, control flow, functions, returns, and error-handling constructs where applicable.

4. Parser Errors

Invalid syntax must produce clear parser errors.

Errors should contain useful source-location information where practical, such as:

- line
- column
- unexpected token
- expected token or construct

Do not silently recover from invalid syntax unless the specification explicitly defines recovery behavior.

5. Lexer Integration

Use the existing lexer/token implementation from Job 001.

Do not duplicate lexical analysis inside the parser.

The parser should operate on tokens rather than raw source characters.

Out of Scope

Do NOT implement:

- Bytecode generation
- Compiler
- ZeroVM execution
- Runtime evaluation
- Garbage collection
- Module loading
- Dependency resolution
- Standard-library behavior
- Native operations
- Runtime optimization

Do not modify the language specification unless the current job explicitly requires resolving a documented specification issue.

Tests

Add parser unit tests covering the syntax implemented by this job.

Tests should include, where supported by the specification:

- Empty program
- Variable declarations
- Constants
- Literals
- Identifiers
- Unary expressions
- Binary expressions
- Operator precedence
- Equality/comparison expressions
- Boolean expressions
- Assignments
- Compound assignments
- Postfix increment/decrement
- Function calls
- Member access
- Index access
- Lists
- Objects
- Function declarations
- Anonymous functions
- Arrow functions
- "return"
- "if / else"
- Ternary expressions
- "loop"
- "while"
- "break"
- "continue"
- "switch / case"
- "try / catch / finally"
- Invalid syntax
- Unexpected tokens
- Missing delimiters
- Other syntax explicitly defined in the specification

Tests must verify the resulting AST structure where appropriate, not merely that parsing succeeds.

Compatibility With Job 001

Do not break the existing lexer tests.

Run the complete test suite after implementation:

cargo test

All existing tests and all new parser tests must pass.

Warnings should be investigated and cleaned up where they are caused by the implementation.

Completion Criteria

Job 002 is complete when:

- [ ] AST structures are implemented.
- [ ] Parser consumes the existing lexer token stream.
- [ ] All specified parser constructs within scope are supported.
- [ ] Operator precedence and associativity follow "Language.md".
- [ ] Parser errors are reported clearly.
- [ ] Parser tests are implemented.
- [ ] Existing lexer tests still pass.
- [ ] "cargo test" passes with all tests successful.
- [ ] No unexplained compiler warnings remain.
- [ ] No unrelated runtime/compiler/VM features were implemented.

Final Report

After completing the job, report:

1. Files created or modified.
2. AST structures implemented.
3. Parser features implemented.
4. Number of tests added.
5. Total tests run and result.
6. Any warnings encountered and their resolution.
7. Any specification ambiguity discovered.
8. Any unresolved issue that should be recorded for a future job.

Do not claim completion unless the implementation and tests have actually been verified.
