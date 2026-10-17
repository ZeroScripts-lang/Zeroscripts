Job 001 — Lexer

Objective

Implement the ZeroScript lexer/tokenizer according to the language specification.

Read First

Read:

- "AGENTS.md"
- "spec/Language.md"
- "spec/INDEX.md"

Inspect the existing project structure and source code before implementing anything.

Only use other specification files if the implementation requires information referenced by the current lexer design.

Scope

Implement the lexer/tokenizer responsible for converting ZeroScript source text into a stream of tokens.

The lexer should recognize only syntax and lexical elements defined by the specification.

Do not implement the parser, bytecode compiler, ZeroVM, or unrelated language features as part of this job.

Requirements

Follow "spec/Language.md" as the source of truth.

Implement the lexical handling required by the specification, including the relevant:

- identifiers
- keywords
- literals
- operators
- punctuation
- delimiters
- comments
- whitespace
- lexical errors

Use the exact ZeroScript syntax defined by the specification.

Do not invent additional syntax or silently accept syntax that the specification does not define.

Error Handling

Invalid lexical input must produce a clear lexer error according to the project's existing error-handling architecture.

Do not silently discard invalid characters or malformed tokens.

Tests

Add unit tests covering the implemented lexer behavior.

Tests should include:

- valid tokens
- keywords
- identifiers
- literals
- operators and punctuation
- comments and whitespace
- invalid characters
- malformed lexical input

Use the project's existing testing conventions.

Completion Criteria

This job is complete when:

- the lexer is implemented
- relevant unit tests are added
- all relevant tests pass
- the implementation matches "spec/Language.md"
- no parser/VM implementation has been unnecessarily introduced
- no unrelated files or systems have been modified

Final Report

After completing the job, report:

1. Files changed
2. Lexer functionality implemented
3. Tests added
4. Tests executed
5. Test results
6. Any unresolved specification ambiguity or dependency
