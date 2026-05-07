# Filewatcher Mutation Gate Architecture

> **For agentic workers:** REQUIRED SUB-SKILL: Use `razorback:subagent-driven-development` when subagent delegation is available. Fall back to `razorback:executing-plans` for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Eliminate the recurring "stale index" bug class by replacing the lossy pause/resume mechanism with a single shared per-workspace async mutation gate that all writers acquire before mutating, enforced at compile-time via a proof-token API. Make event flow observable at INFO level so future regressions surface immediately.

**Architecture:** Today the filewatcher coordinates with catch-up indexing by setting a pause flag and silently dropping events until a workspace rescan recovers them. This pattern has been patched at least 5 times in the past 2 months and still produces "new module not in index" reports under multi-session pressure (240 grace events + 487 catch-up runs in a single day on the developer's box). The fix is structural: extend the existing `indexing_lock_for_path` (`src/tools/workspace/commands/index.rs:20`) — currently used by force-reindex and refresh_stats — into a shared `src/workspace/mutation_gate.rs` module keyed by `workspace_id` (not raw `PathBuf`, which can split locks across path-spelling differences). All canonical writers acquire the same per-workspace `tokio::sync::Mutex<()>` before mutating: watcher event-processor, watcher repair scan, watcher repair-replay (`retry_persisted_repairs`), watcher Tantivy retry (`retry_dirty_tantivy`), catch-up indexer, force-reindexer, refresh-stats, and `register` (which currently calls `index_workspace_files` directly). To make nested gate acquisition a compile-time error rather than a runtime deadlock — `tokio::sync::Mutex` is non-reentrant — every mutation function takes a `&MutationGuard<'_>` proof token. Public `..._gated` wrappers acquire the gate and produce the token; inner `..._inner` helpers take the token by reference and assume the gate is held. Once writers are serialized through the gate via the proof-token API, the pause becomes unnecessary: events are processed in real time and simply block briefly when another writer holds the gate. No silent drops, no buffer queue, no doc/code divergence, no nested-acquisition deadlock possible.

**Tech Stack:** Rust, `tokio::sync::Mutex`, `notify` crate (FSEvents on macOS), SQLite (rusqlite), Tantivy. Touches `src/watcher/`, `src/workspace/`, `src/startup.rs`, `src/tools/workspace/commands/`, and a small slice of `src/handler.rs`.

**Origin:** This plan was revised twice based on Codex (gpt-5.5 high) consensus reviews on 2026-05-06.
- v1 → v2: Codex correctly identified that the original "buffer the pause" approach was a symptom fix and recommended the per-workspace mutation gate.
- v2 → v3: Codex re-review caught a critical deadlock (Task 3 had catch-up hold gate + call `handle_index_command` which also locks gate; `tokio::sync::Mutex` is non-reentrant), three missed writers (`retry_persisted_repairs`, `retry_dirty_tantivy`, `register_remove.rs`'s direct `index_workspace_files` call), a lock-identity bug (raw `PathBuf` keys split-lock across path-spelling differences), and a too-weak lock-order mitigation. v3 addresses all five findings via the proof-token API + workspace_id keys + complete writer coverage.

---

## Background: Evidence and Root Cause

### Verified working today (live test on 2026-05-06)
Modifying `src/handler.rs` content produced full re-index in ~2.5s end-to-end:
- `🔐 Starting ATOMIC incremental update: cleaning 1 files, inserting 1 files/187 symbols/135 relationships/1635 identifiers/101 types`
- `✅ Atomic incremental update complete in 133ms`
- `Watcher batch summary processed=1 deletes=0 renames=0`

So the common-case watcher path is intact. The bugs all surface at coordination boundaries.

### Structural diagnosis

The codebase has at least eight canonical writers per workspace, none of which currently coordinate through a shared lock except force-reindex and refresh-stats:

1. **Watcher event-processor** (`src/watcher/handlers.rs:75 handle_file_created_or_modified_static`) — extracts symbols and writes via `incremental_update_atomic`.
2. **Watcher repair scan** (`src/watcher/runtime.rs:565 run_repair_scan_if_needed`) — walks the workspace, dispatches missed events.
3. **Watcher repair replay** (`src/watcher/runtime.rs:159 retry_persisted_repairs`) — re-dispatches persisted repair events.
4. **Watcher Tantivy retry** (`src/watcher/runtime.rs:330 retry_dirty_tantivy`) — re-projects dirty Tantivy entries.
5. **Catch-up indexing** (`src/startup.rs:76 run_primary_workspace_repair` → `src/tools/workspace/indexing/pipeline.rs`) — runs at session connect, extracts and persists.
6. **Force-reindex** (`src/tools/workspace/commands/index.rs:123 handle_index_command`) — currently uses `indexing_lock_for_path`.
7. **Refresh stats** (`src/tools/workspace/commands/registry/refresh_stats.rs:48`) — currently uses `indexing_lock_for_path`.
8. **Workspace register** (`src/tools/workspace/commands/registry/register_remove.rs:83`) — calls `index_workspace_files` directly, bypassing both the named index and refresh paths.

The DB mutex protects individual SQLite operations but not the whole `extract → persist → project to Tantivy` pipeline. Writers 1-5 and 8 can dispatch a re-index while another is mid-flight, producing torn writes or stale extracted content overwriting fresher state.

The **pause mechanism** was added as a stopgap. Its current implementation (`src/watcher/events.rs:87-96`) drops events silently, only setting a `needs_rescan` flag. It also lies in its own doc comment (`src/workspace/mod.rs:801` says "events accumulate"; they don't). Multi-session pressure exposes the lossy behavior because each session-connect runs a catch-up that pauses the watcher.

### Why proof-token + workspace_id

- **Proof-token (`MutationGuard<'_>`)** makes nested-acquisition a compile error. `tokio::sync::Mutex` is non-reentrant — without compile-time enforcement, a future refactor that calls `handle_index_command` from inside a gated context would deadlock. Convention-based "always acquire gate first" is fragile (codex's v2 review caught exactly this latent bug in the v2 plan).
- **Workspace_id keys** prevent split-lock from path-spelling differences (`/foo/bar` vs `/foo/bar/`, vs canonicalized vs symlinked). Workspace_id is already the stable identifier used by `WatcherPool` and `WorkspacePool` throughout the codebase.

