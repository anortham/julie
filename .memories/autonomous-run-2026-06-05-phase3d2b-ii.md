# Autonomous Execution Report - Phase 3d.2b-ii (delete daemon http-server runtime + de-type pools)

**Status:** Complete
**Plan:** docs/plans/2026-06-05-julie-phase3d-delete-daemon-adapter.md
**Branch:** julie-rescue-phase3d2b-ii (base main @ f39467a9)
**PR:** https://github.com/anortham/julie/pull/37
**Duration:** multi-session (resumed across compaction)
**Phases:** 3d.2b-ii / 1 of 1 complete (the atomic delete half of 3d.2b)
**Tasks:** 8/8 complete (Workers A–D + B1/B2/B3 fix batch + lead gate)

## What shipped
- Atomic server-core kill (one commit d8a872c1): deleted `daemon/app/**` + `{http_transport,transport,mcp_session,token_file,singleton,fd_limit,shutdown_event,workspace_pool,watcher_pool}.rs` + their tests (~16.5k LOC).
- De-typed kept code off `WorkspacePool`/`WatcherPool`: `handler.rs`, `handler/{tool_metrics,workspace_session_attachment}.rs`, `dashboard/state.rs` (+5 routes), `health/checker.rs`, `tests/helpers/workspace.rs`. Kept `SessionTracker`/`SessionPhaseCounts`/`LifecyclePhase`/`LifecyclePhaseKind`/`ShutdownCause` + `drain_sessions` + RecoveryMarker family (→3d.3).
- Pruned daemon-controller symbols from `lifecycle.rs`/`mod.rs`; kept recovery/dashboard surface under `#[allow(dead_code)]`.
- Storage-anchor read-path fix: `workspace_index_dir_for` now consults `in_process_index_root` first → rebound/secondary/reference workspaces keep the shared `~/.julie/indexes/` anchor (the root the deleted pool used to supply). stdio mode (None) unchanged.
- Flipped the `in_process_boundary` tripwire; repaired `transport`/`lifecycle`/`workspace-runtime` xtask buckets + manifest-contract snapshot; stubbed `search_matrix::run_baseline_async` → `bail!` (rewire →3d.3).
- 8 pool-backed daemon multi-workspace session/roots/swap lifecycle tests `#[ignore]`d with a Phase 3d.3 reason.

## Judgment calls (non-blocking decisions made)
- `src/handler.rs:2265` (`workspace_index_dir_for`) — landed the de-pool storage-anchor read-path fix at the single read-path chokepoint (consult `in_process_index_root` first), mirroring the existing `anchor_override.parent().join(id)` resolution. The de-pool had wired `in_process_index_root` into the WRITE paths but missed this resolver; this was the root cause of the rebound/reference routing regressions.
- 8 residual daemon multi-workspace tests — chose `#[ignore]` with a 3d.3 reason (owner decision "Ignore w/ 3d.3 reason, ship") over rewiring, because the pool-backed multi-workspace session attachment can't be reconstructed without 3d.3's daemon.db→registry.db registry work.
- codex F1 + F2 — classified BOTH as pre-existing-since-3c.3 (verified against BASE), flagged-not-fixed: each needs 3d.3 infrastructure or a product-intent decision a deletion PR shouldn't make. Tracked both in the plan's 3d.3 section so the work isn't lost.
- Branch gate authority — used the per-crate superset `nextest -p julie -E 'not test(search_quality)'` (NOT the xtask bucket tiers, which timeout-flake under load), per [[feedback_prefer_fast_per_crate_gates]].
- Reused the d8a872c1 gate evidence for the docs-only follow-up commit 939842c9 (plan file, test-irrelevant) rather than re-running the 995s gate.

## External review (codex, adversarial)
- **Findings:** 2 (verdict: needs-attention)
- **Verified real, fixed:** 0
- **Dismissed:** 0
- **Flagged for your review:** 2 — both verified real but PRE-EXISTING in-process behavior inherited from the 3c.3 cutover, not regressions introduced by 3d.2b-ii.
  - **F1 (high, conf 0.88) — in-process workspace cleanup has no active-workspace interlock.** Why uncertain / why flagged: the fix is a product-intent decision (should `manage_workspace remove` block the current primary?) coupled to 3d.3's session_count rebuild. Verified pre-existing: at BASE `attach_workspace_resources` early-returned on `pool=None` before incrementing session_count, and watcher_ref_count/live_indexing_reason/remove_runtime_if_inactive were already no-ops for the in-process server — runtime behavior byte-identical pre/post-PR. Impact below codex's "high": removed guards were multi-session daemon interlocks meaningless single-session; delete is user-initiated; only the derived index is removed (source untouched, re-indexable).
  - **F2 (medium, conf 0.92) — cross-workspace tool-call metrics record primary source_bytes.** Why flagged: the fix (carry a target DB handle/path into `MetricsTask`) is 3d.3 telemetry/dashboard-rework infrastructure. Verified pre-existing: the old test set up a WorkspacePool and exercised the daemon resolution path; the in-process server never had that pool, so it has recorded primary bytes since the 3c.3 cutover. The test rewire (`target_bytes`→`primary_bytes`) documents the in-process reality, doesn't hide a new regression.
- Positive codex confirmed: the storage-anchor read-path fix is correctly anchored for live `new_in_process` sessions.

## Tests
- Branch gate: `cargo nextest run -p julie --no-fail-fast -E 'not test(search_quality)'` → 1452/1452 passed (174 slow, 1 leaky), 113 skipped, 0 failed (995.6s) @ d8a872c1.
- First fully-GREEN superset of the 3d teardown: the 2 long-standing pre-existing failures (daemon-mcp.token harness bug + daemon reaper load-flake) self-resolved by this PR's deletions, as the 3d.2a/3d.2b-i ledgers predicted.
- Compile authority: `cargo nextest run -p julie --no-run` clean.

## Blockers hit
- None.

## Files changed
- 121 files: +1133 / −16568 (net ~−16.5k LOC). Code commit d8a872c1 (120 files), docs commit 939842c9 (1 file).

## Next steps
- Review PR #37: https://github.com/anortham/julie/pull/37
- Decide F1's in-process remove/cleanup safety model (product-intent) — tracked to 3d.3.
- Decide F2's cross-workspace metrics attribution (carry target DB into MetricsTask) — tracked to 3d.3.
- After merge: 3d.3 (daemon.db→registry.db, standalone dashboard, delete migration.rs, rewire search_matrix + discovery/pid excision, finalize the in-transition CLAUDE.md/AGENTS.md daemon prose).
- NEVER auto-merge — human merge gate.
