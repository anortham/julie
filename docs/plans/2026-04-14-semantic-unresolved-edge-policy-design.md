# Semantic Unresolved Edge Policy Tightening

**Status:** Design
**Author:** OpenCode
**Date:** 2026-04-14
**Area:** `crates/julie-extractors/src/go/relationships.rs`, `crates/julie-extractors/src/powershell/relationships.rs`, `crates/julie-extractors/src/tests/`, existing builtin-filtered extractors

---

## Problem

The branch now preserves structured unresolved call data across the remaining legacy extractor wave. That solved the data-shape problem. The next problem is semantic quality: some extractors still emit unresolved edges that are not plausible workspace relationships.

The inconsistency is visible today:

- Bash already drops obvious shell builtins and common external commands.
- R already drops base-language builtins.
- Go still emits pending calls for stdlib package calls like `fmt.Println`.
- PowerShell still emits pending calls for built-in cmdlets like `Write-Output` and `Get-ChildItem`.

Those edges are noise. They consume graph budget, inflate relationship counts, and create confidently wrong resolution candidates. They also make the structured pending work look worse than it is, because the richer payload now preserves context for edges that should not exist in the first place.

## Goals

- Tighten unresolved-edge emission so extractors keep only plausible workspace-facing unresolved calls.
- Drop obvious runtime, stdlib, and shell/cmdlet noise before it becomes pending graph data.
- Preserve unresolved edges that still represent likely cross-file project code.
- Lock the policy with regression tests instead of leaving behavior implicit or debug-print driven.

## Non-Goals

- Exhaustive builtin catalogs for every supported language.
- Redesigning cross-file resolution consumers.
- Rewriting relationship extraction across all 33 languages in one slice.
- Dropping unresolved edges when the extractor lacks enough evidence to know whether the target is project code.

## Scope

### Extractors to tighten now

- `crates/julie-extractors/src/go/relationships.rs`
- `crates/julie-extractors/src/powershell/relationships.rs`

### Extractors with existing policy to lock down in tests

- `crates/julie-extractors/src/bash/relationships.rs`
- `crates/julie-extractors/src/r/relationships.rs`

### Supporting files likely involved

- `crates/julie-extractors/src/go/specs.rs`
- `crates/julie-extractors/src/powershell/commands.rs`

### Regression suites to extend

- `crates/julie-extractors/src/tests/go/cross_file_relationships.rs`
- `crates/julie-extractors/src/tests/powershell/cross_file_relationships.rs`
- `crates/julie-extractors/src/tests/bash/cross_file_relationships.rs`
- `crates/julie-extractors/src/tests/r/cross_file_relationships.rs`

## Policy

### 1. Pending edges must be project-plausible

The rule for this slice is simple:

> Only emit a pending relationship when the unresolved target could plausibly resolve to user or workspace code.

If the extractor can tell the target is a language/runtime builtin, shell builtin, standard-library package call, or PowerShell built-in cmdlet, it should drop the edge instead of preserving it.

If the extractor cannot tell, keep the edge. False negatives are better than false positives here, but only when the extractor has evidence. This slice is about removing obvious noise, not guessing harder.

### 2. Do not centralize all language knowledge into one generic helper

The contract is shared, but the evidence is language-specific.

- Bash knows command names and already has a builtin/external command filter.
- R knows base-language function names and already has a builtin filter.
- Go can distinguish package-qualified calls and imported package aliases.
- PowerShell can distinguish function calls from command/cmdlet invocations and can filter known built-in cmdlets.

Trying to force this into one shared mega-table would be sloppy. The design should keep the policy local to each extractor, with tests encoding the shared contract.

### 3. Go: drop obvious stdlib package calls, keep project package calls

For Go, the noisy case is package-qualified calls where the qualifier is an imported stdlib package alias.

Examples:

- Drop: `fmt.Println(...)`
- Keep: `utils.HelperFunction(...)`

The extractor already builds an `UnresolvedTarget` with receiver-style package context and knows when the unresolved target came from a package call. The missing piece is deciding whether that package alias points at a standard-library import rather than project code.

This slice should use the import information already captured in Go symbols and signatures to build a narrow stdlib check. The check should be explicit and conservative:

