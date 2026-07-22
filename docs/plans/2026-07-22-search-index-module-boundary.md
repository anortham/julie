# Search Index Module Boundary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Complete Phase 2D by decomposing `crates/julie-index/src/search/index.rs` into focused type, compatibility, lifecycle, mutation, and query modules without changing the `SearchIndex` API, persisted index compatibility, search behavior, serialized results, or concurrency semantics.

**Architecture:** Keep `search::index` as the stable facade. Keep `SearchIndex`, its eight fields, `SearchIndexHandle`, and the existing public reexports at their current paths. Move inherent `impl SearchIndex` method groups and supporting types/helpers into private children under `search/index/`. Query execution gets a private subtree because its existing implementation and helpers cannot fit one 500-line file. Child seams use the narrowest effective visibility and callers outside `search::index` learn no new concepts.

**Tech Stack:** Rust 1.97.0, Tantivy, fs2 advisory locks, serde, tempfile, Cargo nextest, Cargo xtask.

**Architecture Quality:** Deepen the existing `SearchIndex` boundary without changing its state layout or introducing coordinator, context, port, or adapter abstractions. Architecture risk is high because the split is mechanically broad and open/rebuild locking, writer ownership, query construction, ranking, and result assembly are load-bearing.

## Global Constraints

- Preserve every `SearchIndex` field and field type in `index.rs`; do not widen field visibility to make the split compile.
- Preserve every public and `pub(crate)` path, signature, generic bound, return type, serde shape, default, and documentation contract currently exposed through `search::index` and `search/mod.rs`.
- Preserve `SearchIndexHandle = Arc<SearchIndex>` at the same path.
- Preserve Tantivy schema construction and compatibility signatures in the existing `search/schema.rs`; do not duplicate or relocate that module.
- Preserve compatibility marker filename/version, marker JSON, tokenizer registration, schema validation, rebuild locking, temporary-directory placement, atomic rename order, cleanup, and every open disposition/repair reason.
- Preserve writer lock ownership, lazy writer creation, commit/reload ordering, rollback/release/shutdown behavior, and all test-only rebuild/writer seams.
- Preserve query tokenization, boosts, AND-to-OR relaxation, filters, annotation handling, file matching/promotion, compaction, reranking, tie-breaking, hit ordering, truncation, and stored-field extraction.
- Keep `crates/julie-index/src/search/mod.rs` and all existing behavior tests unchanged.
- Keep all new modules private; reexport existing public items from `index.rs` so caller imports do not change.
- Use explicit imports and only the minimum `pub(super)` / `pub(in ...)` visibility required for parent-child or sibling calls.
- Keep every Phase 2D production implementation file at or below 500 lines.
- Do not combine the split with schema, scoring, error-text, logging, performance-policy, API, or test-behavior changes.
- Do not push, merge, publish, or release without separate explicit approval.

---

## Architecture Quality

**Affected modules:** `crates/julie-index/src/search/index.rs`, its new private children, and one new structural test.

**Caller-facing interface:** Existing projection, watcher, workspace-routing, tool, and test callers continue using `crate::search::{SearchIndex, SearchDocument, SearchFilter, ...}` or the current `search::index::*` paths. `crates/julie-index/src/search/mod.rs` retains its existing reexports unchanged.

**Depth/locality check:** The parent owns shared state and the stable namespace. `types.rs` owns documents, filters, result values, and result-only helpers. `compatibility.rs` owns on-disk compatibility policy and locked recreation. `lifecycle.rs` owns create/open/open-or-create construction. `mutation.rs` owns writer-backed state changes and shutdown. The query subtree owns only read-side query construction, execution, and ranking.

**Test surface:** Existing 380 `julie-index` tests remain the behavior oracle. The only new test is structural and enumerates every Phase 2D production file so the 500-line boundary cannot regress silently.

**Seams/adapters:** Dependencies are in-process or local-substitutable through existing Tantivy, filesystem, lock, temp-directory, and subprocess fixtures. No remote boundary exists, so a new port or adapter would add ceremony without substitutability value.

**Rejected shortcuts:** Public submodules would migrate a 210-reference caller surface. New coordinator structs would change `SearchIndex` construction/state and create shallow pass-through interfaces. Free functions plus a shared context would expose or duplicate the eight-field state. A single query child would remain well above 500 lines. Moving `schema.rs` would blur its already-correct responsibility.

**Architecture risk:** High. Public type paths remain stable, but subtle statement or scope movement can change cross-process rebuild safety, writer lifecycle, query semantics, scores, or result order.

### Interface Lane Decision