### Hidden observability (still in scope)

Today's daemon log: 16,531 INFO lines, **0 DEBUG lines**. After this work lands, operators must see "file X arrived, gate held by catch-up for 230ms, indexed 47 symbols" from daemon.log alone.

---

## File Structure

| File | Responsibility | Action | Notes |
|---|---|---|---|
| `src/workspace/mutation_gate.rs` | NEW: shared per-workspace `Arc<AsyncMutex<()>>` cache keyed by `workspace_id`. `MutationGuard<'_>` proof token. `acquire_gate(workspace_id) -> MutationGuard<'_>` async acquisition. RAII drop releases gate. | Create | ~150 LOC including tests for cache identity, proof-token lifetime, and per-workspace isolation. |
| `src/tools/workspace/commands/index.rs:14-36` | Delete local `indexing_lock_for_path` + `indexing_lock_cache`. Rewrite `handle_index_command` to call new `index_workspace_gated(workspace_id)` (acquires gate) which delegates to `index_workspace_inner(&guard, ...)` (the actual work). | Modify | The split is what avoids the v2 deadlock. Existing test `test_shared_index_lock_reuses_lock_for_same_path` should be replaced with a workspace_id-keyed test. |
| `src/tools/workspace/commands/registry/refresh_stats.rs:5, 48` | Update import; call `acquire_gate(workspace_id)` then call new `refresh_stats_inner(&guard, ...)`. | Modify | Mirrors index.rs pattern. |
| `src/tools/workspace/commands/registry/register_remove.rs:83` | Replace direct `index_workspace_files` call with `index_workspace_inner(&guard, ...)` after acquiring the gate via `acquire_gate(workspace_id)`. | Modify | This writer was outside the v1 and v2 plan scope; codex v3 review caught it. |
| `src/watcher/runtime.rs:430-563 (run_cycle)` | Acquire gate at start of mutation pass (after queue-empty check). Hold across the per-event loop. Pass `&MutationGuard<'_>` into `dispatch_file_event` and downstream `handle_file_*_static` functions. | Modify | Stays at ~784 LOC. Logic additions are minimal because we're just inserting `let _guard = mutation_gate::acquire_gate(workspace_id).await;` and threading the reference. |
| `src/watcher/runtime.rs:159 (retry_persisted_repairs)` | Acquire gate before dispatching retried events; pass token through. | Modify | New writer coverage per codex v3 finding 2. |
| `src/watcher/runtime.rs:330 (retry_dirty_tantivy)` | Acquire gate before Tantivy projection writes; pass token through. | Modify | New writer coverage per codex v3 finding 2. |
| `src/watcher/runtime.rs:565-748 (run_repair_scan_if_needed)` | Acquire gate after early-return checks; pass token to dispatch. | Modify | Already in v2 plan. |
| `src/watcher/handlers.rs:75-356` | `handle_file_created_or_modified_static`, `handle_file_deleted_static`, `handle_file_renamed_static` take `_guard: &MutationGuard<'_>` parameter. The proof-token parameter is the compile-time enforcement. | Modify | Signature change; ripples through all callers (which is the point — calling without the gate becomes a build error). |
| `src/watcher/events.rs:46-104` | Delete `process_file_system_event_with_pause`. Single `process_file_system_event` function. Events are queued unconditionally; the queue processor blocks on the gate. | Modify | Net deletion of pause-aware variant. |
| `src/watcher/mod.rs:536-565` | Delete `pause()`, `resume()`, `pause_flag`, `paused_event_count`. | Modify | Net deletion. |
| `src/workspace/mod.rs:793-815` | Delete `pause_file_watching`, `resume_file_watching`. | Modify | Net deletion. |
| `src/handler.rs:1429-1442` | Delete `pause_watcher`, `resume_watcher`. | Modify | Net deletion. Callers (startup.rs, force_safeguards.rs) acquire gate directly. |
| `src/handler.rs:797 (catchup_in_progress)` | **Remove** the per-handler flag entirely. The mutation gate provides cross-session catch-up coordination naturally. | Modify | v3: codex finding 5 — clarified to remove (not "promote OR remove"). |
| `src/startup.rs:76-200 (run_primary_workspace_repair, pause_primary_workspace_updates, resume_primary_workspace_updates)` | Replace pause/resume with `let guard = acquire_gate(workspace_id).await;` then call ungated `_inner` helpers. | Modify | The dropped `_guard` releases gate at function end. |
| `src/tools/workspace/commands/force_safeguards.rs:74-100` | Same pattern: acquire gate → call `_inner` helpers → drop guard. | Modify | Mirrors startup.rs. |
| `src/watcher/observability.rs` | NEW: rate-limiter + summary log helpers including gate-wait timing. | Create | ~100 LOC. |
| `src/watcher/handlers.rs (debug! → info! sites)` | Promote 4-5 key debug! to info! through the new rate-limiter. | Modify | Small surgical changes. |
| `src/tests/integration/watcher_mutation_gate.rs` | RENAME from `src/tests/integration/watcher_pause.rs`. DELETE the existing `test_paused_event_ingestion_sets_rescan_without_queueing` (asserts the OLD drop behavior). REPLACE with the gate-based regression tests in Task 7. | Modify (and rename) | Existing test was a regression guard for the WRONG behavior. |
| `src/tests/integration/watcher_observability.rs` | NEW: assert key INFO log lines fire so future regressions are caught by tests. | Create | ~150 LOC. |
| `CLAUDE.md` | Add brief "Filewatcher Mutation Gate" subsection under "WORKSPACE ARCHITECTURE" pointing operators at the new INFO log lines and naming the gate as the coordination primitive. | Modify | Mechanical. |

