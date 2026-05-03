# Projection Invariants Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Enforce SQLite as the canonical source of truth and Tantivy as a repairable projection with fail-closed search readiness.

**Architecture:** Canonical indexing commits happen only through `SymbolDatabase`; Tantivy reads from committed SQLite state and records projection status in `projection_states`. `src/search/projection.rs` owns projection state transitions, rebuild decisions, and readiness gate updates; `src/tools/workspace/indexing/pipeline.rs` only sequences extraction, canonical persistence, and projection calls.

**Tech Stack:** Rust, SQLite via `rusqlite`, Tantivy, tokio blocking tasks, Julie health/readiness types, cargo nextest, xtask test tiers.

---

## File Structure

- Modify: `src/tools/workspace/indexing/pipeline.rs`
  - Keep `persist_batch` as the only canonical write path for extracted files.
  - Shrink `project_batch` so it delegates projection work to `SearchProjection` instead of duplicating projection-state writes and Tantivy document application.
- Modify: `src/search/projection.rs`
  - Keep `SearchProjection::ensure_current_with_gate`, `SearchProjection::project_documents`, and `projection_served_revision` as the projection contract.
  - If the implementation pushes this file further past the 500-line target, split helpers into `src/search/projection/apply.rs` and `src/search/projection/state.rs` behind the same public API.
- Modify: `src/database/projections.rs`
  - Preserve `ProjectionStatus`, `ProjectionState`, `upsert_projection_state`, and `get_projection_state` as the SQLite-backed ledger.
  - Add only small query helpers if the projection contract needs a single authoritative "current, stale, missing" check.
- Modify: `src/tools/workspace/indexing/index.rs`
  - Keep `backfill_tantivy_if_needed` as the startup and stale-index repair hook.
  - It should call `SearchProjection::ensure_current_with_gate` and treat failure as not ready, not as a quiet success.
- Modify: `src/health/projection.rs` and `src/health/evaluation.rs`
  - Health should report canonical/projection drift and readiness should fail closed when Tantivy is missing, stale, building, or errored.
- Test: `src/tests/integration/projection_repair.rs`
  - Add regression tests for fail-closed readiness and SQLite-driven rebuilds.
- Test: targeted search tests under `src/tests/tools/search/` or health tests in `src/tests/integration/system_health.rs` when externally visible search readiness output changes.

## Implementation Tasks

### Task 1: Lock The Canonical Write Contract

**Files:**
- Modify: `src/tools/workspace/indexing/pipeline.rs:419`
- Test: `src/tests/integration/projection_repair.rs`

Write a failing regression test that indexes a batch, simulates projection failure, and asserts SQLite still has the new canonical revision while search readiness is false. The invariant is boring but important: `persist_batch` may commit canonical data without Tantivy, but callers must not advertise search as ready for that revision.

Implementation notes:
- Leave `persist_batch` responsible for `incremental_update_atomic`, `bulk_store_fresh_atomic`, repair rows, and `get_current_canonical_revision`.
- Do not let projection code write canonical symbols, files, relationships, identifiers, or types.
- Make the returned `canonical_revision` the only input that projection uses as its target revision.

### Task 2: Centralize Tantivy Projection Logic

**Files:**
- Modify: `src/search/projection.rs:39`
- Modify: `src/tools/workspace/indexing/pipeline.rs:545`
- Test: `src/tests/integration/projection_repair.rs`

Move the logic for marking `Building`, applying/removing Tantivy documents, marking `Ready` or `Stale`, and updating `search_ready` behind `SearchProjection`. `project_batch` should prepare `SymbolDocument`, `FileDocument`, and `files_to_clean`, then call the projection API.

Acceptance criteria:
- `project_batch` no longer directly calls `upsert_projection_state` for normal projection state transitions.
- `project_batch` no longer calls `apply_documents_with_context` directly.
- `SearchProjection::project_documents` is the normal incremental projection path after canonical persistence.
- `SearchProjection::ensure_current_with_gate` remains the full backfill/repair path used by `backfill_tantivy_if_needed`.

### Task 3: Make Readiness Fail Closed

**Files:**
- Modify: `src/search/projection.rs:51`
- Modify: `src/tools/workspace/indexing/index.rs:244`
- Modify: `src/health/projection.rs:8`
- Modify: `src/health/evaluation.rs:26`
- Test: `src/tests/integration/projection_repair.rs`

Search readiness must be false unless Tantivy is known to serve the current SQLite canonical revision. Missing projection state, stale state, building state, projection error detail, missing Tantivy index, or failed rebuild all keep readiness false.

Acceptance criteria:
- `ensure_current_with_gate` sets `search_ready` true only after the served projection revision matches the canonical revision.
- Failed backfill in `backfill_tantivy_if_needed` returns an error or leaves readiness false with projection state marked stale. Silent `Ok(())` on a missing required projection is not acceptable once a canonical database exists.
- Health projection output distinguishes "canonical database exists but projection unavailable" from "workspace has no indexed symbols yet".

