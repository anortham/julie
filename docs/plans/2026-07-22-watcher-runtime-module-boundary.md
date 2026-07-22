# Watcher Runtime Module Boundary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Complete Phase 2C by decomposing `crates/julie-runtime/src/watcher/runtime.rs` into focused runtime-state, event-processing, repair, and Tantivy-projection modules without changing watcher behavior or concurrency semantics.

**Architecture:** Keep `watcher::runtime` as the private facade and keep `QueueRuntime`, all 17 fields, constants, constructors, mutation-gate acquisition, cycle ordering, and watcher-facing methods in the parent file. Move only private inherent-method groups into child modules under `watcher/runtime/`: queue processing and shutdown draining, persisted/overflow repair, and Tantivy projection commit/retry. The parent retains the existing effective visibility toward `watcher/mod.rs`; child-to-parent and sibling seams are limited to `pub(super)`.

**Tech Stack:** Rust 1.97.0, Tokio, Notify, SQLite, Tantivy, Cargo nextest, Cargo xtask.

**Architecture Quality:** Deepen the existing private `QueueRuntime` boundary without changing `IncrementalIndexer` or adding coordinator/context abstractions. Architecture risk is high because method placement is mechanical but mutation-gate lifetime, cancellation, retry ordering, shutdown drain, and durable projection publication are load-bearing.

## Global Constraints

- Preserve every `QueueRuntime` field, field type, constant value, watcher-facing signature, return value, log string, error path, retry limit, operation order, and test-only commit-failure behavior.
- Preserve the cycle order exactly: dirty Tantivy retry, queue batch, persisted extractor-repair retry, then overflow repair scan.
- Preserve the shutdown invariant that the queue-drain mutation guard is dropped before `retry_dirty_tantivy` reacquires the same workspace gate.
- Preserve each gate acquisition scope around its complete mutation batch; do not shorten, widen, or nest mutation guards.
- Keep `crates/julie-runtime/src/watcher/mod.rs` and all existing watcher behavior tests unchanged.
- Keep `QueueRuntime` and its fields in `runtime.rs`; do not widen fields to `pub(super)`.
- Keep watcher-facing methods physically in `runtime.rs` with their existing signatures and effective visibility.
- Use child modules of `watcher::runtime`, not sibling modules of `watcher::runtime`.
- Use explicit `crate::watcher::...` paths for helpers moved one module deeper; do not rely on stale `super::` meaning.
- Keep every Phase 2C production implementation file at or below 500 lines.
- Do not combine the split with behavior changes, retry-policy changes, lock substitutions, new adapters, or test rewrites.
- Do not push, merge, publish, or release without separate explicit approval.

---

## Architecture Quality

**Affected modules:** `crates/julie-runtime/src/watcher/runtime.rs` and its new private children.

**Caller-facing interface:** `IncrementalIndexer::start_watching` continues to construct `runtime::QueueRuntime` and call `run_cycle` / `drain_for_shutdown`; `IncrementalIndexer::process_pending_changes` continues to use `QueueRuntime::from_indexer(...).process_pending_changes()`. The `#[cfg(test)] IncrementalIndexer::process_pending_changes_with_commit_failure_for_test` seam remains available at the same path.

**Depth/locality check:** The parent facade owns shared runtime state and lifecycle ordering. `processing.rs` owns queue batching and shutdown dispatch, `repairs.rs` owns durable extractor-repair and overflow reconciliation, and `projection.rs` owns Tantivy retry/commit/freshness publication. Callers learn no new concepts.

**Test surface:** Existing tests continue through `IncrementalIndexer`, not private helpers. The only new test is structural and enumerates the Phase 2C implementation files.

**Seams/adapters:** Dependencies are in-process or local-substitutable through existing temp-workspace, SQLite, filesystem, and Tantivy fixtures. No port or adapter is justified. Child cross-calls use private `pub(super)` inherent methods only where the parent or another child must invoke them.