Estimated total: ~700 added (counting tests), ~280 removed, ~200 modified. Net +220 LOC because the test coverage and proof-token plumbing are the value.

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` ("RUNNING TESTS" section), `AGENTS.md`, `docs/TESTING_GUIDE.md`. Use the `cargo xtask test` runner; do not invent commands.

**Worker red/green scope:** `cargo nextest run --lib <exact_test_name> 2>&1 | tail -10`. One narrow test per RED/GREEN cycle. Workers limited to 2 test runs per fix per CLAUDE.md.

**Worker ceiling:** A single named test for the worker's behavior. Workers do not own broader regression gates. Workers MUST NOT run `cargo xtask test changed`, `cargo xtask test dev`, or any tier; the lead handles regression checks.

**Worker gate invariants (per task):**
- Task 1 (mutation gate module): same-workspace_id concurrent acquisitions serialize; different-workspace_id acquisitions proceed in parallel; lock cache does not leak; `MutationGuard<'_>` cannot outlive its `MutexGuard`.
- Task 2 (watcher integration): all four watcher mutation sites (run_cycle, run_repair_scan_if_needed, retry_persisted_repairs, retry_dirty_tantivy) acquire the gate before mutating and pass the token through to handler calls; concurrent catch-up cannot interleave a write between extract and persist.
- Task 3 (catch-up integration + nested-call split): `run_primary_workspace_repair` acquires gate ONCE at top, calls `_inner` helpers; calling `handle_index_command` (the gated variant) from within a gate-holding context is a compile error.
- Task 4 (other-writer migration): force-reindex, refresh-stats, and register_remove all acquire the gate via the public `..._gated` wrapper and never via `..._inner` directly from outside a held context.
- Task 5 (pause removal): no `pause_flag`, `pause()`, `resume()`, `pause_watcher`, `resume_watcher`, `pause_file_watching`, `resume_file_watching`, `process_file_system_event_with_pause`, `paused_event_count`, or `catchup_in_progress` symbols remain. Compilation succeeds.
- Task 6 (observability): per-batch INFO summary visible in daemon.log without debug build; gate-wait timing reported when > 100ms; rate-limited.
- Task 7 (regression tests, lead-owned): a file created or modified during a held-gate window is in the index within `<gate-release + queue-tick>` time without manual rescan; concurrent catch-ups serialize; nested-acquisition is a compile error (verified by a `compile_fail` doctest).

