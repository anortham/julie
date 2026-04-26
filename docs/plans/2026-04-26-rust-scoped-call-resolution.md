# Rust Scoped Call Resolution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Preserve Rust scoped call targets through extraction and indexing so `crate::module::function()` resolves to the intended project symbol, while external calls such as `std::collections::HashMap::new()` do not resolve to unrelated local functions named `new`.
**Architecture:** Rust extraction should emit `StructuredPendingRelationship` entries with `UnresolvedTarget.namespace_path`; indexing should carry those structured pendings through batch processing; the resolver should prefer namespace-consistent candidates and reject known external namespace roots when no project match exists.
**Tech Stack:** Rust, tree-sitter-rust, Julie extractor crate, SQLite-backed `SymbolDatabase`, Tantivy-backed workspace indexing, `cargo nextest`, `cargo xtask`.

---

## Background

The narrow item is stale as written. Rust scoped identifiers are already recognized in the extractor:

- `crates/julie-extractors/src/rust/identifiers.rs` extracts the terminal identifier from `scoped_identifier`.
- `crates/julie-extractors/src/rust/relationships.rs` creates call relationships for `scoped_identifier`.
- Existing extractor tests cover terminal-name extraction and basic scoped-call pending relationships.

The remaining defect is worse than missing extraction: scoped call resolution throws away namespace context. `std::collections::HashMap::new()` can be treated as a bare call to `new`, which can create a bogus relationship to a local `fn new()`. That makes the call graph confident in a wrong edge, a delightful little footgun.

## Execution Skills

- @razorback:test-driven-development
- @razorback:subagent-driven-development
- @razorback:verification-before-completion

## Work Items

### 1. Add extractor regressions for scoped target metadata

**Files:**

- `crates/julie-extractors/src/tests/rust/relationships.rs`
- `crates/julie-extractors/src/tests/rust/cross_file_relationships.rs`
- `crates/julie-extractors/src/rust/relationships.rs`

**Tests first:**

- Add a test for `std::collections::HashMap::new()` in a file that also defines local `fn new()`. Expected result: no resolved direct relationship to the local `new`; the structured pending target has terminal name `new` and namespace path `["std", "collections", "HashMap"]`.
- Add a test for `crate::search::hybrid::should_use_semantic_fallback()`. Expected result: structured pending target has terminal name `should_use_semantic_fallback` and namespace path `["crate", "search", "hybrid"]`.

**Implementation:**

- Add a helper in `crates/julie-extractors/src/rust/relationships.rs` that converts a Rust `scoped_identifier` node into an `UnresolvedTarget`.
- Use the full scoped text for display, the final segment for `terminal_name`, and preceding segments for `namespace_path`.
- Route scoped calls through structured pending output when namespace context exists.
- Avoid creating direct local call relationships for scoped targets unless a scoped match is proven. Bare local resolution remains valid for simple identifiers.

**Acceptance criteria:**

- The new tests fail before implementation.
- Structured pending targets preserve namespace path.
- External scoped calls no longer resolve to same-name local functions at extraction time.
- Existing simple call and member call behavior remains green.

### 2. Carry structured pending relationships through indexing

**Files:**

- `src/tools/workspace/indexing/processor.rs`
- `src/tools/workspace/indexing/pipeline.rs`

**Tests first:**

- Add or update a focused indexing test that proves `structured_pending_relationships` from extractor output reach the resolver input instead of being dropped.

**Implementation:**

- Import `StructuredPendingRelationship` where batch processing already imports `PendingRelationship`.
- Extend `process_file_with_parser` to return `Vec<StructuredPendingRelationship>` from extractor results.
- Extend `ExtractedBatch` with `all_structured_pending_relationships`.
- Extend `ExtractOutcome::WithParser` and batch accumulation to carry structured pendings alongside legacy pendings.
- Update `resolve_pending_relationships` to prefer structured pendings when present, with legacy fallback for extractors that still emit only `PendingRelationship`.

**Acceptance criteria:**

- Structured pending data is not discarded by `processor.rs`.
- Batch-level resolver input contains scoped target metadata.
- Legacy pending behavior remains available for languages that do not emit structured targets yet.

### 3. Make resolver namespace-aware

**Files:**

- `src/tools/workspace/indexing/resolver.rs`
- `src/tests/core/batch_resolver.rs`

**Tests first:**

