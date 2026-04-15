# Structured Unresolved Relationship Hardening Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan task-by-task.

**Goal:** Finish the structured unresolved-relationship migration for the remaining legacy extractor wave and lock the contract down with shared invariant tests.

**Architecture:** Keep downstream consumers stable and migrate producers. Each remaining extractor should emit `StructuredPendingRelationship` first, then preserve compatibility by degrading to legacy `PendingRelationship`. Canonical extraction and normalization stay the shared contract, with invariant tests proving structured identity survives extend, offset, and rekey flows.

**Tech Stack:** Rust, tree-sitter extractors, Julie canonical extraction pipeline, `cargo test`, `cargo xtask test dev`.

**Spec:** `docs/plans/2026-04-14-structured-unresolved-hardening-design.md`

**Execution note:** This is a light plan for same-session execution. In this harness, the recommended parallel path is `razorback:subagent-driven-development` for independent tasks; if executed sequentially, use `razorback:executing-plans`.

**Testing:** Follow `@test-driven-development`. For each task, write or tighten a failing test first, run the narrow test to confirm RED, implement the smallest fix, then rerun the same narrow test for GREEN. Run `cargo xtask test dev` once after the full batch.

---

### Task 1: Expand Shared Invariant Coverage

**Files:**
- Modify: `crates/julie-extractors/src/tests/relationship_precision.rs`
- Modify: `crates/julie-extractors/src/base/results_normalization.rs`
- Modify: `crates/julie-extractors/src/tests/path_identity.rs`
- Modify: `crates/julie-extractors/src/tests/api_surface.rs`

**What to build:** Strengthen the shared invariant suite so the structured unresolved contract is explicit before the remaining language migrations land. This task should prove what must remain true regardless of language-specific parser details.

**Approach:**
- Add failing tests for `ExtractionResults::extend`, `apply_record_offset`, and `rekey_normalized_locations` as they apply to `structured_pending_relationships`.
- Keep this task narrow: if the invariants already hold, the change can be test-only. If a test exposes a bug in normalization or rekeying, fix that bug here before touching more languages.
- Keep degradation behavior stable: structured entries must still carry a compatible legacy pending payload.

**Acceptance criteria:**
- [ ] `relationship_precision.rs` covers structured extend, offset, rekey, and degradation invariants.
- [ ] Structured targets with colliding terminal names remain distinguishable after normalization.
- [ ] `path_identity.rs` and `api_surface.rs` still reflect canonical parity expectations after the stronger invariant coverage.
- [ ] Narrow invariant tests pass.

### Task 2: Migrate The Systems-Style Extractor Wave

**Files:**
- Modify: `crates/julie-extractors/src/c/mod.rs`
- Modify: `crates/julie-extractors/src/c/relationships.rs`
- Modify: `crates/julie-extractors/src/cpp/mod.rs`
- Modify: `crates/julie-extractors/src/cpp/relationships.rs`
- Modify: `crates/julie-extractors/src/rust/mod.rs`
- Modify: `crates/julie-extractors/src/rust/relationships.rs`
- Modify: `crates/julie-extractors/src/zig/mod.rs`
- Modify: `crates/julie-extractors/src/zig/relationships.rs`
- Modify: `crates/julie-extractors/src/tests/c/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/cpp/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/rust/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/zig/cross_file_relationships.rs`

**What to build:** Add structured pending storage/getters to the remaining systems-style extractors and switch their unresolved-call emitters from raw `PendingRelationship` construction to `BaseExtractor::create_pending_relationship(...)`.

**Approach:**
- Follow the existing JS/TS and OO-wave pattern: add `structured_pending_relationships`, add a getter, and add an `add_structured_pending_relationship` helper if that keeps call sites simple.
- Keep detection logic unchanged. The task is about preserving richer unresolved target shape, not changing whether calls are considered resolved or pending.
- Assert only what the AST supports. For these languages, most cases will remain plain identifier targets, with richer receiver or namespace fields only where the local parser path already exposes them.

**Acceptance criteria:**
- [ ] `c`, `cpp`, `rust`, and `zig` emit structured pending relationships for representative unresolved-call paths.
- [ ] Legacy `pending_relationships` remain populated through compatibility degradation.
- [ ] The language cross-file suites assert the new structured shape without inflating false precision.
- [ ] Narrow per-language tests pass.

