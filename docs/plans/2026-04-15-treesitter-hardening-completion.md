# Tree-Sitter Hardening Completion Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan task-by-task.

**Goal:** Finish the remaining extractor hardening gaps so `crates/julie-extractors` satisfies the world-class exit criteria from the 2026-04-14 hardening plan.

**Architecture:** Close the remaining bypasses around the canonical extraction pipeline, finish the last structured-pending migrations, then tighten shared semantic policy and replace the missing invariant coverage. End with consumer-facing docs, a durable review artifact, and fresh branch-level verification.

**Tech Stack:** Rust, tree-sitter, julie-extractors test suite, cargo xtask, cargo clippy, cargo fmt

---

### Task 1: Close Canonical API and Structured-Pending Gaps

**Files:**
- Modify: `crates/julie-extractors/src/lib.rs`
- Modify: `crates/julie-extractors/src/factory.rs`
- Modify: `crates/julie-extractors/src/javascript/relationships.rs`
- Modify: `crates/julie-extractors/src/typescript/relationships.rs`
- Modify: `crates/julie-extractors/src/csharp/relationships.rs`
- Modify: `crates/julie-extractors/src/csharp/di_relationships.rs`
- Modify: `crates/julie-extractors/src/csharp/member_type_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/javascript/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/typescript/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/csharp/di_registration_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/csharp/field_property_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/csharp/cross_file_relationships.rs`

**What to build:** Remove the extra public extraction entrypoint that still lets callers bypass canonical parsing and JSONL handling, then migrate the remaining touched JS/TS/C# paths that still emit name-only pending edges. Keep compatibility glue internal where needed, but stop exposing it as the supported public surface.

**Approach:** Preserve `factory.rs` as an internal registry-compat helper for tests if needed, but remove the public re-export and route surfaced behavior through `extract_canonical` / `ExtractorManager`. For JS/TS inheritance and the remaining C# helper paths, emit `StructuredPendingRelationship` first and only degrade to legacy `PendingRelationship` at the compatibility edge.

**Acceptance criteria:**
- [ ] `lib.rs` no longer exposes a second public extraction path that bypasses canonical parsing.
- [ ] JS and TS inheritance paths no longer create bare `PendingRelationship` records as their primary unresolved output.
- [ ] C# constructor, DI, and member-type unresolved paths emit structured pending targets.
- [ ] Regression tests assert structured unresolved data for the touched paths.

### Task 2: Tighten Shared Semantic Policy

**Files:**
- Modify: `crates/julie-extractors/src/base/extractor.rs`
- Modify: `crates/julie-extractors/src/base/types.rs`
- Modify: `crates/julie-extractors/src/tests/base.rs`
- Create: `crates/julie-extractors/src/tests/doc_comments.rs`
- Create: `crates/julie-extractors/src/tests/identifier_semantics.rs`
- Modify: `crates/julie-extractors/src/tests/mod.rs`

**What to build:** Make doc-comment policy explicit and narrow, and remove or deliberately implement the dead `IdentifierKind::Import` contract.

**Approach:** Prefer language-aware doc-comment rules over the current broad comment-prefix whitelist. Remove `IdentifierKind::Import` unless the extractor surface already emits it coherently across representative languages, because dead public enum variants are fake support.

**Acceptance criteria:**
- [ ] Generic comments are no longer promoted to doc comments unless the language policy says they are docs.
- [ ] Doc-comment behavior is pinned by a dedicated invariant test module.
- [ ] `IdentifierKind::Import` is either removed cleanly or backed by real extractor/test coverage.
- [ ] Existing base tests stop locking in the old broad comment behavior.

### Task 3: Build Missing Invariant Suites and Replace Soft JSONL Coverage

**Files:**
- Create: `crates/julie-extractors/src/tests/type_invariants.rs`
- Create: `crates/julie-extractors/src/tests/path_invariants.rs`
- Create: `crates/julie-extractors/src/tests/jsonl_invariants.rs`
- Create: `crates/julie-extractors/src/tests/review_regressions.rs`
- Modify: `crates/julie-extractors/src/tests/go/types.rs`
- Modify: `crates/julie-extractors/src/tests/typescript/relative_paths.rs`
- Modify: `crates/julie-extractors/src/tests/json/mod.rs`
- Modify: `crates/julie-extractors/src/tests/python/relationships.rs`
- Modify: `crates/julie-extractors/src/tests/mod.rs`

**What to build:** Land the missing Task 10 and Task 11 coverage so extractor guarantees are pinned by invariant tests instead of smoke checks and local JSONL helpers.

**Approach:** Move JSONL verification onto the production path in `pipeline.rs`, rewrite soft assertions to exact value assertions in the touched files, and add cross-cutting invariant modules for path normalization, type mapping, and review regressions.

**Acceptance criteria:**
- [ ] `type_invariants.rs`, `path_invariants.rs`, `jsonl_invariants.rs`, and `review_regressions.rs` exist and are wired into `tests/mod.rs`.
- [ ] The touched path/type/JSONL/Python tests assert exact values rather than only presence or non-emptiness.
- [ ] JSONL tests no longer duplicate production extraction logic in a local helper.
- [ ] The new invariant suites pass through the canonical production path.

### Task 4: Finish Docs, Review Artifact, and Final Verification

**Files:**
- Create: `crates/julie-extractors/README.md`
- Modify: `crates/julie-extractors/src/lib.rs`
- Create: `docs/plans/2026-04-14-treesitter-world-class-review.md`

**What to build:** Document the supported extractor surface and record the final verification and review state required for world-class sign-off.

**Approach:** Describe the canonical API, compatibility boundaries, path/ID semantics, JSONL behavior, and unresolved-relationship guarantees in the crate README. Then write the review artifact with commands run, findings disposition, downgrade records, and the final go/no-go judgment.

**Acceptance criteria:**
- [ ] `crates/julie-extractors/README.md` explains the supported public surface and semantic guarantees.
- [ ] `docs/plans/2026-04-14-treesitter-world-class-review.md` records verification evidence, review findings, and downgrade records.
- [ ] `cargo fmt --check`, `cargo clippy`, and `cargo xtask test full` pass and are captured in the review artifact.
- [ ] No Important gaps from the remaining hardening review are left open.