**Lead affected-change scope:** `cargo xtask test changed` after the lead consolidates a coherent batch of worker commits.

**Branch gate:** `cargo xtask test reliability` (daemon + workspace_init + integration buckets) followed by `cargo xtask test full` once before merge. Both lead-owned.

**Replay/metric evidence:**
- Hard gate: the new mutation-gate regression tests pass; the `compile_fail` doctest proves nested-acquisition is impossible.
- Hard gate: live smoke — touch a file while a session is connected, verify INFO log entry appears AND symbols are queryable via `fast_search` within 5s.
- Hard gate: live smoke — run two `manage_workspace(operation="index", force=true)` calls concurrently from two sessions; verify they serialize (no torn writes, no deadlock).
- Report-only: daemon log shows new INFO lines fire during multi-session activity; gate hold-time stays under 500ms p99 for normal operation, under 30s for catch-up on fresh clones.

**Escalation triggers:**
- Any worker pass leaves a flaky concurrency test (gate ordering hazard → lead owns).
- Two worker attempts fail review on the same task.
- Test reveals a hidden invariant in `dispatch_file_event`, `incremental_update_atomic`, or any nested gate-acquisition path → escalate to strategy tier.
- Any test for a non-watcher writer breaks: stop and investigate the gate-acquisition order.
- Performance: gate hold-time exceeds 30s for steady-state operation (not catch-up on fresh clones) — investigate whether a writer is leaking the guard.

**Assigned verification failure:** Workers stop and report when assigned verification fails. They do not commit broken tests, they do not patch on top, and they do not present a failure as evidence.

**Verification ledger:** Use `docs/plans/verification-ledger-template.md`. Record invariant, command, scope label, commit SHA, result, and timestamp per task. Reuse only when both scope label and commit SHA match HEAD exactly.

---

## Model Routing

**Project source of truth:** `RAZORBACK.md` at repo root. This work is in the "watcher or session reference ownership" + "concurrency, restart" categories — both flagged as shared-invariant work.

**Strategy tier:** planning, decomposition, lead review, finding triage.
- Codex mapping: `gpt-5.5 high` for plan, contract, and lead decisions.
- Claude mapping: Opus.

**Implementation tier (coupled):** all watcher/gate integration work is shared-invariant and concurrency-sensitive — use the coupled implementation tier.
- Codex mapping: `gpt-5.3-codex xhigh` (concurrency, restart-adjacent).
- Claude mapping: Sonnet high.

**Mechanical tier:** doc comment updates, import path changes, manifest tweaks with no test/replay/gate ownership.
- Codex mapping: `gpt-5.4-mini medium`.
- Claude mapping: Haiku.

**Gate-interpretation reviewer:** reviews plan + failing-gate-test + diff to decide whether the test or implementation is wrong.
- Codex mapping: `gpt-5.3-codex high`.
- Claude mapping: Opus or Sonnet high.

**Escalation tier:** subtle correctness, repeated worker failure, weak tests, gate interpretation, ordering deadlock concerns.
- Codex mapping: `gpt-5.5 high/xhigh`.
- Claude mapping: Opus.

**Worker eligibility:**
- Implementation-tier workers may take Tasks 4 (other-writer migrations, AFTER Task 1 finalizes the proof-token API), 6 (observability), and 8 (doc cleanup).
- Tasks 1, 2, 3, 5 are coupled-implementation (concurrency-sensitive, lock ordering matters) — stay in lead session OR go to a single coupled-tier worker, never split across multiple unattended workers.
- **Task 7 is lead-owned** (codex v3 finding 5): the regression tests define the concurrency contract; an implementation worker could write tests that encode the wrong behavior and inadvertently lock in a bug.

**Task ordering dependency:** Task 4 cannot start until Task 1 lands (the proof-token API must be stable before other writers migrate). Tasks 2, 3 also depend on Task 1. Task 5 (pause removal) is the final consolidation — only do it AFTER Tasks 2, 3, 4 have all migrated their callers.

**Escalation triggers:** Any unexpected interaction with `incremental_update_atomic`, `dispatch_file_event`, `WatcherPool` lifecycle, or session-connect catch-up coordination → escalate to strategy tier.

**Mechanical exclusion:** Mechanical workers cannot own the regression tests (Task 7) or any task involving the proof-token API design (Tasks 1, 3).

---

## Tasks

### Task 1: Build shared mutation gate with proof-token API

