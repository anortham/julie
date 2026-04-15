# Semantic Unresolved Edge Policy Tightening Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan task-by-task.

**Goal:** Remove obvious runtime and stdlib noise from unresolved-edge emission while preserving plausible cross-file project calls.

**Architecture:** Keep policy local to each extractor. Go should use import-backed package context to drop known stdlib package calls, PowerShell should drop explicit built-in cmdlets by name, and Bash plus R should serve as locked reference cases through regression tests. The shared contract is narrow: runtime noise is absent, project-like unresolved calls remain present.

**Tech Stack:** Rust, tree-sitter extractors, Julie extractor regression suites, `cargo test`, `cargo xtask test dev`.

**Spec:** `docs/plans/2026-04-14-semantic-unresolved-edge-policy-design.md`

**Execution note:** This is a light plan for same-session execution. In this harness, the recommended parallel path is `razorback:subagent-driven-development` for independent tasks; if executed sequentially, use `razorback:executing-plans`.

**Testing:** Follow `@test-driven-development`. For each task, write or tighten a failing test first, run the narrow test to confirm RED, implement the smallest fix, then rerun the same narrow test for GREEN. Run `cargo xtask test dev` once after the full batch.

**Git:** Do not create commits unless the user explicitly asks for one.

---

### Task 1: Tighten Go stdlib package filtering

**Files:**
- Modify: `crates/julie-extractors/src/tests/go/cross_file_relationships.rs:68-248`
- Modify: `crates/julie-extractors/src/go/relationships.rs:95-193`

**What to build:** Replace the current Go builtin-noise expectation with the policy we want: project package calls stay pending, stdlib package calls do not. `utils.HelperFunction(...)` remains the positive control, and `fmt.Println(...)` becomes the negative control.

**Approach:**
- Tighten or rename the existing builtin Go regression so it asserts that `fmt.Println(...)` creates no entry in either `pending_relationships` or `structured_pending_relationships`.
- Keep `test_cross_package_function_call_creates_pending_relationship()` as the positive control for project package calls, including its structured package-context assertions.
- In `extract_call_relationships()`, filter only the package-qualified unresolved branch where the qualifier resolves to a `SymbolKind::Import` symbol.
- Parse the existing import symbol signature to recover the import path and add a small exact-match stdlib predicate seeded with `fmt`. Keep the helper narrow and easy to extend.
- If the qualifier is not an import symbol, or the import path cannot be recovered, keep the pending edge.

**Acceptance criteria:**
- [ ] `fmt.Println(...)` produces no legacy or structured pending edge.
- [ ] `utils.HelperFunction(...)` still produces a structured pending edge with package context intact.
- [ ] Same-file Go calls still resolve directly.
- [ ] Narrow Go regression test passes.

### Task 2: Tighten PowerShell built-in cmdlet filtering

**Files:**
- Modify: `crates/julie-extractors/src/tests/powershell/cross_file_relationships.rs:49-319`
- Modify: `crates/julie-extractors/src/powershell/relationships.rs:35-98`

**What to build:** Drop pending edges for known PowerShell runtime cmdlets without breaking cross-file project functions that use the same Verb-Noun naming shape.

**Approach:**
- Tighten `test_builtin_cmdlet_no_pending_relationship()` so it asserts absence in both `pending_relationships` and `structured_pending_relationships` for `Write-Output` and `Get-ChildItem`.
- Keep `test_cross_file_function_call_creates_pending_relationship()` and `test_verb_noun_cmdlet_call_creates_pending_relationship()` as positive controls for project-defined functions such as `Get-Data` and `Export-CustomObject`.
- Add a small `is_builtin_cmdlet()` predicate in `powershell/relationships.rs` and apply it only on the unresolved-command path before pending creation.
- Keep the predicate explicit by exact cmdlet name. Do not filter on Verb-Noun shape alone.

**Acceptance criteria:**
- [ ] `Write-Output` and `Get-ChildItem` produce no legacy or structured pending edge.
- [ ] Cross-file project functions in PowerShell still produce pending edges.
- [ ] User-defined Verb-Noun functions remain pending when unresolved locally.
- [ ] Narrow PowerShell regression test passes.

### Task 3: Lock Bash and R builtin filtering as contract tests

**Files:**
- Modify: `crates/julie-extractors/src/tests/bash/cross_file_relationships.rs:49-279`
- Modify: `crates/julie-extractors/src/tests/r/cross_file_relationships.rs:247-320`
- Modify: `crates/julie-extractors/src/bash/relationships.rs:74-94`
- Modify: `crates/julie-extractors/src/r/relationships.rs:253-297`

**What to build:** Turn the existing Bash and R builtin filters into explicit invariants so Go and PowerShell are tightening toward an established contract rather than inventing a new one.

**Approach:**
- Add a Bash regression that exercises filtered command names already listed in `is_builtin_command()`, including one shell builtin and one common external command, and assert that neither creates a pending edge.
- Tighten the R builtin regression so it asserts both no pending edge and no synthetic resolved relationship for base-language calls such as `print()` and `mean()`.
- Prefer test-only changes. Touch the production builtin predicates only if the new tests expose drift between current behavior and the intended contract.

**Acceptance criteria:**
- [ ] Bash tests explicitly prove builtin and common external command noise is dropped.
- [ ] R tests explicitly prove builtin function noise is dropped.
- [ ] No new synthetic builtin relationship IDs appear in the R path.
- [ ] Narrow Bash and R regression tests pass.

### Task 4: Final verification for the batch

**Files:**
- Verify: `crates/julie-extractors/src/go/relationships.rs`
- Verify: `crates/julie-extractors/src/powershell/relationships.rs`
- Verify: `crates/julie-extractors/src/tests/go/cross_file_relationships.rs`
- Verify: `crates/julie-extractors/src/tests/powershell/cross_file_relationships.rs`
- Verify: `crates/julie-extractors/src/tests/bash/cross_file_relationships.rs`
- Verify: `crates/julie-extractors/src/tests/r/cross_file_relationships.rs`

**What to build:** Finish the batch with the policy tightened, the reference tests explicit, and no accidental over-filtering of project calls.

**Approach:**
- Review the combined diff for one failure mode in particular: dropping project-like unresolved edges because the filter grew beyond the evidence available in the extractor.
- Run the narrow RED and GREEN loops for each task first, then run `cargo xtask test dev` once at the end.
- Clean up any fresh warning noise introduced by the batch before calling it done.

**Acceptance criteria:**
- [ ] Targeted Go, PowerShell, Bash, and R regressions are green.
- [ ] `cargo xtask test dev` passes.
- [ ] The branch is ready for the next unresolved-edge policy slice without carrying runtime-noise regressions forward.
