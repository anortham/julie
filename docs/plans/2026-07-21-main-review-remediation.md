# Main Review Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Correct the production indexing, search-projection, web-navigation, concurrency, and test-control-plane defects found while reviewing `origin/main..main` at `af929c55`.

**Architecture:** Canonical indexing paths must reconcile and revision-stamp derived web edges before reporting success. Tantivy rebuilds must publish one committed generation while ordinary readers retain the prior generation, and every projection path must use the same database enrichment. Test-runner timing must prebuild the targets it labels warm.

**Tech Stack:** Rust, SQLite/rusqlite, Tantivy, cargo-nextest, xtask TOML manifest.

**Architecture Quality:** High risk. Keep fixes behind existing caller-facing indexing, search, impact, and xtask interfaces; do not add speculative adapters or language-specific path rules.

## Global Constraints

- Follow RED -> GREEN -> REFACTOR for every behavior change.
- Preserve workspace file-level isolation and mutation-gate ownership.
- Resolve ambiguous web matches conservatively; never invent a cross-service or cross-schema link.
- Chunk SQLite `IN` queries below the bind-variable limit.
- Keep default navigation/search output byte-compatible when web mode is not requested.
- Workers run only exact assigned tests; the lead owns changed/dev/full gates.
- Do not push, publish, release, or overwrite unrelated work.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `docs/TESTING_GUIDE.md`, and `xtask/test_tiers.toml`.

**Worker red/green scope:** Exact named `cargo nextest run` tests for each changed behavior, with at most the RED and GREEN runs per fix.

**Worker ceiling:** Exact named tests only; no xtask tiers.

**Worker gate invariant:** Each assigned regression test proves the caller-visible defect no longer reproduces.

**Lead affected-change scope:** `cargo check`, `cargo xtask test changed`, and focused bucket commands selected from Miller impact evidence.

**Branch gate:** `cargo xtask test dev`; add `cargo xtask test system` for startup/indexing and `cargo xtask test dogfood` for search projection.

**Replay/metric evidence:** Fast-tier warm timing uses three complete current-membership runs after target-aware prebuild; p50/p95 are evidence, not pass/fail beyond the declared 60-second budget.

**Escalation triggers:** Rebuild synchronization or projection failures require the system gate; search token changes require dogfood.

**Assigned verification failure:** Workers diagnose their exact test and report if it does not converge; the lead owns broader failures.

**Verification ledger:** Record command, invariant, current commit/tree state, result, and UTC timestamp in the final review report.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Reconcile web edges in every canonical index path | Batch A | `src/tools/workspace/indexing/pipeline.rs`, `src/tools/workspace/indexing/incremental.rs`, relevant indexing tests | No | None - safe parallel batch. |
| Task 2: Make watcher projection readiness follow commit success | Batch A | `crates/julie-runtime/src/watcher/handlers.rs`, `crates/julie-runtime/src/watcher/runtime.rs`, watcher tests | No | None - safe parallel batch. |
| Task 3: Repair xtask scale output and warm accounting | Batch A | `xtask/src/changed.rs`, `xtask/src/runner.rs`, xtask tests and control-plane docs | No | None - safe parallel batch. |
| Task 4: Unify structural-fact projection and guard rebuild publication | None - serial after Batch A | `crates/julie-index/src/search/**`, projection/search tests | Yes | Search projection and rebuild synchronization share the same index internals. |
| Task 5: Harden web derivation, batching, and impact semantics | None - serial after Task 4 | `crates/julie-core/src/database/**`, `crates/julie-pipeline/src/indexing_core/web_edges.rs`, `crates/julie-tools/src/impact/**`, focused tests | Yes | Uses the projection and edge contracts stabilized by Task 4. |
| Task 6: Format, document, and verify the integrated review fixes | None - serial | Changed files, verification evidence, agent-doc sync | Yes | Must operate on the final integrated diff. |

**Commit mode:** `parallel-lead-commit` for Batch A; no worker commits. The lead will leave the reviewed fixes uncommitted unless the user requests a commit.

### Task 1: Reconcile web edges in every canonical index path

**Files:**
- Modify: `src/tools/workspace/indexing/pipeline.rs`
- Modify: `src/tools/workspace/indexing/incremental.rs`
- Test: `src/tests/integration/indexing_pipeline.rs`
- Test: `src/tests/core/incremental_update_atomic/web_edges.rs`

**Interfaces:**
- Consumes: canonical structural-fact persistence and `rebuild_web_edges(&mut SymbolDatabase)`.
- Produces: successful full, incremental, and orphan-cleanup indexing always leaves `web_edges` consistent.

**What to build:** Add end-to-end failing tests through real indexing entry points, then reconcile web edges after canonical persistence and orphan deletion. Revision-stamp successful rebuilds and repair revision lag during startup handoff. Propagate rebuild failures rather than reporting indexing success with stale derived data.

**Acceptance criteria:**
- [x] Fresh/full indexing produces HTTP and SQL web edges without a watcher event.
- [x] Orphan cleanup degrades removed targets to conservative external edges.
- [x] Revision lag is repaired from canonical SQLite state during startup handoff.
- [x] Exact worker tests pass.

### Task 2: Make watcher projection readiness follow commit success