**Files:**
- Create: `src/workspace/mutation_gate.rs`
- Modify: `src/workspace/mod.rs` (add `pub mod mutation_gate;`)
- Test: inline `#[cfg(test)] mod tests` in mutation_gate.rs (small new module, tests are unit-level).

**What to build:** A new module with:
1. `MutationGuard<'a>` struct wrapping `tokio::sync::OwnedMutexGuard<()>` (or `MutexGuard<'a, ()>`). Holds the lock for its lifetime; drop releases it.
2. `acquire_gate(workspace_id: &str) -> MutationGuard<'static>` async function. Looks up or creates an `Arc<AsyncMutex<()>>` in a `OnceLock<StdMutex<HashMap<String, Arc<AsyncMutex<()>>>>>`, awaits `lock_owned()`, wraps in `MutationGuard`.
3. The `MutationGuard` is the proof token — every mutation function takes `_guard: &MutationGuard<'_>` as a parameter. Calling such a function without a guard is a compile error.
4. A `compile_fail` doctest demonstrating that you can't construct `MutationGuard` outside the module (private constructor).

**Approach:**
- Cache uses `OnceLock<StdMutex<HashMap<String, Arc<AsyncMutex<()>>>>>` mirroring the current `indexing_lock_cache` pattern.
- Key by `workspace_id` (already a stable string identifier in WatcherPool/WorkspacePool). Codex v3 finding 4: this avoids split-locks from path-spelling differences.
- Provide `#[cfg(test)] pub fn clear_cache_for_test()` for tests.
- The `MutationGuard` type does NOT need to expose the inner mutex guard — it's just a "I am proof the gate is held" token. Make the inner field private.

**Acceptance criteria:**
- [ ] Two `acquire_gate(p)` calls for the same `p` return guards backed by the same Arc.
- [ ] Different workspace_ids return different Arcs (verified by lock-and-poll test).
- [ ] `compile_fail` doctest demonstrates `MutationGuard` cannot be constructed externally.
- [ ] Worker-scope verification: `cargo nextest run --lib mutation_gate 2>&1 | tail -10`.

---

### Task 2: Watcher mutation sites acquire the gate (4 sites)

**Files:**
- Modify: `src/watcher/runtime.rs:430-563` (`run_cycle` — acquire gate before draining queue)
- Modify: `src/watcher/runtime.rs:565-748` (`run_repair_scan_if_needed` — acquire gate after early-return checks)
- Modify: `src/watcher/runtime.rs:159` (`retry_persisted_repairs` — acquire gate before dispatching retries)
- Modify: `src/watcher/runtime.rs:330` (`retry_dirty_tantivy` — acquire gate before Tantivy projection writes)
- Modify: `src/watcher/handlers.rs:75-356` — handler functions take `_guard: &MutationGuard<'_>` parameter
- Modify: `src/watcher/runtime.rs` `dispatch_file_event` — takes `_guard: &MutationGuard<'_>`, threads it to handlers
- Test: New tests in renamed `src/tests/integration/watcher_mutation_gate.rs`

**What to build:** All four watcher mutation paths acquire the gate before mutating. Handler signatures take the proof token, making it impossible to call them from outside a gated context.

**Approach:**
- At each mutation site: `let _guard = mutation_gate::acquire_gate(workspace_id).await;` then call existing logic, threading `&_guard` into handler calls.
- The watcher's `IncrementalIndexer` already knows its workspace path; derive `workspace_id` once at construction and store it.
- Hold the gate for the duration of each mutation pass — typically 1-100ms for run_cycle. Don't hold across `tokio::time::sleep` or other long awaits.

**Acceptance criteria:**
- [ ] All four watcher mutation sites acquire and hold the gate.
- [ ] Handler functions require `_guard` parameter; calling without it fails to compile.
- [ ] If catch-up is holding the gate, watcher's run_cycle blocks (verifiable via test that injects a gate-hold).
- [ ] Worker-scope verification: `cargo nextest run --lib tests::integration::watcher_mutation_gate::test_watcher_blocks_during_catchup 2>&1 | tail -10`.

---

### Task 3: Split catch-up indexer into gated wrapper + ungated inner

**Files:**
- Modify: `src/startup.rs:76-200` (`run_primary_workspace_repair` and helpers)
- Modify: `src/tools/workspace/indexing/pipeline.rs` if needed (to expose ungated inner helpers)
- Test: extend `src/tests/integration/watcher_mutation_gate.rs`.

**What to build:** Split the catch-up indexer into a public `_gated` wrapper that acquires the gate and a private `_inner` helper that takes `&MutationGuard<'_>` and does the work. This is what avoids the v2 deadlock: catch-up can call `_inner` while holding its own guard, AND public callers (like force-reindex via handle_index_command) acquire the gate via `_gated`.