- **Selected - stable facade plus private child inherent implementations:** preserves state, methods, paths, and caller obligations while localizing responsibilities.
- **Rejected - public submodules and caller migration:** creates avoidable API churn across projection, watcher, routing, tools, and tests.
- **Rejected - nested coordinator objects:** changes construction and ownership merely to avoid private module seams.
- **Rejected - context plus free functions:** widens state access and replaces a cohesive facade with high-arity plumbing.

### Dependency Classification

- **In-process:** Tantivy schema/tokenizers/query/scoring, reranking, serde value types, and `LanguageConfigs`.
- **Local-substitutable:** Tantivy directories, compatibility marker files, fs2 advisory locks, temporary rebuild directories, and subprocess fixtures.
- **Remote/external service:** none. No new resilience or adapter policy is required.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `docs/TESTING_GUIDE.md`, `xtask/test_tiers.toml`, `xtask/src/changed/mapping.rs`, and `docs/plans/2026-07-21-julie-improvement-roadmap-design.md`.

**Baseline evidence:** `cargo nextest run -p julie-index` passes 380/380 with one skipped test at `e3cf16d70cda1852d684b401899615504312c078`.

**Worker red/green scope:** `cargo nextest run -p julie-index --lib tests::search::index_boundary_test::search_index_implementation_files_stay_within_limit`.

**Worker ceiling:** Run only the exact structural test once RED and once GREEN during the TDD loop. The lead owns existing search tests and broader gates.

**Worker gate invariant:** Initial RED must report `src/search/index.rs` at 2,326 lines against the 500-line limit. GREEN proves the facade and every enumerated private production module exist and remain at or below 500 lines.

**Lead affected-change scope:** Run `cargo xtask test changed`. Production paths under `crates/julie-index/src/search/` must map to and pass `core-index`, all 13 `tools-search-*` buckets, and `search-quality`.

**Lead focused scope:** Run `cargo check -p julie-index` and `cargo nextest run -p julie-index` at the exact implementation commit.

**Branch gate:** Run `cargo xtask test dev` once at final current HEAD before handoff.

**Specialist gates:** Run `cargo xtask test dogfood` for product-linked search quality and `cargo xtask test full` for the roadmap-required broad pre-merge pass at the exact implementation commit.

**Formatting and structural evidence:** Run targeted `rustfmt --edition 2024 --check` on all Phase 2D Rust files, `git diff --check`, a line-count assertion through the structural test, and a diff check proving `crates/julie-index/src/search/mod.rs` plus existing behavior tests are unchanged.

**Replay/metric evidence:** Test outcomes, exact file limits, unchanged public paths/signatures, compatibility/rebuild order, writer lifecycle, query construction, scoring, and result ordering are hard gates. Durations are report-only.

**Escalation triggers:** Any required change to `search/mod.rs`, `search/schema.rs`, existing tests, public signatures, field shapes, serialized values, compatibility artifacts, lock/write scopes, query semantics, score/order behavior, or error/log strings blocks completion and requires plan reassessment.

**Assigned verification failure:** Workers stop and report when assigned verification fails unless this plan explicitly assigns that gate for update.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp in `docs/plans/2026-07-22-search-index-module-boundary-verification.md`. Evidence is reusable only at the exact recorded HEAD and scope.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Split `SearchIndex` behind its stable facade | None - serial | Create the private `search/index/` modules, structural test, and verification ledger; modify only `search/index.rs` and the search test registry | Not applicable - single task | All new modules share private types, inherent implementations, imports, and visibility; compiling partial independent splits would create conflicting intermediate states. |

### Task 1: Split `SearchIndex` behind its stable facade

**Files:**

- Create: `crates/julie-index/src/search/index/types.rs`
- Create: `crates/julie-index/src/search/index/compatibility.rs`
- Create: `crates/julie-index/src/search/index/lifecycle.rs`
- Create: `crates/julie-index/src/search/index/mutation.rs`
- Create: `crates/julie-index/src/search/index/query/mod.rs`
- Create: `crates/julie-index/src/search/index/query/execution.rs`
- Create: `crates/julie-index/src/search/index/query/terms.rs`
- Create: `crates/julie-index/src/search/index/query/files.rs`
- Create: `crates/julie-index/src/tests/search/index_boundary_test.rs`
- Create: `docs/plans/2026-07-22-search-index-module-boundary-verification.md`
- Modify: `crates/julie-index/src/search/index.rs:1-2326`
- Modify: `crates/julie-index/src/tests/search/mod.rs`

**Interfaces:**

- Consumes: existing `search::schema::{create_schema, compatibility_signature, SchemaFields}`, tokenizer registration, Tantivy index/reader/writer/query APIs, `LanguageConfigs`, reranking helpers, filesystem locking, serde, and temp directories.
- Produces: the identical `search::index` namespace, `SearchIndex` facade, open outcomes, document/filter/result types, public helper functions/constants, query behavior, and persisted index contract.

