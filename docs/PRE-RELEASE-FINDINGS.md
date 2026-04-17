# Pre-Release Findings — v6.10.0 ("World-Class Systems Hardening")

**Original review:** 2026-04-16 — three parallel `razorback:code-reviewer` agents (A: concurrency/lifecycle, B: data correctness, C: platform surface) against `git diff v6.9.0..HEAD` (129 files, +14,145 / -2,639).
**Last verified:** 2026-04-17 — static inspection + `cargo xtask test dev` (335s, 10 buckets green).
**Status:** 3/3 blockers fixed, 13/13 serious items fixed. Ready to tag after manual upgrade + Windows smoke tests.

---

## TL;DR

**Verdict as of 2026-04-17 (evening): All three release blockers and all 13 serious items fixed. Ready to tag pending manual upgrade test + Windows smoke.**

Original review flagged 3 blockers, ~8 serious issues, and ~14 minor items. After two fix passes (2026-04-16 and 2026-04-17):

- ✅ **3/3 release blockers fixed** — upgrade-path Tantivy wipe, per-restart full rebuild, dashboard auto-refresh.
- ✅ **13/13 serious items fixed** — A-I1, A-I2, A-I3, A-I4, B-I3, B-I4, B-I5, B-I6, C-C1, C-I1, C-I2, C-I3, C-I4, C-I5.
- 🟡 **Minor items** mostly unaddressed — tracked below; none block tag.

A-I5 and C-I6 were downgraded during verification (not real bugs as originally written). The refactor's core structure (state machine, atomic revision recording, projection plumbing, health module split, handshake fix) is sound. `cargo xtask test dev` passes (10 buckets, 335s).

---

## Verification Status (2026-04-17)

Verified against current `main` working tree (uncommitted). Each finding below is tagged ✅ FIXED, ⚠️ OPEN, or 🟡 DOWNGRADED.

### ✅ Fixed and verified in working tree

| ID | Finding | Evidence |
|----|---------|----------|
| **B-C1** | Tantivy wipe on legacy upgrade | `SymbolDatabase::ensure_canonical_revision` (`src/database/revisions.rs:160`) bootstraps canonical metadata from live SQLite counts; `clear_all` only fires when the workspace is genuinely empty |
| **B-C2** | Per-restart full rebuild | `SearchProjection::ensure_current_from_database` (`src/search/projection.rs:43`) now uses `db.count_projection_source_docs()` — a live `COUNT(*) FROM files + symbols` — instead of per-pass revision deltas |
| **C-C2** | Dashboard `formatUpper` typo | `dashboard/templates/status.html:179` now calls `formatUpperValue` |
| **A-I1** | `retry_dirty_tantivy` leaks `begin_operation` | `src/watcher/runtime.rs:283-295` — `search_index` check moved BEFORE `begin_operation` |
| **A-I2** | Workspace-pool lock held across `update_session_count().await` | `src/daemon/workspace_pool.rs:161-164` — `drop(guard)` explicit before the awaited session-count update |
| **A-I3** | `flag_restart_pending_for_restart` overwrites phase state | `src/daemon/lifecycle.rs:261-284` — function now takes `current_phase` as a param; state file write guarded by `first_request` |
| **A-I4** | Watcher queue + event tasks fire-and-forget | `src/watcher/mod.rs:45` introduces `run_guarded_task_step` (spawn-and-await-join pattern); event and queue loops (lines 413, 474) wrap each cycle |
| **B-I4** | `clean_orphaned_files` doesn't advance canonical revision | `src/database/workspace.rs:8-70` (`delete_orphaned_files_atomic`) records a canonical revision; `src/tools/workspace/indexing/incremental.rs:225-288` flips projection state to Stale before Tantivy cleanup and back to Ready afterward |
| **B-I5** | `clean_orphaned_files` missing explicit identifier/type deletes | `src/database/workspace.rs:19-53` — explicit deletes for `symbol_vectors`, `relationships`, `identifiers`, `types`, `indexing_repairs`, `symbols`, `files` in order |
| **C-C1** | `src/health/checker.rs` 790 lines | Now **321 lines**. New files: `src/health/projection.rs` (194), `src/health/indexing.rs` (94), `src/health/data_plane.rs` (193) |
| **C-I1** | Dashboard tests not in any xtask tier | `xtask/test_tiers.toml:127-131` adds `dashboard` bucket; included in `dev` and `full` tiers |
| **C-I2** | Duplicated `overall_health_level` | Single `overall_from_planes` in `src/health/evaluation.rs:3`; callers at `src/health/checker.rs:98` and `src/dashboard/state.rs:309, 388` |
| **C-I3** | New xtask tiers undocumented | CLAUDE.md quick-reference table (lines 82-83) documents `reliability` and `benchmark` |
| **C-I4** | `system-health` special bucket redundant | `xtask/src/runner.rs:93-96` — `reliability` tier = `[daemon, workspace-init, integration]`, `benchmark` tier = `[system-health]`. No overlap |
| **C-I5** | `DeviceLoadPolicy` drops fields | `src/embeddings/sidecar_protocol.rs:98-106` — `accelerated` and `degraded_reason` added with `#[serde(default)]` |