**Approach:**
- `run_primary_workspace_repair` becomes: `let guard = mutation_gate::acquire_gate(workspace_id).await; run_primary_workspace_repair_inner(&guard, ...).await`.
- The `_inner` function takes `&MutationGuard<'_>` and contains the existing snapshot/plan/cancel/index/refresh logic.
- Any call from `_inner` into another mutation function uses the `_inner` variant of that function (passing `&guard`), NEVER the `_gated` variant.
- Remove `pause_primary_workspace_updates` and `resume_primary_workspace_updates`; the gate replaces them.
- Per-handler `catchup_in_progress` flag (`src/handler.rs:797`) is removed — the gate provides cross-session coordination.

**Acceptance criteria:**
- [ ] `run_primary_workspace_repair` (gated) holds the gate for the whole repair pass.
- [ ] `run_primary_workspace_repair_inner` takes `&MutationGuard<'_>` and does NOT acquire the gate.
- [ ] Concurrent calls to `run_primary_workspace_repair` from two daemon-mode sessions serialize correctly.
- [ ] Calling `handle_index_command` (gated) from inside `_inner` is a compile error (the `_inner` variants are scoped to coordinate with each other).
- [ ] Worker-scope verification: `cargo nextest run --lib tests::integration::watcher_mutation_gate::test_concurrent_catchup_serializes 2>&1 | tail -10`.

---

### Task 4: Other writers migrate to gated/inner pattern (3 writers)

**DEPENDENCY: Task 1 must land first** so the `MutationGuard` API is stable.

**Files:**
- Modify: `src/tools/workspace/commands/index.rs:14-160` — delete local `indexing_lock_for_path` + `indexing_lock_cache`. Split `handle_index_command` into `index_workspace_gated(workspace_id, ...)` (acquires gate) and `index_workspace_inner(&guard, ...)` (does work).
- Modify: `src/tools/workspace/commands/registry/refresh_stats.rs:5, 48` — `refresh_workspace_internal` becomes a `_gated` wrapper acquiring gate; logic moves to `refresh_workspace_inner(&guard, ...)`.
- Modify: `src/tools/workspace/commands/registry/register_remove.rs:83` — replace direct `index_workspace_files` with `index_workspace_inner(&guard, ...)` after `let guard = acquire_gate(workspace_id).await;`. (Codex v3 finding 2.)
- Modify: `src/tools/workspace/commands/force_safeguards.rs:74-100` — `pause_force_reindex_watchers` becomes `acquire_force_reindex_gate(workspace_id) -> MutationGuard<'_>`; resume disappears (drop guard).

**What to build:** Three currently-uncoordinated writers (force-reindex, refresh-stats, register) all migrate to the `_gated` / `_inner` split, sharing the same gate as the watcher and catch-up.

**Approach:** Mechanical refactor of four call sites. Behavior preserved (gate-held writers still serialize through one mutex), but the implementation no longer touches the watcher's pause flag, and `register` (previously bypassing all coordination) is now coordinated.

**Acceptance criteria:**
- [ ] All eight pre-existing writers (4 watcher + catch-up + force-reindex + refresh-stats + register) acquire the same gate via `_gated` wrappers.
- [ ] Existing `test_shared_index_lock_reuses_lock_for_same_path` is REPLACED with workspace_id-keyed equivalent that still passes.
- [ ] Force-reindex integration tests still pass.
- [ ] Worker-scope verification: `cargo nextest run --lib tools::workspace::commands::index 2>&1 | tail -10` AND `cargo nextest run --lib tools::workspace::commands::registry 2>&1 | tail -10`.

---

### Task 5: Remove pause/resume infrastructure

**DEPENDENCY: Tasks 2, 3, 4 must land first** (all callers migrated to gate).

**Files:**
- Modify: `src/watcher/events.rs:46-104` — delete `process_file_system_event_with_pause`; rename `process_file_system_event` to be the only function; remove `pause_flag` and `paused_event_count` parameters from internal helpers.
- Modify: `src/watcher/mod.rs:536-565` — delete `pause()` and `resume()` methods; delete `pause_flag` and `paused_event_count` fields and their initializers.
- Modify: `src/watcher/mod.rs:412-449` — event-detector spawn no longer passes `pause_flag` and `paused_event_count`.
- Modify: `src/workspace/mod.rs:793-815` — delete `pause_file_watching` and `resume_file_watching`.
- Modify: `src/handler.rs:1429-1442` — delete `pause_watcher` and `resume_watcher`.
- Modify: `src/handler.rs:797` — remove `catchup_in_progress` field and its initializer.

**What to build:** Net code deletion. After Tasks 2-4, all pause-call-sites have migrated to gate acquisition. The pause infrastructure is unused; remove it.