### Task 4: Remove Duplicate Projection Helpers

**Files:**
- Modify: `src/search/projection.rs:305`
- Modify: `src/tools/workspace/indexing/pipeline.rs:545`
- Test: `src/tests/tools/search/annotation_search_tests.rs`

Deduplicate document application so annotation context, owner-name context, clean-file handling, and Tantivy commit behavior have one implementation. If tests need raw uncommitted projection, keep `apply_uncommitted_documents_from_symbols` as a test/support path and name it honestly.

Acceptance criteria:
- The production indexing pipeline and repair path share the same context-loading behavior.
- Annotation search keeps passing because `load_symbol_contexts_from_database` remains the source for committed projection context.
- Direct in-memory projection helpers are not used to bypass SQLite in production.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `RAZORBACK.md`, and `docs/TESTING_GUIDE.md`.

**Worker red/green scope:** Each worker writes the failing test first and runs only the exact test it owns:
`cargo nextest run --lib <exact_test_name> 2>&1 | tail -10`

**Worker ceiling:** Workers may run at most their exact test filter, twice per fix cycle: once RED, once GREEN. Workers must not run `cargo xtask test changed`, `cargo xtask test dev`, dogfood, system, reliability, or broad `cargo nextest run --lib`.

**Worker gate invariant:** The worker report must state which invariant passed, for example "Tantivy stale state leaves `search_ready=false` while SQLite canonical revision remains advanced."

**Lead affected-change scope:** After a coherent batch, the lead runs:
`cargo xtask test changed`

**Branch gate:** Before handoff, the lead runs:
`cargo xtask test dev`

**Specialist gates:** Because this touches projection/search behavior, the lead also runs:
`cargo xtask test dogfood`

**Escalation triggers:** Add `cargo xtask test system` if startup repair, workspace init, or `backfill_tantivy_if_needed` behavior changes. Add `cargo xtask test reliability` if daemon lifecycle, watcher lifecycle, restart, or concurrent session behavior changes.

**Assigned verification failure:** Workers stop and report when assigned verification fails unless this plan is explicitly updated to change that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp. Reuse an existing passing ledger entry for the same HEAD and scope instead of rerunning an expensive gate.

## Model Routing

**Project source of truth:** `RAZORBACK.md`. Do not copy the global model table into this plan. If a local sentence conflicts with `RAZORBACK.md`, `RAZORBACK.md` wins.

**Plan-specific overrides:** Projection state, `search_ready`, canonical revision handling, health readiness, dogfood result interpretation, and indexing pipeline semantics are shared-invariant work. Use Codex `gpt-5.3-codex high` for bounded projection implementation and `gpt-5.3-codex xhigh` when debugging projection repair, dogfood drift, or readiness failures.

**Worker eligibility:** Use implementation-tier workers only for narrow tests or local edits with non-overlapping files. Use coupled implementation or lead-owned work for `SearchProjection`, health readiness, and indexing pipeline changes because they share hidden invariants.

**Mechanical exclusion:** Mechanical workers cannot own failing tests, replay evidence, metrics, or acceptance gates.

**Unsupported harness behavior:** If the harness cannot choose models per agent, use `inherit`, note it in the worker report, and continue.

## Task Decomposition

- Worker A: projection-state regression tests in `src/tests/integration/projection_repair.rs`. Owns exact-test RED/GREEN only.
- Worker B: `SearchProjection` API consolidation in `src/search/projection.rs`. Coupled implementation tier because projection state and readiness gates are easy to get subtly wrong.
- Worker C: pipeline delegation cleanup in `src/tools/workspace/indexing/pipeline.rs`. Must coordinate with Worker B on the new API shape, so assign after the API test is reviewed.
- Worker D: health/readiness output in `src/health/projection.rs` and `src/health/evaluation.rs`. Owns a narrow health test if output semantics change.
- Lead: integration review, `cargo xtask test changed`, `cargo xtask test dev`, and `cargo xtask test dogfood`.

## Risks

- `search_ready` can become true for a stale Tantivy index if any path treats projection repair as best-effort success. That is the main bug this plan is designed to kill.
- `project_batch` currently has detailed failure handling. Moving it into `SearchProjection` must preserve repair details in `IndexingBatchState::mark_repair_needed`.
- Full rebuild and incremental projection must load symbol context the same way or annotation and owner-name search will drift.
- A naive "Tantivy unavailable means no-op" behavior is wrong once SQLite has canonical data. It should be visible in projection state and health.
- Projection tests can accidentally assert only that a function returns `Ok`. Do not accept smoke-only tests here; assert canonical revision, projected revision, status, readiness, and at least one search/health observable.