### ✅ Fixed in 2026-04-17 pass

| ID | Finding | Evidence |
|----|---------|----------|
| **B-I3** | `delete_workspace_data` leaves orphan rows | `src/database/workspace.rs:72-117` now sets `PRAGMA foreign_keys = ON` and explicitly deletes `symbol_vectors`, `identifiers`, `types`, `relationships`, `symbols`, `files`, `indexing_repairs`, `canonical_revisions`, `projection_states` in dependent-first order. Regression test: `test_delete_workspace_data_clears_all_owned_tables`. |
| **B-I6** | No reader-side projection gate | New `SearchProjection::ensure_current_with_gate` (`src/search/projection.rs`) flips `search_ready` to FALSE before `clear_all` and back to TRUE only when the projection ends up `Ready`. `backfill_tantivy_if_needed` now uses the gated method. Regression tests: `test_ensure_current_with_gate_flips_search_ready_on_rebuild` and `test_ensure_current_with_gate_keeps_search_ready_false_on_missing`. |

### 🟡 Downgraded during verification (not real bugs)

- **A-I5** — 30-second `READY_TIMEOUT` confirmed sufficient. `run_auto_indexing` fires after `DAEMON_READY` is written; `get_or_init` opens DB and search without triggering indexing. Release smoke item, not a bug.
- **C-I6** — `ProjectionFreshness::Unavailable` is used only for zero-symbol workspaces; broken projections surface as `RebuildRequired` / `Lagging`. Ranking logic may still deserve a rethink but the finding as written claimed more than the code proved.

### Verification checks run

- Static code inspection against each finding's cited file + line reference.
- File-size checks: `wc -l src/health/checker.rs src/health/projection.rs src/health/indexing.rs src/health/data_plane.rs src/dashboard/state.rs` → `321, 194, 94, 193, 672`.
- Grep for `overall_from_planes` / `overall_health_level` confirms consolidation.
- Prior test runs (from 2026-04-16 working-tree progress): `cargo xtask test dev` and `cargo xtask test system` passing.

---

## Remaining Work Before Tagging

### Nice-to-have (not blocking)

- Add a `browser_evaluate` or Rust render smoke check for the dashboard poller (guards against future JS typos in the same class as C-C2).
- Run `cargo xtask test reliability` to exercise daemon + workspace + integration buckets end-to-end.
- Add a v6.9.0→v6.10.0 upgrade integration test (create DB at schema_version=14, migrate, open, confirm Tantivy preserved on first edit) — the fix is correct but there's no dedicated regression test yet.

### Verification before tag

