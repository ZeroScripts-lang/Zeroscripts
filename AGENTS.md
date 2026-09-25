ZeroScript — Agent Instructions

## Implementation Language

The reference implementation of ZeroScript is written in Rust.

Use Rust for the ZeroScript implementation unless a job explicitly states otherwise.

1. Project

ZeroScript is an experimental programming language and runtime project.

The specification is the source of truth for language semantics and runtime behavior.

Primary specification:

- "spec/Language.md"
- "spec/Modules.md"
- "spec/Dependencies.md"
- "spec/Async.md"
- "spec/Bytecode.md"
- "spec/ZeroVM.md"
- "spec/StandardLibrary.md"
- "spec/Runtime.md"
- "spec/ISSUES.md"
- "spec/INDEX.md"

"ZeroScripts.md" is the original master specification and should be treated as a reference/archive.

---

2. Before Working

Before implementing anything:

1. Read this file.
2. Read the relevant job file from "jobs/".
3. Read only the specification files/sections relevant to that job.
4. Inspect the existing source code and project structure.
5. Understand existing architecture before making changes.

Do not start implementing before understanding the scope of the current job.

---

3. Specification Rules

The specification is authoritative.

Do not:

- invent new syntax without specification support
- invent new semantics
- silently change existing semantics
- simplify behavior by removing specified rules
- rewrite specification text
- implement unrelated features
- modify other systems just because an alternative design seems better

If the specification is ambiguous, contradictory, or incomplete:

1. Stop at the affected decision.
2. Explain the ambiguity.
3. Check "spec/ISSUES.md".
4. Do not guess a new language rule.

Implementation decisions must follow the specification, not personal preference.

---

4. Scope

Work only on the current job.

Do not expand the task unnecessarily.

If another component is required as a dependency for the current job, implement only the minimum necessary part or clearly report the dependency before proceeding.

Avoid large unrelated refactors.

---

5. Implementation Principles

Prefer:

- simple architecture
- explicit behavior
- predictable execution
- modular components
- clear error handling
- maintainable code
- testable components

Do not optimize prematurely.

Do not introduce unnecessary dependencies.

Do not over-engineer features that are not required by the specification.

---

6. Tests

Every implemented behavior should have appropriate tests.

After implementation:

1. Run the relevant tests.
2. Run broader tests when practical.
3. Check for regressions.
4. Fix failures caused by the current changes.

Do not remove or weaken existing tests merely to make the implementation pass.

---

7. Changes

Before modifying files, understand why each file needs to change.

Keep changes focused on the current job.

Do not modify specification files unless the current job explicitly requires a specification update.

If an implementation reveals a genuine specification problem, record it in "spec/ISSUES.md" instead of silently changing behavior.

---

8. Job Completion

A job is complete only when:

- the requested functionality is implemented
- relevant tests exist
- relevant tests pass
- no known regression remains
- implementation matches the specification
- changes are limited to the job scope

At completion, report:

- what was implemented
- files changed
- tests added/changed
- tests executed
- test results
- unresolved issues or dependencies

Do not claim completion if an important part of the job is unfinished.

---

9. Git

Keep commits focused.

Prefer one logical implementation task per commit.

Use clear commit messages describing the actual change.

Do not commit unrelated changes.

---

10. Important

The current job has priority over future jobs.

Do not implement future phases early just because their specification is already available.

Read the specification broadly enough to understand dependencies, but implement narrowly according to the current job.

When in doubt:

Spec → Job → Existing Code → Implement → Test → Review

---

11. CHECK.md Maintenance

"CHECK.md" is the living audit of implementation status against the
specification. It is not a one-time report.

After completing a job:

1. Update every row in "CHECK.md" that the job's Required Final Report
   claims as newly implemented, with concrete evidence (file:line, test
   name).
2. Never change a status to "Implemented" without a passing test that
   exercises the actual behavior, not just the existence of a
   struct/function/opcode with a matching name.
3. If a job only partially closes a gap, update the status to "Partially
   Implemented" and describe precisely what remains — do not mark the
   whole domain done because most of it works.
4. If new gaps are discovered while implementing (things the spec requires
   that no current or future job file covers), add them to "CHECK.md"
   under a "Candidate Gaps for Future Work" style section, and mention them
   in the job's Required Final Report. Do not silently drop them.
5. Do not delete historical rows from "CHECK.md" to make it shorter;
   superseded rows should be updated in place, not removed.

Before starting a new job, read "CHECK.md" to understand what previous jobs
actually verified — treat it as a map of the current implementation
boundary, not as specification.

---

12. Spec Citation Discipline

Every non-trivial implementation decision that is not a mechanical
transcription of the spec's own example must be traceable to a specific
section number.

- When a job file or a spec section is ambiguous, silent, or appears to
  conflict with another section, do not resolve it by picking whichever
  reading is easiest to implement. Stop, write out the two (or more)
  readings, check "ISSUES.md", and report the decision explicitly in the
  Required Final Report.
- Do not invent field names, error message formats, config file syntax, or
  behavior details that the spec does not state, even when a "reasonable"
  default seems obvious. If a default must be chosen because the spec is
  silent on a mechanical detail (e.g. exact wording of an error message),
  say so explicitly rather than presenting it as spec-derived.
- A job file describing a feature with a section number attached (e.g.
  "§35") is a pointer to the source of truth, not a paraphrase to be
  trusted blindly — re-read the cited section before implementing it.

---

13. Standard Library Calling Convention

Per Bytecode.md §145-§146, the VM's core opcode set is meant to stay small;
standard library and native functionality are meant to go through a single
native-call boundary (conceptually "CALL_NATIVE") rather than each stdlib
function getting its own bespoke opcode or VM special-case.

- New standard library work (string/list/object/color/file/system/json/http)
  should be implemented against this boundary from the start where
  practical, even before the formal bytecode versioning/validation pass
  (see the bytecode hardening job) exists.
- If a stdlib job is implemented before the native-call boundary is
  formalized, it must remain easy to migrate later without changing
  observable behavior — do not couple stdlib semantics to whatever ad hoc
  dispatch mechanism is used first.

---

14. Job Sequencing & Dependencies

Job files are numbered in dependency order starting from 008. A job that
states a dependency on an earlier job number must not be started until that
earlier job's Required Final Report confirms its acceptance criteria are
met.

- Do not skip ahead to a later job "because it looks easier" or "because
  the spec section is simpler" — later jobs frequently assume correctness
  fixed by earlier ones (e.g. every standard library and module job assumes
  Job 008's shared-reference and const-enforcement fixes are in place).
- If a later job's own testing reveals that an earlier "completed" job did
  not actually satisfy its acceptance criteria, stop, report the
  regression, and treat it as a defect in the earlier job rather than
  working around it locally.
- New jobs beyond the currently planned range should follow the same
  numbering convention (next integer, descriptive filename) and the same
  required structure (Objective, Required Reading, Scope, Out of Scope,
  Testing Requirements, Acceptance Criteria, Required Final Report) used by
  Job 007 onward.