- match the imported package alias back to its import path,
- drop only when that import path is a known stdlib package path,
- keep all other unresolved package-qualified calls.

No heuristic based on directory layout, dots in package names, or module-root guessing. That would break on real Go projects.

### 4. PowerShell: drop built-in cmdlets, keep user-defined Verb-Noun functions

PowerShell is trickier because built-in cmdlets and user functions share the Verb-Noun shape.

Examples:

- Drop: `Write-Output`, `Get-ChildItem`
- Keep: `Export-CustomObject`, cross-file project functions like `Get-Data`

The filter therefore cannot be “drop all Verb-Noun commands.” That would torch real project code.

This slice should add an explicit built-in cmdlet predicate in the relationship layer and use it only for known PowerShell runtime cmdlets. The list does not need to be exhaustive; it needs to cover the obvious graph-noise cases we can defend with tests. Unknown commands should remain pending.

### 5. Existing filters become contractual

Bash and R already follow the policy in code, but their tests do not pin it cleanly enough.

This slice should tighten those suites so builtin filtering is asserted, not implied by debug output or loose comments. That gives the new Go and PowerShell rules a stable cross-language reference point.

## Implementation Outline

1. Tighten Go tests first so `fmt.Println` is expected to produce no pending edge while `utils.HelperFunction` still does.
2. Add the smallest Go-side import-aware stdlib filter in `go/relationships.rs`, using import symbol/signature data rather than path-shape guessing.
3. Tighten PowerShell tests first so built-in cmdlets are asserted absent while user-defined Verb-Noun functions remain pending.
4. Add a small built-in cmdlet predicate in `powershell/relationships.rs`; keep it narrow and test-backed.
5. Strengthen Bash and R regression tests so their existing builtin drop behavior becomes explicit.
6. Run `cargo xtask test dev` after the full batch.

## Risks And Guardrails

### Risk: over-filtering project code

The failure mode is dropping unresolved edges that should survive for cross-file resolution.

Guardrails:

- keep Go filtering tied to known stdlib import paths, not generic package-shape guesses,
- keep PowerShell filtering tied to known built-in cmdlet names, not generic Verb-Noun matching,
- retain the existing cross-file project-call tests and tighten them where needed.

### Risk: pretending the runtime catalog is complete

We do not have perfect builtin inventories for every language, and pretending otherwise would make the policy brittle.

Guardrail:

- limit this slice to obvious, defensible noise cases,
- document that unknown targets remain pending unless the extractor has strong evidence to drop them.

### Risk: policy drift across languages

Without tests, each extractor can wander into its own interpretation of what “unresolved” means.

Guardrail:

- use Bash and R as locked reference cases,
- encode the shared contract in language regression suites: builtins/runtime noise absent, project-like cross-file calls preserved.

## Acceptance Criteria

- [ ] Go no longer emits `pending_relationships` or `structured_pending_relationships` for stdlib package calls like `fmt.Println`.
- [ ] Go still emits structured unresolved edges for project-like cross-package calls such as `utils.HelperFunction`.
- [ ] PowerShell no longer emits pending edges for built-in cmdlets such as `Write-Output` and `Get-ChildItem`.
- [ ] PowerShell still emits pending edges for cross-file project functions, including user-defined Verb-Noun functions.
- [ ] Bash tests explicitly assert that builtin/external command noise is dropped.
- [ ] R tests explicitly assert that builtin function noise is dropped.
- [ ] `cargo xtask test dev` passes after the batch.

## Test Strategy

- Follow strict TDD for each extractor change:
  1. write or tighten a failing regression test,
  2. run the exact test and confirm RED,
  3. implement the smallest policy change,
  4. rerun the same test and confirm GREEN.
- Use the existing Go and PowerShell cross-file suites as the RED entrypoints for behavior changes.
- Use Bash and R cross-file suites to turn existing filters into explicit invariants.
- Run `cargo xtask test dev` once after the batch is complete.

## Exit Condition For This Slice

This slice is done when the extractor set above only emits unresolved edges that still look like plausible workspace relationships, and the regression suites make that contract hard to accidentally break.