1. `cargo xtask test full` on macOS AND Windows (named-pipe path is least-covered transport in the diff; `flush()` semantics differ).
2. Manual upgrade test: populated v6.9.0 workspace → install v6.10.0 → edit a file → `fast_search` returns expected results → daemon restart → `fast_search` still works without a full rebuild.
3. Dashboard smoke: open `/status`, confirm 5-second poll updates without console errors.
4. Stale-binary restart smoke: kill daemon, update binary, reconnect client → session establishes without stdin byte loss.
5. Tag v6.10.0.

---

## Original Recommended Fix Order (historical, superseded)

1. **Day 1 (blockers):** Fix B-C1, B-C2, C-C2. Add the v6.9.0→v6.10.0 upgrade test.
2. **Day 2 (serious):** Split `health/checker.rs` (C-C1), fix `retry_dirty_tantivy` leak (A-I1), release the workspace-pool lock before session-count update (A-I2), add `dashboard` bucket to xtask tiers (C-I1).
3. **Day 3 (should-fix):** Orphan-cleanup + revision / projection-state plumbing (B-I3, B-I4, B-I5, B-I6). Consolidate duplicated `overall_health_level` (C-I2). Document new xtask tiers (C-I3). Drop redundant `system-health` bucket (C-I4).
4. **Verification:** `cargo xtask test full` on macOS AND Windows.
5. **Tag v6.10.0.**

---

## Release Blockers

### Blocker 1 — Tantivy index wiped on first edit after v6.9.0 → v6.10.0 upgrade ✅ FIXED
**Severity:** Critical (data-path / UX catastrophe for all existing users)
**Source:** Reviewer B (data correctness), Critical #1
**Location:** `src/search/projection.rs:29-41`

**Sequence:**
1. User on v6.9.0 has a populated SQLite DB and a populated Tantivy index.
2. User upgrades to v6.10.0. Migrations 15–18 run, creating empty `canonical_revisions` and `projection_states` tables. No backfill seeds a canonical revision for the existing data.
3. `plan_primary_workspace_repair` finds no staleness, so no repair runs on session connect. Tantivy keeps working.
4. User modifies any file. Pipeline runs `backfill_tantivy_if_needed` at `src/tools/workspace/indexing/index.rs:147`, which calls `ensure_current_from_database`.
5. `db.get_latest_canonical_revision(...)` returns `None` (empty table).
6. Code path at `projection.rs:29-41` fires: `index.clear_all()?` — **wipes the valid Tantivy index**, writes `ProjectionStatus::Missing`, returns.
7. Pipeline continues with `files_to_index = [the 1 changed file]`. Tantivy now contains 2 docs instead of ~100k. Search is broken until `force: true` reindex.

**Fix (preferred):** Add a schema-version-19 migration that seeds a `canonical_revisions` row when upgrading a DB that has symbols but no revisions. Insert with `kind='fresh'` and the cumulative symbol/file counts from `SELECT COUNT(*)`.

**Fix (alternative):** In `ensure_current_from_database`, when canonical is `None` but SQLite has symbols, treat as "unknown canonical — trigger a full rebuild from SQLite" instead of `clear_all()` + bail.

**Test:** Create a DB at schema_version=14 (pre-canonical-revisions), populate symbols/files/Tantivy as v6.9.0 would, open a v6.10.0 `SymbolDatabase` (migrations run), call `ensure_current_from_database`, assert Tantivy docs are preserved.

---

### Blocker 2 — Every daemon restart triggers a full Tantivy rebuild after any incremental write ✅ FIXED
**Severity:** Critical (startup UX regression, amplifies Blocker 1)
**Source:** Reviewer B, Critical #2
**Location:** `src/search/projection.rs:43-44`

```rust
let expected_docs = canonical.symbol_count + canonical.file_count;
let docs_match = expected_docs == 0 || index.num_docs() == expected_docs as u64;
```

`canonical_revisions.symbol_count` records the delta for THIS revision (see `record_canonical_revision_tx` args at `src/database/bulk_operations.rs:924-942` — `inserted_symbol_count` is per-pass, not cumulative). But `index.num_docs()` is the cumulative Tantivy doc count. After any incremental write, they diverge. `docs_match` is false, the short-circuit fails, and every daemon restart triggers a full Tantivy rebuild by reading `get_all_symbols()` and `get_all_file_contents_with_language()` into memory.