**Approach:**
- Use `fast_refs(symbol="pause_watcher")`, `fast_refs(symbol="pause_file_watching")`, `fast_refs(symbol="catchup_in_progress")` etc. before deleting to confirm nothing still references them.
- The `set_watcher_paused` method in `SharedIndexingRuntime` (called from `pause()`/`resume()` per `src/watcher/mod.rs:541-547, 552-563`) — if only used for pause coordination, remove it too.
- Compilation must succeed after deletion.

**Acceptance criteria:**
- [ ] `rg -n 'pause_flag|pause_file_watching|pause_watcher|paused_event_count|process_file_system_event_with_pause|catchup_in_progress' src/` returns zero hits.
- [ ] `cargo build --quiet` succeeds.
- [ ] Worker-scope verification: `cargo nextest run --lib tests::integration::watcher 2>&1 | tail -10`.

---

### Task 6: INFO-level event observability

**Files:**
- Create: `src/watcher/observability.rs` — rate-limiter + summary log helpers including gate-wait timing.
- Modify: `src/watcher/handlers.rs:82, 111-114, 183-189, 347` (key debug! → info! via new helpers).
- Modify: `src/watcher/runtime.rs:474-478, 511, 546` — promote per-batch logs through the rate-limiter.

**What to build:** Operators must see, from daemon.log alone:
- A file event arrived for path X (rate-limited; folded into batch summary if > 5/sec).
- File X was indexed (with symbol count) OR skipped because hash matched.
- Gate acquisition latency (e.g., "waited 230ms for mutation gate") when it exceeds 100ms.
- Per-batch summary including number of gate waits this batch and cumulative wait time.

**Approach:**
- Per-watcher rate limiter using `AtomicU64` for last-emit timestamp + counter. No new crate dependencies.
- Wrap `mutation_gate::acquire_gate(...)` calls in a small helper that times the await and logs INFO if > 100ms.
- Pattern: "in the last second, processed N files (M unchanged, K extracted), waited L ms total on gate".

**Acceptance criteria:**
- [ ] After a content change, daemon.log (INFO level) shows file path and symbol count within ~3s.
- [ ] When catch-up holds the gate for >100ms, watcher logs an INFO line with wait time on first acquisition after the wait.
- [ ] No log spam: a 1000-file rebuild produces ≤ 60 file-event INFO lines (rate-limited).
- [ ] Worker-scope verification: `cargo nextest run --lib tests::integration::watcher_observability 2>&1 | tail -10`.

---

### Task 7: Mutation-gate regression tests (LEAD-OWNED)

**Per codex v3 finding 5: this task defines the concurrency contract and must be lead-owned, not implementation-tier.**

**Files:**
- Modify (and rename): `src/tests/integration/watcher_pause.rs` → `src/tests/integration/watcher_mutation_gate.rs`. The existing `test_paused_event_ingestion_sets_rescan_without_queueing` literally asserts the OLD drop behavior — DELETE that test, replace with the new ones below.

**What to build:** Integration tests that exercise the bug class:
1. `test_new_file_during_held_gate_is_indexed_after_release`: spawn a holder that acquires the gate, create a new `.rs` file via `std::fs`, drop the holder, assert file appears in DB symbols within 5s.
2. `test_modified_file_during_held_gate_is_reindexed_after_release`: hold gate, modify content of an indexed file, release, assert symbols updated.
3. `test_concurrent_catchup_serializes`: spawn two `run_primary_workspace_repair` calls concurrently, assert they run serially (not interleaved); verify via execution-order tracking.
4. `test_watcher_blocks_during_catchup`: simulate catch-up holding the gate; verify watcher's `run_cycle` blocks until release; verify the queued events are processed correctly after.
5. `test_force_reindex_blocks_watcher`: force-reindex holds gate; verify watcher's queue grows but doesn't process; verify it drains correctly after release.
6. `test_register_workspace_serializes_with_watcher`: register a new workspace concurrently with watcher activity on an existing one; assert no torn writes.
7. `test_workspace_id_keyed_not_path_keyed`: acquire gate via `/foo/bar` and `/foo/bar/` style different paths but same workspace_id; assert second call blocks (proves canonicalization works).
8. `test_mutation_guard_cannot_be_constructed_externally`: `compile_fail` doctest in mutation_gate.rs proves the proof-token's safety.

**Approach:**
- Reuse harness from existing `src/tests/integration/watcher_handlers.rs` and `watcher_queue.rs`.
- Use a tempdir workspace with a real `IncrementalIndexer`, a real `notify` watcher, and a real SQLite db.
- Allow up to 5s polling for assertions to absorb FSEvents latency on macOS.

**Acceptance criteria:**
- [ ] All 8 tests RED before Tasks 1-5 land, GREEN after.
- [ ] Tests are not flaky on 10 consecutive runs (lead-owned check before merge).
- [ ] Worker-scope verification: `cargo nextest run --lib tests::integration::watcher_mutation_gate 2>&1 | tail -10`.