### Task 3: Migrate The Dynamic And Package-Aware Wave

**Files:**
- Modify: `crates/julie-extractors/src/go/mod.rs`
- Modify: `crates/julie-extractors/src/go/relationships.rs`
- Modify: `crates/julie-extractors/src/python/mod.rs`
- Modify: `crates/julie-extractors/src/python/relationships.rs`
- Modify: `crates/julie-extractors/src/ruby/mod.rs`
- Modify: `crates/julie-extractors/src/ruby/relationships.rs`
- Modify: `crates/julie-extractors/src/gdscript/mod.rs`
- Modify: `crates/julie-extractors/src/gdscript/relationships.rs`
- Modify: `crates/julie-extractors/src/dart/mod.rs`
- Modify: `crates/julie-extractors/src/dart/pending_calls.rs`
- Modify: `crates/julie-extractors/src/tests/go/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/python/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/ruby/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/gdscript/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/dart/cross_file_relationships.rs`

**What to build:** Migrate the remaining dynamic and package-aware extractors to the structured pending model while preserving any receiver, selector, module, package, or namespace context that the grammar already exposes.

**Approach:**
- Go should preserve package or selector context where pending calls come from package-qualified invocations.
- Python, Ruby, and GDScript should preserve receiver or namespace context when the unresolved path is member-like or module-qualified.
- Dart needs the same treatment in both its generic visit logic and `pending_calls.rs`, because pending-call emission is split across those paths.
- Do not invent metadata. If a parser path only has a terminal identifier, store that cleanly and stop there.

**Acceptance criteria:**
- [ ] `go`, `python`, `ruby`, `gdscript`, and `dart` emit structured pending relationships for representative unresolved-call paths.
- [ ] Package, receiver, or namespace context is preserved where the AST already exposes it.
- [ ] Legacy compatibility stays intact for all migrated languages.
- [ ] Narrow per-language tests pass.

### Task 4: Finish Canonical Registry Coverage And Parity Checks

**Files:**
- Modify: `crates/julie-extractors/src/registry.rs`
- Modify: `crates/julie-extractors/src/manager.rs`
- Modify: `crates/julie-extractors/src/factory.rs`
- Modify: `crates/julie-extractors/src/tests/api_surface.rs`
- Modify: `crates/julie-extractors/src/tests/mod.rs`

**What to build:** Make sure the remaining migrated languages reach canonical extraction through paths that return `structured_pending_relationships`, not older macro-generated paths that only pull `pending_relationships`.

**Approach:**
- Audit the registry paths for the remaining wave and move them onto explicit extraction entrypoints if the macro path still drops structured data.
- Keep the public API centered on canonical extraction. `extract_all`, `extract_canonical`, and compatibility wrappers should agree on structured pending output.
- Extend parity tests only as far as needed to prove the remaining wave is visible through the public surface.

**Acceptance criteria:**
- [ ] Canonical extraction returns `structured_pending_relationships` for the remaining migrated languages.
- [ ] `api_surface.rs` continues to prove parity between canonical and compatibility entrypoints.
- [ ] No migrated language still relies on a registry path that drops structured pending output.
- [ ] Narrow parity tests pass.

### Task 5: Final Verification And Branch Cleanup

**Files:**
- Modify: any touched files from Tasks 1-4 as needed for review cleanup only
- Verify: `crates/julie-extractors/src/tests/`
- Verify: `crates/julie-extractors/src/base/`

**What to build:** Finish the batch cleanly, with no half-migrated extractors, no broken parity tests, and no unverified contract changes.

**Approach:**
- Review the combined diff for accidental semantic drift or fake precision.
- Remove fresh warning noise introduced by the batch if any appears.
- Run the full local regression tier once the narrow RED/GREEN loops are done.

**Acceptance criteria:**
- [ ] All migrated languages still populate both legacy and structured pending outputs.
- [ ] Shared invariant tests, language regressions, and parity tests are green.
- [ ] `cargo xtask test dev` passes.
- [ ] The branch is ready for the next slice: semantic unresolved-edge policy tightening.