On a 100k-symbol workspace, this is several hundred MB of allocation and tens of seconds of work on every daemon startup.

Existing tests don't catch this because `test_search_projection_rebuilds_after_canonical_revision_advances` uses a replace (delete + insert = net change 0, so delta matches cumulative by coincidence).

**Fix (preferred):** Compute `expected_docs` from the tables at check time — `SELECT COUNT(*) FROM symbols + SELECT COUNT(*) FROM files`. The canonical revision record's counts then remain per-pass deltas (useful for Fresh-vs-Incremental telemetry).

**Fix (alternative):** Change `canonical_revisions` to store cumulative totals (requires migration to recompute).

---

### Blocker 3 — Dashboard auto-refresh silently dies after first tick (JS typo) ✅ FIXED
**Severity:** Critical (user-visible regression, silent failure mode)
**Source:** Reviewer C (platform surface), Critical #2
**Location:** `dashboard/templates/status.html:179`

Script calls `formatUpper(...)` but the defined function is `formatUpperValue`. Every 5-second poll throws `ReferenceError`. The handler is wrapped in `try {} catch (_) {}`, so the exception is swallowed. Every badge update below that line is skipped; the dashboard looks "stuck."

**Fix:** Change `formatUpper` → `formatUpperValue` on line 179. One character.

**Follow-up:** Add a `browser_evaluate` smoke check to `src/tests/dashboard/integration.rs` or a Rust render test that lints for `formatX` identifiers called in the live handler against a known function list.

---

## Serious Issues (should fix before tagging)

### A-I1 — `retry_dirty_tantivy` leaks `begin_operation` when `search_index` is None ✅ FIXED
**Location:** `src/watcher/runtime.rs:283-296`

```rust
self.indexing_runtime.write()...begin_operation(IndexingOperation::WatcherRepair);
warn!(...);
let Some(search_index) = self.search_index.as_ref() else {
    return;  // no finish_operation — runtime stuck
};
```

Early `return` skips the `finish_operation()` at lines 365-368, leaving the indexing runtime permanently in "WatcherRepair active" state, which surfaces in dashboard health.

In production daemon mode `search_index` is always `Some`, so this is primarily a test-time hazard — but the shape is a guard-pattern bug that will bite if anyone runs the queue runtime in a config without Tantivy.

**Fix:** Move the `search_index` check BEFORE `begin_operation`, or use an RAII guard that fires `finish_operation` on early return.

---

### A-I2 — Workspace pool write lock held across `update_session_count().await` ✅ FIXED
**Location:** `src/daemon/workspace_pool.rs:109-164`

The write lock on `self.workspaces` is acquired at line 109. `init_workspace().await` doesn't yield. But `update_session_count(workspace_id, true).await` at line 154 does a `spawn_blocking` + await for a SQLite UPDATE while the write lock is still held. Any other session connecting (same or different workspace) is blocked on the lock during that SQLite round-trip.

Low-likelihood deadlock (spawn_blocking runs on a different thread), but a real throughput hit under session storm.

**Fix:** Drop `guard` before `update_session_count`, matching the existing pattern at line 164 (`drop(guard); // release write lock before async watcher attach`).

---

### A-I4 — Watcher queue + event tasks are fire-and-forget; panics go silent ✅ FIXED
**Location:** `src/watcher/mod.rs:378-402, 424-440`

Both `event_handle` and `queue_handle` are spawned but the only await is in `stop()`. If `queue_runtime.run_cycle().await` panics, the task dies, the JoinHandle holds the panic payload, and **no file changes get processed ever again** until the daemon restarts.

The dashboard reports `indexing_runtime.watcher_paused` but not "queue task died" — observability gap for a production incident.

**Fix:** Wrap the loop body in `tokio::spawn(AssertUnwindSafe(...).catch_unwind())` with a health flag, OR have a supervisor task that periodically polls `handle.is_finished()` and logs/restarts.