**Rejected shortcuts:** Separate coordinator structs duplicate the 17-field shared runtime context and create shallow interfaces. Passing a new `RuntimeContext` through free functions repeats the existing high-arity dispatch problem. Moving watcher-facing methods into children either changes effective Rust visibility or requires broader visibility declarations. Leaving one 400-plus-line repair file is accepted for this behavior-only split; splitting the repair algorithm further would be over-decomposition.

**Architecture risk:** High. The public product interface is unchanged, but concurrency and durable projection correctness depend on statement and scope ordering.

### Interface Lane Decision

- **Selected - facade-owned state plus child inherent implementations:** smallest caller surface, no field widening, existing tests remain authoritative.
- **Rejected - coordinator structs:** duplicates shared `Arc`/lock state, adds construction obligations, and creates speculative seams.
- **Rejected - context plus free functions:** increases parameter/context churn without reducing watcher caller obligations.

### Doubt Pass Resolution

A read-only Claude doubt pass was reconciled against the live code:

- Accepted: child modules must live under `watcher::runtime`; literal watcher-level siblings cannot access private `QueueRuntime` fields.
- Accepted: watcher-facing methods remain in the parent facade so their existing `pub(super)` visibility still means `crate::watcher`.
- Accepted: moved child code rewrites `super::dispatch_file_event` and `super::filtering` to explicit `crate::watcher::...` paths.
- Accepted: the Tantivy write-side child is named `projection.rs`, reflecting commit, retry, failure accounting, and durable projection state.
- Accepted: the deadlock invariant comment stays with the shutdown-drain implementation.
- Rejected: the boundary RED would fail on missing new files. The enumerated test checks the existing 1,099-line facade first, so RED terminates on the intended line-limit assertion before reaching later paths.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `docs/TESTING_GUIDE.md`, `xtask/test_tiers.toml`, and `docs/plans/2026-07-21-julie-improvement-roadmap-design.md`.

**Baseline evidence:** `cargo nextest run -p julie-runtime` passes 90/90 at `a400196f4045f1655293804cc397460ff6ec8934`; nextest reports one pre-existing leaky test process as report-only evidence.

**Worker red/green scope:** `cargo nextest run -p julie-runtime --lib tests::watcher_runtime_boundary::watcher_runtime_implementation_files_stay_within_limit`.

**Worker ceiling:** Run only the exact structural test once RED and once GREEN during the TDD loop. The lead owns existing watcher tests and broader gates.

**Worker gate invariant:** The initial RED names `src/watcher/runtime.rs` at 1,099 lines against a 500-line limit. GREEN proves the facade plus `processing.rs`, `repairs.rs`, and `projection.rs` all exist and are each at or below 500 lines.

**Lead affected-change scope:** After the coherent split and before the implementation commit, run `cargo xtask test changed`; production watcher paths must map to and pass `core-runtime` plus `workspace-runtime`.

**Lead focused scope:** Run `cargo check -p julie-runtime` and `cargo nextest run -p julie-runtime` at the exact implementation commit. Existing count is 90 passing tests plus the new boundary test.

**Branch gate:** Run `cargo xtask test dev` once at the final current HEAD before handoff.

**Specialist gate:** Run `cargo xtask test reliability` at the exact implementation commit because the touched area owns watcher lifecycle, mutation serialization, repair, and shutdown behavior.

**Replay/metric evidence:** Test outcomes, file limits, unchanged caller files, exact method signatures, exact cycle order, and gate-scope structure are hard gates. Durations and nextest leak reporting are report-only.

**Escalation triggers:** Any change to `watcher/mod.rs`, existing tests, runtime field shapes, operation ordering, log/error strings, gate lifetimes, retry policy, durable projection semantics, or shutdown behavior blocks completion. Product-code changes beyond the boundary require reassessing whether `system` or `full` is necessary.