---

### Task 8: Doc-comment cleanup + CLAUDE.md note

**Files:**
- Modify: any remaining doc comments referencing pause semantics in `src/watcher/`, `src/workspace/`, `src/handler.rs`, `src/startup.rs`.
- Modify: `CLAUDE.md` — add brief "Filewatcher Mutation Gate" subsection under "WORKSPACE ARCHITECTURE" pointing operators at the new INFO log lines and naming the gate as the coordination primitive.

**What to build:** Update docs to reflect the new architecture. No "pause" or "events accumulate" phrasing should remain. Add operator note: "All workspace mutations serialize through `mutation_gate::acquire_gate(workspace_id)`. If you see `Watcher waited Nms on mutation gate` log lines exceeding 1s for steady-state operation, investigate the writer holding the gate (catch-up indexing, force-reindex, register, or refresh-stats)."

**Approach:** Pure documentation.

**Acceptance criteria:**
- [ ] No remaining "events continue to accumulate" claims in the codebase.
- [ ] No remaining doc comments referring to pause/resume that don't exist anymore.
- [ ] CLAUDE.md mentions the mutation gate, the proof-token API, and which INFO log lines to look for.

---

## Out of Scope (explicit)

To prevent this plan from sprawling further:

- **Watcher restart blind-spot fix** (daemon stale-binary auto-restart leaves a brief window with no watcher). Real but separate problem; track separately. The mutation gate doesn't address this since the watcher process itself is restarting.
- **Symbol extraction failures** that cause "indexed but no symbols" outcomes — different bug class, different file.
- **Tantivy commit failures beyond `retry_dirty_tantivy`** that cause "in DB but not searchable" outcomes — different bug class.
- **File watcher unification** with the catch-up indexer code path (one canonical writer instead of multiple writers behind one gate) — desirable long-term but bigger scope.

If review reveals one of these is actually the user's primary symptom, that's an escalation trigger and we revise scope before implementing.

---

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Holding the gate during a slow catch-up (e.g., 30s on a fresh clone) blocks user-visible operations behind it. | Catch-up already had this latency property when it paused the watcher; the user just couldn't see it. The new INFO logs make it visible. If wait times become a problem, sub-divide the catch-up into smaller gate-held passes. |
| Lock ordering deadlock if a writer takes the DB mutex before the gate, and another takes them in opposite order. | **API-enforced**: `MutationGuard<'_>` is the proof token; mutation functions take `&MutationGuard<'_>` as a parameter. Public `_gated` wrappers acquire the gate; private `_inner` helpers require the token. **Calling an `_inner` function without the token is a compile error.** Convention is no longer load-bearing. (Codex v3 finding 3.) |
| Nested gate acquisition deadlock (`tokio::sync::Mutex` non-reentrant). | **API-enforced**: same proof-token mechanism. `_inner` functions never call `_gated` functions; the type system makes the wrong call shape impossible to express. |
| Gate cache grows unbounded if many workspaces are touched. | The existing `indexing_lock_for_path` has the same property and has been fine. Defer eviction to follow-up work; not on the path of this bug class. Track in TODO. |
| Tests for concurrency are flaky on slow CI. | Use polling with 5s timeout, not fixed sleeps. Re-run flaky tests under `cargo nextest --no-fail-fast` to triage. **Task 7 is lead-owned** (codex v3 finding 5) so flake investigation lands on the lead, not workers. |
| Lock identity bug: same workspace via different path spellings creates two locks. | **Key by workspace_id** (already a stable identifier in WatcherPool/WorkspacePool), not by raw `PathBuf`. Workspace_id is derived from canonicalized path at workspace registration. (Codex v3 finding 4.) |

---

## Estimated Effort

- Task 1 (mutation_gate.rs with proof-token): 2-3 hours (coupled implementation — designing the safe API).
- Task 2 (watcher gate integration, 4 sites): 3-4 hours (coupled implementation).
- Task 3 (catch-up gated/inner split): 2-3 hours (coupled implementation).
- Task 4 (other-writer migration, 3 writers): 2-3 hours (implementation tier — depends on Task 1).
- Task 5 (pause removal): 1-2 hours (coupled implementation — touches many files).
- Task 6 (observability): 2 hours (implementation tier).
- Task 7 (regression tests, LEAD-OWNED): 4-6 hours (lead).
- Task 8 (doc cleanup): 30 minutes (mechanical tier).
- Lead review + integration verification: 3 hours.

Total: ~20-26 hours of focused work. Larger than v1's buffering plan, but produces a structurally correct fix with compile-time safety against the entire bug class.