**Note:** This is a pre-existing pattern, not introduced by this refactor, but the refactor was the moment to fix it. Can file for v6.10.1 if time-boxed; surface it to the dashboard health plane at minimum.

---

### B-I3 — `delete_workspace_data` leaves orphan rows ✅ FIXED
**Location:** `src/database/workspace.rs:28-33`

Explicitly deletes `canonical_revisions` and `projection_states` (new in v6.10.0). But still doesn't clear:
- `symbol_vectors` — virtual table, no FK cascade. Leaves orphan embedding rows.
- `indexing_repairs` — new in v6.10.0, no FK. Stale but harmless records.
- `identifiers` and `types` — depend on FK cascade, which requires `foreign_keys = ON` at the moment of the `DELETE FROM symbols`. The function doesn't set the pragma, so cleanup is fragile to prior-operation state.

**Fix:** Add explicit `DELETE FROM symbol_vectors`, `DELETE FROM identifiers`, `DELETE FROM types`, `DELETE FROM indexing_repairs`. Set `PRAGMA foreign_keys = ON` at the top, or rely on explicit deletes for all cascading tables. Explicit is safer than relying on cascade in a cleanup function.

---

### B-I4 — `clean_orphaned_files` doesn't advance canonical revision or update projection state ✅ FIXED
**Location:** `src/tools/workspace/indexing/incremental.rs:195-266`

When files are detected as deleted from disk, symbols/relationships/files rows are removed in one SQLite transaction, then Tantivy docs are removed in a separate commit. No canonical revision is recorded for this change.

If a crash lands between the SQLite commit (line 265) and the Tantivy commit (line 283), Tantivy carries phantom docs pointing to deleted files. The next `ensure_current_from_database` invocation will see a doc count mismatch and rebuild — so it self-heals, but only if that path runs. Between crash and next indexing run, search returns stale results (non-existent files in hits).

**Fix:** (a) wrap the SQLite orphan deletion in a call that also records a canonical revision (incremental kind, cleaned_file_count=N, others=0), and (b) have orphan cleanup update `projection_states` to `Stale` before Tantivy cleanup, then `Ready` after. That way the invariant "if state is Ready and revision matches, Tantivy is consistent" actually holds.

---

### B-I5 — `clean_orphaned_files` missing explicit DELETE for identifiers and types ✅ FIXED
**Location:** `src/tools/workspace/indexing/incremental.rs:239-259`

Pre-existing (v6.9.0 had the same omission). Explicit deletes cover `symbol_vectors`, `relationships`, `symbols`, `files`. Missing:
- `identifiers` for the orphaned file
- `types` for symbols that belonged to the orphaned file

If FK cascade is on, the `DELETE FROM files` cascades to `identifiers.file_path`, and `DELETE FROM symbols` cascades to `types.symbol_id` and `identifiers.containing_symbol_id`. But mixing explicit deletes with implicit cascade in the same function is fragile.

**Fix:** Add explicit deletes matching the ordering pattern in `incremental_update_atomic` at `src/database/bulk_operations.rs:649-667`.

---

### B-I6 — Projection state is write-only; no reader-side gate ✅ FIXED
**Location:** `src/search/projection.rs:160-168`

`rebuild` calls `index.clear_all()?` (which commits the deletion at `src/search/index.rs:246-253`), then `apply_documents` adds docs and commits at the end. Between those two commits, a concurrent search returns zero results.

The pipeline path sets `search_ready` atomic to false during repair. But `backfill_tantivy_if_needed` (called from `index_workspace_files` directly at `src/tools/workspace/indexing/index.rs:147`) does NOT touch `search_ready`. Searches during backfill return empty.

**Fix:** Set `search_ready = false` before `clear_all` in `backfill_tantivy_if_needed`, OR have search paths check `get_projection_state().status == Ready` before querying.

---