**Assigned verification failure:** Workers stop and report when assigned verification fails unless this plan explicitly assigns that gate for update.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp in `docs/plans/2026-07-22-watcher-runtime-module-boundary-verification.md`. Evidence is reusable only at the exact recorded HEAD and scope.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Split watcher runtime behind its facade | None - serial | Create `crates/julie-runtime/src/watcher/runtime/processing.rs`, `crates/julie-runtime/src/watcher/runtime/repairs.rs`, `crates/julie-runtime/src/watcher/runtime/projection.rs`, `crates/julie-runtime/src/tests/watcher_runtime_boundary.rs`, and `docs/plans/2026-07-22-watcher-runtime-module-boundary-verification.md`; modify `crates/julie-runtime/src/watcher/runtime.rs` and `crates/julie-runtime/src/tests/mod.rs` only | Not applicable - single task. | Not applicable - single task. |

### Task 1: Split watcher runtime behind its facade

**Files:**
- Create: `crates/julie-runtime/src/watcher/runtime/processing.rs`
- Create: `crates/julie-runtime/src/watcher/runtime/repairs.rs`
- Create: `crates/julie-runtime/src/watcher/runtime/projection.rs`
- Create: `crates/julie-runtime/src/tests/watcher_runtime_boundary.rs`
- Create: `docs/plans/2026-07-22-watcher-runtime-module-boundary-verification.md`
- Modify: `crates/julie-runtime/src/watcher/runtime.rs:1-1099`
- Modify: `crates/julie-runtime/src/tests/mod.rs:1-15`

**Interfaces:**
- Consumes: `IncrementalIndexer`, `FileChangeEvent`, `MutationGuard<'static>`, `MutationGateRegistry`, `SharedIndexingRuntime`, `SharedEmbeddingProvider`, `SymbolDatabase`, and the optional `SearchIndex`.
- Produces: the identical `watcher::runtime::QueueRuntime` private facade and unchanged `IncrementalIndexer` watcher-facing behavior.

**Contract inputs:** `QueueRuntime::from_indexer`, `QueueRuntime::new`, `QueueRuntime::run_cycle`, `QueueRuntime::process_pending_changes`, `QueueRuntime::drain_for_shutdown`, and `IncrementalIndexer::process_pending_changes_with_commit_failure_for_test` retain their exact signatures and effective visibility. The existing `run_cycle_with_retry_age` and shutdown guard scopes define the ordering contract.

**File ownership:** Create `crates/julie-runtime/src/watcher/runtime/processing.rs`, `crates/julie-runtime/src/watcher/runtime/repairs.rs`, `crates/julie-runtime/src/watcher/runtime/projection.rs`, `crates/julie-runtime/src/tests/watcher_runtime_boundary.rs`, and `docs/plans/2026-07-22-watcher-runtime-module-boundary-verification.md`; modify `crates/julie-runtime/src/watcher/runtime.rs` and `crates/julie-runtime/src/tests/mod.rs` only

**Serialization required:** Not applicable - single task.

**Dependency reason:** Not applicable - single task.

**Step 1: Write the failing structural test**

Add `pub mod watcher_runtime_boundary;` to `crates/julie-runtime/src/tests/mod.rs`.

Create `crates/julie-runtime/src/tests/watcher_runtime_boundary.rs`:

```rust
use std::{fs, path::PathBuf};

#[test]
fn watcher_runtime_implementation_files_stay_within_limit() {
    for relative_path in [
        "src/watcher/runtime.rs",
        "src/watcher/runtime/processing.rs",
        "src/watcher/runtime/repairs.rs",
        "src/watcher/runtime/projection.rs",
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
cargo nextest run -p julie-runtime --lib tests::watcher_runtime_boundary::watcher_runtime_implementation_files_stay_within_limit 2>&1 | tail -20
```

Expected: FAIL because `src/watcher/runtime.rs` has 1,099 lines against the 500-line limit. The test checks that existing path first, so the failure must not be a missing-file error.

**Step 3: Write the minimal behavior-preserving implementation**

Keep this shape in `runtime.rs`:

```rust
mod processing;
mod projection;
mod repairs;

#[derive(Clone)]
pub(super) struct QueueRuntime {
    // Retain all existing fields, types, cfg attributes, and documentation.
}

impl QueueRuntime {
    // Retain from_indexer, new, acquire_gate_or_mark_rescan,
    // mark_rescan_pending_due_to_cancelled_gate, run_cycle,
    // run_cycle_with_retry_age, and process_pending_changes here.

    pub(super) async fn drain_for_shutdown(&self) {
        self.drain_for_shutdown_inner().await;
    }
}

// Retain the cfg(test) IncrementalIndexer commit-failure helper here.
```

Move the existing method bodies by responsibility:

- `processing.rs`: `drain_for_shutdown_inner` containing the unchanged former `drain_for_shutdown` body, `process_queue_batch`, and `projection_paths_for_event`.
- `repairs.rs`: `retry_persisted_repairs`, `repair_path_is_retryable`, `path_has_registered_extractor`, and `run_repair_scan_if_needed`.
- `projection.rs`: `retry_dirty_tantivy`, `handle_tantivy_retry_failure`, `persist_projection_state`, `record_commit_failure`, and `commit_search_index`.

Use `pub(super)` only for these cross-file inherent methods:

```rust
// processing.rs
pub(super) async fn drain_for_shutdown_inner(&self);
pub(super) async fn process_queue_batch(&self) -> usize;

// repairs.rs
pub(super) async fn retry_persisted_repairs(&self, min_repair_age: Duration) -> usize;
pub(super) async fn run_repair_scan_if_needed(&self);

// projection.rs
pub(super) async fn retry_dirty_tantivy(&self);
pub(super) async fn commit_search_index(
    &self,
    context: &str,
    affected_paths: &HashSet<String>,
);
```

All other moved helpers remain private inside their owning child module. Rewrite only module-relative paths whose meaning changes one level deeper:

```rust
crate::watcher::dispatch_file_event(...)
crate::watcher::filtering::build_gitignore_matcher(...)
crate::watcher::filtering::should_index_file(...)
```

Do not change statements, branch order, lock order, guard scopes, logging, retry constants, or durable projection writes. Keep the explicit shutdown-drain scope and its deadlock explanation around the mutation guard before the call to `retry_dirty_tantivy`.

**Step 4: Run compile and exact GREEN verification**

Run:

```bash
cargo check -p julie-runtime
```

Expected: PASS with `watcher/mod.rs` unchanged.

Run:

```bash
cargo nextest run -p julie-runtime --lib tests::watcher_runtime_boundary::watcher_runtime_implementation_files_stay_within_limit 2>&1 | tail -20
```

Expected: PASS 1/1 with every enumerated production file at or below 500 lines.

**Step 5: Apply commit mode**

- `serial-worker-commit`: after the exact GREEN test and lead inline review, checkpoint and commit the owned implementation files. Record the implementation SHA before exact-commit lead gates.

**Acceptance criteria:**
- [ ] The structural test fails on the 1,099-line facade before implementation and passes after every Phase 2C production file is at or below 500 lines.
- [ ] `QueueRuntime` retains all fields and constants in `runtime.rs` without widening field visibility.
- [ ] All watcher-facing methods retain their exact signatures, effective visibility, and caller paths.
- [ ] Cycle order remains dirty retry, queue batch, persisted repair retry, then overflow repair scan.
- [ ] Queue, repair, retry, and shutdown mutation guards retain their original acquisition/drop scopes.
- [ ] Shutdown releases its queue-drain guard before dirty-Tantivy retry.
- [ ] Retry limits, failure counters, repair reasons, durable projection writes, logs, and error behavior remain unchanged.
- [ ] `crates/julie-runtime/src/watcher/mod.rs` and all existing behavior tests remain unchanged.
- [ ] `cargo nextest run -p julie-runtime` passes at the exact implementation commit.
- [ ] `cargo xtask test changed` selects and passes the mapped runtime buckets before the implementation commit.
- [ ] `cargo xtask test reliability` and the final-current-HEAD `cargo xtask test dev` pass.
- [ ] The verification ledger records all hard gates at their exact SHAs.
- [ ] Worker-scope verification passes and the change is committed under serial-worker-commit mode.