- Add `test_resolve_structured_batch_prefers_crate_namespace_path_candidate`.
  - Build two candidate symbols with the same terminal name.
  - One candidate lives under a path matching `crate::search::hybrid`.
  - One candidate is a same-name decoy elsewhere.
  - Expected result: the namespaced candidate wins.
- Add `test_resolve_structured_batch_rejects_std_namespace_project_symbol`.
  - Candidate local symbol is named `new`.
  - Pending target is `std::collections::HashMap::new`.
  - Expected result: no relationship is produced.
- Add `test_resolve_batch_legacy_wrapper_preserves_existing_behavior`.
  - Legacy `PendingRelationship` still resolves by terminal name and existing scoring.

**Implementation:**

- Add `resolve_structured_batch(&[StructuredPendingRelationship], &SymbolDatabase)` or equivalent.
- Keep `resolve_batch(&[PendingRelationship], &SymbolDatabase)` as a compatibility wrapper that maps each legacy pending to a simple structured target.
- Continue batching candidate lookup by terminal name for database efficiency.
- Extend candidate scoring to account for `UnresolvedTarget.namespace_path`.
- Reject known external Rust roots such as `std`, `core`, and `alloc` when no project candidate explicitly matches that root.
- For `crate::...` targets, prefer candidates whose file path or parent symbol context matches the namespace path.
- Make namespace mismatch stronger than same-directory and same-language scoring so a wrong local match cannot win by proximity.

**Acceptance criteria:**

- Existing resolver tests remain green.
- New resolver tests fail before implementation and pass afterward.
- Namespace-aware scoring is scoped to structured targets; legacy resolver behavior stays intact.
- The resolver remains language-agnostic by default, with Rust-specific namespace roots guarded by target metadata, not broad path assumptions.

### 4. Add an indexed-workspace integration regression

**Files:**

- `src/tests/tools/call_path_tests.rs`

**Tests first:**

- Add a fixture-style integration test that indexes a small Rust workspace with:
  - `src/search/hybrid.rs` defining `should_use_semantic_fallback`.
  - `src/other.rs` defining a decoy `should_use_semantic_fallback`.
  - A caller using `crate::search::hybrid::should_use_semantic_fallback()`.
- Expected result: `call_path` or relationship inspection reaches the intended `search::hybrid` target, not the decoy.
- Add the `std::collections::HashMap::new()` false-positive shape if the existing helper pattern can express a negative call path cleanly.

**Implementation:**

- Reuse existing call path test helpers in `src/tests/tools/call_path_tests.rs`.
- Keep the fixture minimal and language-specific to Rust, because the defect comes from Rust scoped call syntax.

**Acceptance criteria:**

- The integration test proves the fix survives the extractor, indexing pipeline, resolver, and call graph query path.
- No false positive edge is created for external `std::...` calls.

## Verification

Run the narrow RED/GREEN tests during implementation:

```bash
cargo nextest run --lib test_external_scoped_call_does_not_resolve_to_local_bare_name 2>&1 | tail -10
cargo nextest run --lib test_crate_scoped_call_preserves_namespace_path_in_structured_pending 2>&1 | tail -10
cargo nextest run --lib test_resolve_structured_batch_prefers_crate_namespace_path_candidate 2>&1 | tail -10
cargo nextest run --lib test_resolve_structured_batch_rejects_std_namespace_project_symbol 2>&1 | tail -10
cargo nextest run --lib test_call_path_resolves_rust_crate_scoped_call_to_namespaced_target 2>&1 | tail -10
```

After the implementation batch:

```bash
cargo xtask test changed
cargo xtask test dev
```

## Risks

- Over-filtering scoped targets could drop valid local calls such as `Self::new` or `super::module::function`. Keep tests around those forms if existing behavior depends on them.
- Path matching for `crate::...` must handle Rust module layouts including `foo.rs` and `foo/mod.rs`.
- Structured resolver changes touch shared indexing behavior, so legacy pending fallback must stay boring and explicit.

## Done Criteria

- Scoped Rust calls preserve namespace metadata in extractor output.
- Indexing no longer drops `structured_pending_relationships`.
- Resolver uses namespace metadata to prefer the right project symbol and reject external namespace false positives.
- New unit and integration tests pass.
- `cargo xtask test changed` and `cargo xtask test dev` pass before handoff.