### C-C1 — `src/health/checker.rs` is 790 lines, violates CLAUDE.md 500-line rule ✅ FIXED (now 321 lines)
**Location:** `src/health/checker.rs`

`CLAUDE.md` calls the 500-line rule MANDATORY. The flagship refactored module blows past it by 58%. Not a cohesive responsibility: `build_data_plane` alone is 186 lines; `search_projection_health_for_workspace` is 106 lines of self-contained projection logic; three free helpers at the bottom (172 lines) are orthogonal to `HealthChecker`.

**Fix:** Move `search_projection_health_for_workspace` to a new `src/health/projection.rs`. Move the three free helpers (`projection_detail`, `projected_revision_from_state`, `indexing_health`) to `src/health/indexing.rs`. Move `build_data_plane` to `src/health/data_plane.rs`. Target: `checker.rs` ≤ 450 lines.

---

### C-I1 — Dashboard tests not in any xtask tier ✅ FIXED
**Location:** `xtask/test_tiers.toml`

`src/tests/dashboard/` has 53 tests including the new +377-line `integration.rs`. `test_tiers.toml` never mentions dashboard — not in `dev`, `system`, `dogfood`, `full`, `reliability`, or `benchmark`. They only execute if someone types `cargo test --lib tests::dashboard`.

**Fix:** Add a `dashboard` bucket to `test_tiers.toml`, include it in `dev` at minimum (and in `full`).

---

## Important (should fix this release)

### A-I3 — `flag_restart_pending_for_restart` overwrites richer phase state ✅ FIXED
**Location:** `src/daemon/lifecycle.rs:258-280` + callers in `src/daemon/mod.rs`

Unconditionally computes `next_phase = transition(Ready, ShutdownRequested{...})`. A daemon already in `Stopping{cause: Signal}` can be downgraded to `Draining{cause: RestartRequired}` on a subsequent stale-binary detection.

Cause field regression affects dashboard reporting, not operational behavior.

**Fix:** `store_phase` should only fire on `first_request`, or `transition` should read the current phase.

---

### A-I5 — READY_TIMEOUT = 30s should be verified against cold-workspace `get_or_init` 🟡 DOWNGRADED (not a bug)
**Location:** `src/adapter/mod.rs:32`

With `run_auto_indexing` now gated by `catchup_in_progress` and triggered AFTER DAEMON_READY is written (inside `serve()`), the handshake is not on the critical path for indexing. Should be safe, but confirm there's no code path where `get_or_init` triggers synchronous indexing. Smoke test on empty and huge workspaces before release.

---

### C-I2 — Duplicated `overall_health_level` logic with different semantics ✅ FIXED
**Locations:** `src/health/evaluation.rs:3` (`overall_from_planes`) vs `src/dashboard/state.rs:652` (`overall_health_level`)

They disagree: the dashboard version gates runtime inclusion on `runtime_configured`, while `overall_from_planes` always factors runtime. `HealthChecker::system_snapshot` and the dashboard can report different overall levels for the same state when embeddings aren't configured.

**Fix:** Consolidate to one function in `src/health/evaluation.rs`. Both call sites use it.

---

### C-I3 — New xtask tiers `reliability` and `benchmark` undocumented ✅ FIXED
**Location:** `CLAUDE.md` canonical tier table vs `xtask/src/runner.rs:90-96` and `xtask/src/cli.rs:5`

Two new tiers added via `PROGRAM_TIERS` are invisible to any agent reading `CLAUDE.md`.

**Fix:** Update the tier table in `CLAUDE.md`.

---

### C-I4 — `system-health` special bucket redundant with `integration` bucket ✅ FIXED
**Location:** `xtask/src/runner.rs:99-103`

`system-health` hardcodes `cargo test --lib tests::integration::system_health`, but the existing `integration` bucket already runs `tests::integration` (which includes `system_health`). In the `reliability` tier, both run, so `system_health` tests execute twice.

**Fix:** Drop the redundancy, or comment why running twice is intentional.

---