**Contract inputs:** All 62 current `SearchIndex` methods, all current free helper visibilities, `SearchIndexHandle`, `SearchIndexOpenDisposition`, `SearchIndexOpenOutcome`, `SearchDocument`, `SearchFilter`, result types, `UnifiedHit`, `FileMatchKind`, and `SEARCH_COMPAT_MARKER_FILE` retain their exact caller-facing contracts.

**File ownership:** Create the files listed above; modify only `crates/julie-index/src/search/index.rs` and `crates/julie-index/src/tests/search/mod.rs`. Existing tests, `search/mod.rs`, `search/schema.rs`, projection code, watcher code, routing code, and tool code are forbidden edits.

**Serialization required:** Not applicable - single task.

**Dependency reason:** The structural RED/GREEN test spans the coherent boundary, and all child inherent implementations depend on the parent-owned private fields.

**Step 1: Write the failing structural test**

Add `mod index_boundary_test;` to `crates/julie-index/src/tests/search/mod.rs`.

Create `crates/julie-index/src/tests/search/index_boundary_test.rs`:

```rust
use std::{fs, path::PathBuf};

#[test]
fn search_index_implementation_files_stay_within_limit() {
    for relative_path in [
        "src/search/index.rs",
        "src/search/index/types.rs",
        "src/search/index/compatibility.rs",
        "src/search/index/lifecycle.rs",
        "src/search/index/mutation.rs",
        "src/search/index/query/mod.rs",
        "src/search/index/query/execution.rs",
        "src/search/index/query/terms.rs",
        "src/search/index/query/files.rs",
    ] {
        assert_line_limit(relative_path, 500);
    }
}

fn assert_line_limit(relative_path: &str, limit: usize) {
    let contents = fs::read_to_string(crate_file(relative_path))
        .unwrap_or_else(|error| panic!("failed to read {relative_path}: {error}"));
    let line_count = contents.lines().count();

    assert!(
        line_count <= limit,
        "{relative_path} has {line_count} lines; limit is {limit}"
    );
}

fn crate_file(relative_path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative_path)
}
```

**Step 2: Run the test to verify RED**

Run:

```bash
cargo nextest run -p julie-index --lib tests::search::index_boundary_test::search_index_implementation_files_stay_within_limit 2>&1 | tail -20
```

Expected: FAIL because `src/search/index.rs` has 2,326 lines against the 500-line limit. The existing facade is enumerated first, so RED must not be a missing-file failure.

**Step 3: Establish the stable facade and type boundary**

Keep this shape in `index.rs`:

```rust
mod compatibility;
mod lifecycle;
mod mutation;
mod query;
mod types;

pub use compatibility::SEARCH_COMPAT_MARKER_FILE;
pub use lifecycle::{SearchIndexOpenDisposition, SearchIndexOpenOutcome};
pub use types::{
    ContentSearchResult, ContentSearchResults, FileMatchKind, FileSearchResult,
    FileSearchResults, SearchDocument, SearchFilter, SymbolSearchResult,
    SymbolSearchResults, UnifiedHit,
};

pub struct SearchIndex {
    // Retain all eight existing fields, types, cfg attributes, and docs here.
}

pub type SearchIndexHandle = Arc<SearchIndex>;
```

Move the document/filter/result declarations and their existing impls from the pre-`SearchIndex` section into `types.rs`. Move `truncate_utf8_bytes`, `is_test_symbol_result`, and `symbol_role_and_test_role` with them, then reexport each at its existing effective visibility from `index.rs`. Do not alter serde derives, field order/types, constructors, defaults, truncation behavior, or role classification.

Keep the private cfg(test) `RebuildPauseForTest` beside `SearchIndex` in `index.rs` because it is the concrete type of a parent-owned field. Moving its methods does not justify exporting or widening that field type.

**Step 4: Move compatibility and lifecycle without changing open behavior**

Move into `compatibility.rs`:

- `SearchCompatMarker` and its version/file constants.
- Expected/read/write marker helpers.
- Index/schema compatibility checks.
- `recreate_index_with_lock` with its stable sibling lock, fs2 lock scope, temporary rebuild directory, cleanup, and atomic rename sequence unchanged.

Move into `lifecycle.rs`:

- `SearchIndexOpenDisposition`, `SearchIndexOpenOutcome`, and their existing helpers.
- Public `create`, `open`, and `open_or_create` variants.
- Tokenizer-aware create/open/open-or-create methods.
- Tokenizer registration and `SearchIndex` construction.