**Files:**
- Modify: `crates/julie-runtime/src/watcher/handlers.rs`
- Modify: `crates/julie-runtime/src/watcher/runtime.rs`
- Test: `crates/julie-runtime/src/tests/watcher_handlers/repair_projection.rs`

**Interfaces:**
- Consumes: batched uncommitted Tantivy writes and the runtime commit boundary.
- Produces: projection revision becomes Ready only after a successful commit; failures persist repairable stale state.

**What to build:** Reproduce the premature Ready transition, move readiness publication to the successful commit boundary, and mark stale/dirty repair state when commit fails.

**Acceptance criteria:**
- [x] Uncommitted writes never advance durable projected revision.
- [x] Commit failure leaves projection repairable rather than falsely current.
- [x] Exact worker tests pass.

### Task 3: Repair xtask scale output and warm accounting

**Files:**
- Modify: `xtask/src/changed.rs`
- Modify: `xtask/src/runner.rs`
- Test: `xtask/tests/changed_tests.rs`
- Test: `xtask/tests/runner_coverage_tests.rs`
- Test: `xtask/tests/runner_tests.rs`
- Modify: `docs/TESTING_GUIDE.md`
- Modify: `AGENTS.md`
- Modify: `CLAUDE.md`
- Modify: `docs/plans/2026-07-21-julie-test-control-plane-verification.md`

**Interfaces:**
- Consumes: selected manifest bucket commands.
- Produces: scaled output without stale advice and target-aware prebuild elapsed separated from bucket execution elapsed.

**What to build:** Add failing output and executor-order tests, derive prebuild commands for selected cargo test/nextest targets, and update timing documentation. Re-measure the current fast membership after integration.

**Acceptance criteria:**
- [x] `changed --scale` does not advise rerunning `--scale`.
- [x] Every selected Rust test target is prebuilt before its bucket timer starts.
- [x] Current fast membership has fresh three-run p50/p95 evidence.
- [x] Exact xtask tests pass.

### Task 4: Unify structural-fact projection and guard rebuild publication

**Files:**
- Modify: `crates/julie-index/src/search/index.rs`
- Modify: `crates/julie-index/src/search/projection.rs`
- Modify: `crates/julie-index/src/search/projection/apply.rs`
- Modify: `crates/julie-index/src/search/projection/facts_text.rs`
- Test: projection and search concurrency tests under `crates/julie-index/src/tests/search/` and `src/tests/tools/search/`

**Interfaces:**
- Consumes: `Arc<SearchIndex>` and database-backed symbol projection.
- Produces: full, repair, and watcher projections share structural-fact enrichment; readers cannot observe the clear/repopulate gap.

**What to build:** Add failing full/repair search tests, centralize database enrichment, and stage deletion plus replacement in one Tantivy writer generation. Commit once on success and roll back on failure so ordinary and cross-process readers retain the prior committed generation until the replacement is complete.

**Acceptance criteria:**
- [x] Full and repaired indexes preserve route/table searchability.
- [x] A reader cannot observe an empty generation during rebuild.
- [x] Failed rebuilds roll back without leaking staged deletion into a later commit.
- [x] Test-only writer access is gated to test-support builds.
- [x] Exact worker tests pass.

### Task 5: Harden web derivation, batching, and impact semantics

**Files:**
- Modify: `crates/julie-core/src/database/structural_facts.rs`
- Modify: `crates/julie-core/src/database/web_edges.rs`
- Modify: `crates/julie-pipeline/src/indexing_core/web_edges.rs`
- Modify: `crates/julie-index/src/search/projection/facts_text.rs`
- Modify: `crates/julie-tools/src/impact/mod.rs`
- Modify: `crates/julie-tools/src/impact/walk.rs`
- Test: focused database, pipeline, search, and impact tests.

**Interfaces:**
- Consumes: pinned extractor metadata and existing impact graph walk.
- Produces: conservative unique web matches, bind-safe batch queries, searchable scalar/array endpoint metadata, bounded merged text, and web callers included in impact ranking/test inference.

**What to build:** Test and fix each review finding independently: ambiguity, large batches, metadata keys, byte cap, and impact integration. Keep all behavior language-agnostic and use real extractor metadata shapes.

**Acceptance criteria:**
- [x] Equal top matches remain ambiguous instead of linking arbitrarily.
- [x] Large symbol batches stay below SQLite bind limits.
- [x] `target_path`, `target_table`, and `source_tables` are searchable.
- [x] Merged relationship text never exceeds its cap.
- [x] Web callers participate in ranked impact and likely-test output without ranking seed symbols as their own callers.
- [x] Exact worker tests pass.

### Task 6: Format, document, and verify the integrated review fixes

**Files:**
- Modify: only files changed in `origin/main..main` or by Tasks 1-5.

**Interfaces:**
- Consumes: integrated reviewed diff.
- Produces: warning-free changed code, synchronized agent docs, and fresh verification evidence.

**What to build:** Format only review-range/fix files, remove mechanical warnings and diff-check failures, update plan status/evidence accurately, and run the lead-owned gates.

**Acceptance criteria:**
- [x] Review-range and remediation Rust files pass rustfmt; the integrated diff passes `git diff --check`.
- [x] `cargo check` is warning-free for changed code.
- [x] Agent docs remain synchronized.
- [x] Required affected-change, dev, system, and dogfood gates pass.