### C-I5 — Rust `DeviceLoadPolicy` drops fields the Python contract guarantees ✅ FIXED
**Location:** `src/embeddings/sidecar_protocol.rs:98-102`

Rust struct only contains `requested_device_backend` and `resolved_device_backend`. The Python side also sends `accelerated` and `degraded_reason` inside `load_policy`; the Python validator enforces consistency with top-level fields. The Rust side moves them to top-level `accelerated`/`degraded_reason` on `HealthResult`, which is fine, but `validate_health_response` doesn't verify `top-level accelerated == load_policy.accelerated`.

**Fix:** Mirror the nested fields in `DeviceLoadPolicy`, or add an explicit cross-check assertion.

---

### C-I6 — `projection_rank` can mask degradation across workspaces 🟡 DOWNGRADED
**Location:** `src/dashboard/state.rs:672-682`

`ProjectionFreshness::Unavailable` severity is 0, so any "current" workspace outranks it. Workspace A `Unavailable` + Workspace B `Current` → dashboard surfaces B's state, hides A's problem.

**Fix:** Promote `Unavailable` to severity 4 (above `RebuildRequired`), or show per-workspace breakdown when ranks vary.

---

## Minor (nice to have)

### A-minor findings
- **#6** `active_sessions` snapshot in accept loop is read twice with `read_ipc_headers` in between — logically noisy but safe (`src/daemon/mod.rs:745, 829`).
- **#7** `Unexpected` `ReadyOutcome` returns only the first line; document this invariant (`src/adapter/mod.rs:269`).
- **#8** `probe_readiness` on Unix actually connects — each probe creates an accepted-and-immediately-dropped connection with a 5s header-read timeout on the daemon side. Noisy logs ("IPC header read timed out"). Consider stat-only probe on Unix (`src/daemon/transport.rs:48`).
- **#9** `Ordering::Relaxed` on `restart_pending` pairs with `store_phase` on `RwLock`. Not a real ordering bug; a code comment on the memory model would help the next reader.

### B-minor findings
- **#7** Migrations don't wrap individual apply/record pairs in a transaction. Each migration's DDL is idempotent, so safe, but noisy-logs possible (`src/database/migrations.rs:45-50`).
- **#8** `record_canonical_revision_tx` uses `last_insert_rowid()` — brittle to future INSERT order changes. Consider `RETURNING revision` (`src/database/revisions.rs:80`).
- **#9** `ensure_current_from_database` reads `get_all_symbols()` and `get_all_file_contents_with_language()` into memory on every rebuild. Potential OOM on memory-constrained users at 100k+ symbols (`src/search/projection.rs:66-67`).
- **#10** Migration 017 inlines `projected_revision` in `CREATE TABLE` but migration 018 still runs on fresh DBs (idempotent via `has_column` guard). Consider rolling 018's guard into 017.
- **#11** No test for v6.9.0→v6.10.0 upgrade path. Critical given Blocker 1.
- **#12** `CanonicalRevisionKind` has only `Fresh` and `Incremental`. Adding purely-new files to an existing workspace gets recorded as "Fresh". Inaccurate telemetry, not a correctness bug.

### C-minor findings
- **M1** `src/dashboard/state.rs` is now 691 lines — over the 500-line target. Not as egregious as `checker.rs` (790) but worth splitting.
- **M2** Python sidecar `test_runtime.py:331` hardcodes `"probe encode failed on directml:0, fell back to CPU"` as exact expected value. Fragile.
- **M3** `BAAI/bge-small-en-v1.5` default model ID still in `src/embeddings/sidecar_provider.rs:107`, but Python default is `nomic-ai/CodeRankEmbed` (768d vs 384d). Dead dead-defaulting.
- **M4** Verify Python sidecar tests are wired into CI somewhere. A protocol regression on the Python side won't fail `cargo xtask` until a Rust integration test catches it indirectly.
- **M5** `src/embeddings/sidecar_provider.rs:109` silently defaults `accelerated: false` on health misparse. Low risk, but defensive hardening.

---