Keep all open methods as inherent `impl SearchIndex` methods. Compatibility helpers called across children use the narrowest `pub(super)` visibility. Preserve every error mapping, repair reason, disposition, marker write point, and reader reload policy.

**Step 5: Move writer-backed mutation and shutdown**

Move into `mutation.rs`:

- `num_docs`.
- `add_search_doc`, `commit`, `release_writer`, `rollback_writer`, `clear`, and document removal methods.
- Lazy writer acquisition and writer-lock helpers.
- `shutdown` and `is_shutdown`.
- All existing cfg(test) rebuild pause/failure and writer-acquisition method seams; the field-only `RebuildPauseForTest` type stays private in `index.rs`.

Keep these as inherent methods over the parent-owned fields. Preserve `Mutex<Option<IndexWriter>>` ownership, lock scopes, writer memory settings, add/delete/commit/reload order, shutdown flag ordering, rollback/release semantics, and every cfg(test) signature.

**Step 6: Move query construction, execution, and ranking**

Use this private subtree:

- `query/mod.rs`: query constants, public search wrappers, annotation search, symbol search, content/file wrappers, and unified wrapper methods through the existing `search_unified_full` delegation point.
- `query/execution.rs`: the unchanged `search_unified_full` algorithm plus stored-document field extraction helpers.
- `query/terms.rs`: query tokenization, term filtering, annotation context terms, annotation query construction, and boosted-term insertion.
- `query/files.rs`: path normalization, basename/glob matching, match classification, file promotion, result compaction, and unified reranking helpers. Reexport `classify_file_match` as `pub(crate)` and `compact_alnum_lc`, `apply_reranker_to_content_results`, and `apply_symbol_title_boost_to_file_results` as `pub` from `index.rs`, exactly matching their current paths.

Keep public query methods as inherent `impl SearchIndex` methods and reexport existing public or `pub(crate)` free helpers from `index.rs`. Preserve clause order, boosts, relaxed fallback, limits, filters, deduplication, match classification, score mutations, promotion/rerank order, tie-breakers, excerpts, truncation, and final serialized results. Do not extract a new query coordinator or replace existing value flow with a context object.

**Step 7: Compile and run exact GREEN verification**

Run:

```bash
cargo check -p julie-index
```

Expected: PASS with `search/mod.rs`, `search/schema.rs`, and all callers unchanged.

Run:

```bash
cargo nextest run -p julie-index --lib tests::search::index_boundary_test::search_index_implementation_files_stay_within_limit 2>&1 | tail -20
```

Expected: PASS 1/1 with every enumerated production file at or below 500 lines.

**Step 8: Apply commit mode**

- `serial-worker-commit`: after exact GREEN and lead inline review, checkpoint and commit the owned implementation files. Record the implementation SHA before exact-commit lead gates.

**Acceptance criteria:**

- [ ] Structural RED reports the 2,326-line existing facade; GREEN proves every Phase 2D production file is at or below 500 lines.
- [ ] `SearchIndex`, its eight fields, and `SearchIndexHandle` remain in `index.rs` with unchanged types and visibility.
- [ ] All 62 current methods and all public / `pub(crate)` types, constants, and helper paths retain their exact caller-facing contracts.
- [ ] `search/mod.rs`, `search/schema.rs`, every existing behavior test, and all external callers remain unchanged.
- [ ] Compatibility marker content, schema/tokenizer compatibility, rebuild lock/temp/rename sequencing, and open outcomes remain unchanged.
- [ ] Writer ownership, lock scopes, mutation order, commit/reload, rollback/release, shutdown, and test hooks remain unchanged.
- [ ] Query tokenization, boosts, filtering, relaxation, promotion, compaction, reranking, scores, excerpts, truncation, and hit ordering remain unchanged.
- [ ] `cargo check -p julie-index`, `cargo nextest run -p julie-index`, `cargo xtask test changed`, `cargo xtask test dogfood`, `cargo xtask test dev`, and `cargo xtask test full` pass at their required exact commits.
- [ ] Verification evidence is recorded in `docs/plans/2026-07-22-search-index-module-boundary-verification.md` with exact scope labels and SHAs.

## Final Lead Review and Handoff

1. Inspect the final diff for moved-body-only changes and forbidden-file edits.
2. Confirm `search/mod.rs`, `search/schema.rs`, existing tests, and external callers have no diff.
3. Confirm every old public or `pub(crate)` item still resolves at the same path and every `SearchIndex` method retains its signature.
4. Confirm all nine production files pass the structural line limit.
5. Run and ledger focused, changed, dogfood, dev, and full gates at the exact required commits.
6. Run `git diff --check`, targeted rustfmt verification, and a final worktree-state audit.
7. Checkpoint the completed Phase 2D result and update the active brief before requesting any merge, push, or release approval.