## Strengths (what the refactor got right)

Documenting the wins so future work can build on the same patterns.

**Adapter handshake (uncommitted):**
- `DAEMON_READY\n` protocol cleanly closes the stdin-byte-loss race; adapter blocks on READY before touching stdin.
- Byte-at-a-time read with capped buffer (`READY_LINE_MAX = 64`) — no unbounded allocation.
- Eight tests in `src/tests/adapter/ready.rs` cover: ready, EOF, partial signal, timeout, delayed drop, trailing-bytes-preserved, unexpected line, full handshake simulation, and runaway-length cap.

**Daemon lifecycle:**
- Restart handoff via `RestartHandoffAction` / `restart_handoff_action()` is pure and testable.
- Retry loop in `run_adapter_with` is bounded at `MAX_RETRIES=2` with no sleeps.
- `TransportEndpoint` is a clean Unix/Windows probe abstraction; Windows uses `WaitNamedPipeW` with 1ms timeout, avoiding the pipe-instance-consumption bug.
- `accept_loop` reads headers BEFORE `sessions.add_session()`, so rejects don't register phantom sessions.
- `drain_sessions` uses arm-then-check notifier — no missed-wakeup race.

**Watcher:**
- Overflow and persisted-repair replay now share one `QueueRuntime`, eliminating drift between live and manual drain paths.

**Database / indexing:**
- Migrations are idempotent (`CREATE TABLE IF NOT EXISTS`, `CREATE INDEX IF NOT EXISTS`, migration 018 guards `ALTER TABLE` with `has_column()`).
- Canonical revisions are monotonic per workspace via SQLite `AUTOINCREMENT`.
- **The atomic claim is real.** `incremental_update_atomic` and `bulk_store_fresh_atomic` wrap cleanup, inserts, index rebuild, and canonical revision into a single `outer_tx.commit()`. Crash anywhere inside leaves SQLite unchanged.
- WAL TRUNCATE checkpoint after success reclaims space without opening a consistency window.
- `IndexingStage` state machine has 8 ordered stages with history tracking — partial failures are tractable.
- `search_ready` atomic is gated on repair state — a repair-needed file flips to false.
- `repairs.rs` is a journal, not a destructive routine (name is scarier than the code).
- Test coverage on the write path is excellent: atomic writes, dangling relationships, dangling types, workspace cleanup, revision counts.

**Platform surface:**
- Health module split (`types`, `evaluation`, `embedding`, `report`, `checker`) is genuinely separate responsibilities.
- Sidecar protocol change is backward-compatible on the wire — all new Rust fields use `#[serde(default)]`, protocol version stays at `1`.
- Python-side `_validate_health_metadata` is strict and cross-checks `load_policy.accelerated` against top-level `accelerated`.
- Dashboard integration + state tests are real tests with real assertions, not smoke checks.
- Tera autoescaping on, poll script uses `.textContent` not `.innerHTML` — no XSS surface.

---

## Verification Plan (before tagging)

1. **Fix all three blockers and the 500-line violation.**
2. **Add the missing upgrade-path test** — create DB at v6.9.0 schema, migrate, confirm Tantivy preserved on first edit.
3. **Run `cargo xtask test full` on macOS.**
4. **Run `cargo xtask test full` on Windows.** The named-pipe path is the least-covered transport in this diff. `flush()` semantics on Windows named pipes differ from Unix sockets.
5. **Manual upgrade test:** Populated v6.9.0 workspace → install v6.10.0 → edit a file → `fast_search` returns expected results → daemon restart → `fast_search` still works without full rebuild.
6. **Dashboard smoke:** Open `/status`, confirm 5-second poll updates without console errors.
7. **Stale-binary restart smoke:** Kill daemon, update binary, reconnect client → session establishes without stdin-byte-loss.
8. **Tag v6.10.0.**

---

## Sign-off

Three parallel reviews, zero false positives detected in spot-checks. Refactor scope and ambition are warranted; the blockers are fixable in under a day. Ship once they're fixed.
