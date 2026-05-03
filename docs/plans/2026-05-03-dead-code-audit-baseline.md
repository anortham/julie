# Dead Code Audit Baseline

**Workspace:** `dead-code-audit-cleanup_4f5b083c`
**Path:** `/Users/murphy/source/julie/.worktrees/dead-code-audit-cleanup`
**Commit:** `84b83c06`
**Captured:** `2026-05-03T19:33:10Z`

## Index Health

`manage_workspace(operation="health", detailed=true)` reported `READY`.

- SQLite: `24198` symbols across `1260` files
- Relationships: `32578`
- Projection status: `READY`
- Projection freshness: `CURRENT`
- Canonical revision: `1`
- Projected revision: `1`
- Projection lag: `0`
- Repair needed: `false`

`manage_workspace(operation="stats")` reported the same workspace as `ready`.

## Evidence Commands

```bash
python3 .claude/skills/dead-code-audit/scripts/dead_code_inventory.py --workspace-id dead-code-audit-cleanup_4f5b083c --limit 80
cargo build
./target/debug/julie-server signals --workspace . --fresh --limit 80
python3 .claude/skills/dead-code-audit/scripts/dead_code_inventory.py --workspace-id dead-code-audit-cleanup_4f5b083c --limit 600 --json
```

The inventory command is candidate evidence only. It is not deletion proof.

## Inventory Summary

| Section | Count |
| --- | ---: |
| Test-only relationship refs | 201 |
| Zero relationship refs and no production identifier hits | 536 |
| Likely graph gaps | 503 |
| Cfg-test markers in source paths | 79 |
| Symbols | 24198 |
| Relationships | 32578 |
| Identifiers | 120906 |

## Early Warning Signals

`julie-server signals` reported fixture-only C# entry points and many centrality gaps.

- Entry points: 4, all in `fixtures/real-world/csharp-advanced/DevOpsController.cs`
- Auth coverage candidates: 4, same fixture paths
- Entry point linkage gaps: 4, same fixture paths
- Centrality gaps: 3056

These are useful graph-quality leads, not cleanup targets for this plan.

## Architecture-Stage Candidate Snapshot

| Stage | Candidate | Inventory Section | Initial Label | Notes |
| --- | --- | --- | --- | --- |
| Projection | `SearchProjection::project_documents` at `src/search/projection.rs:181` | Test-only relationship refs | `test-fossil` | Duplicates `project_documents_with_locks`; only tests call the non-lock variant. Needs projection test rewrite before deletion. |
| Projection | `projection_served_revision` at `src/search/projection.rs:377` | Zero relationship refs | `keep` | Inventory missed unqualified same-file calls. `rg` shows three production uses in `ensure_current_inner` and projection paths. |
| Projection | `apply_uncommitted_documents_from_symbols` at `src/search/projection/apply.rs:22` | Likely graph gaps | `keep` | Product watcher paths call this for dirty Tantivy repair. |
| Workspace runtime | `WorkspacePool::mark_indexed` at `src/daemon/workspace_pool.rs:172` | Test-only relationship refs | `delete` | Only `test_mark_indexed` calls it. The in-memory `indexed` flag has no product reader after the workspace service split. |
| Workspace runtime | `WorkspacePool::is_indexed` at `src/daemon/workspace_pool.rs:164` | Likely graph gap | `delete` | `rg` shows only workspace-pool tests call this method. |
| Workspace runtime | `WorkspacePool::sync_indexed_from_db` at `src/daemon/workspace_pool.rs:189` | Not directly listed | `delete` | Product call sites exist, but the only side effect is setting the now-unused `WorkspaceEntry.indexed` flag. |
| Workspace runtime | `WorkspacePool::indexing_snapshot` at `src/daemon/workspace_pool.rs:211` | Likely graph gap | `keep` | Product cleanup and dashboard code use this snapshot path. |
| Workspace runtime | `WorkspacePool::mark_indexed` test at `src/tests/daemon/workspace_pool.rs:125` | Test-only fossil | `delete` | Test preserves the stale flag behavior rather than product behavior. |
| Indexing | `IndexingBatchState::parsed_file_count` at `src/tools/workspace/indexing/state.rs:335` | Cfg-test marker | `keep` | Integration tests assert meaningful parse/text/repair counts. |
| Indexing | `IndexingBatchState::text_only_file_count` at `src/tools/workspace/indexing/state.rs:340` | Cfg-test marker | `keep` | Same as above. |
| Indexing | `IndexingBatchState::repair_file_count` at `src/tools/workspace/indexing/state.rs:344` | Zero relationship refs | `keep` | Production logs use this count in indexing pipeline and index command. |
| Lifecycle | `flag_restart_pending_for_restart` at `src/daemon/lifecycle.rs:346` | Test-only relationship refs | `needs-design-review` | Lifecycle helper is test-only, but restart transition coverage is high-risk. Leave for a targeted lifecycle cleanup. |
| Watcher | `WatcherPool::increment_ref` at `src/daemon/watcher_pool.rs:57` | Test-only relationship refs | `graph-gap` | Product session attachment uses watcher ref changes through service paths; method-level graph misses this pattern. |
| Adapter/transport | `forward_streams` at `src/adapter/mod.rs:317` | Test-only relationship refs | `needs-design-review` | Defer until the HTTP transport plan has parity tests. |
| Adapter/transport | `ReadyOutcome`, `ForwardOutcome`, `BranchOutcome` at `src/adapter/mod.rs` | Zero relationship refs | `needs-design-review` | Defer with adapter transport candidates. |

## Verification Notes So Far

| Candidate | Evidence | Decision |
| --- | --- | --- |
| `projection_served_revision` | `fast_refs` showed definition only, but `rg` showed same-file production calls at `src/search/projection.rs:111`, `206`, and `293`. | `keep`, graph gap |
| `SearchProjection::project_documents` | `fast_refs` showed only two test callers. | Candidate for later deletion or test-only rename |
| `apply_uncommitted_documents_from_symbols` | `fast_refs` showed product calls from `src/watcher/handlers.rs` and `src/watcher/runtime.rs`. | `keep`, graph gap |
| `WorkspacePool::mark_indexed` | `fast_refs` showed only `test_mark_indexed`. | Candidate for deletion |
| `WorkspacePool::is_indexed` | `rg "\.is_indexed\(" src` showed only tests. | Candidate for deletion |
| `WorkspacePool::sync_indexed_from_db` | `fast_refs` showed product call sites, but all only set `WorkspaceEntry.indexed`, which has no product reader. | Candidate for deletion with call-site cleanup |
| `indexing_snapshot` | `rg` showed product use from cleanup and dashboard state. | `keep` |

## Current Cleanup Scope

The first safe cleanup batch is workspace-runtime only:

- remove `WorkspaceEntry.indexed`
- remove `WorkspacePool::is_indexed`
- remove `WorkspacePool::mark_indexed`
- remove `WorkspacePool::sync_indexed_from_db`
- remove product call sites that only synced the dead flag
- remove or rewrite tests that preserve that dead flag

Projection API cleanup is plausible but riskier because `SearchProjection::project_documents` is still named in the projection plan contract. Adapter cleanup is deferred until HTTP transport exists.

## Cleanup Ledger

| Candidate | Decision | Action | Evidence | Verification |
| --- | --- | --- | --- | --- |
| `WorkspaceEntry.indexed` | `delete` | Removed field | No product reader after workspace service split. | `cargo nextest run --lib tests::daemon::workspace_pool` passed |
| `WorkspacePool::is_indexed` | `delete` | Removed method and fossil tests | `rg "\.is_indexed\(" src` showed only tests. | `cargo nextest run --lib tests::daemon::workspace_pool` passed |
| `WorkspacePool::mark_indexed` | `delete` | Removed method and `test_mark_indexed` | `fast_refs` showed only `test_mark_indexed`. | `cargo nextest run --lib tests::daemon::workspace_pool` passed |
| `WorkspacePool::sync_indexed_from_db` | `delete` | Removed method and call sites | Product call sites only synced the removed in-memory flag. | `cargo nextest run --lib tests::daemon::workspace_pool` passed |
| `WorkspaceSessionAttachment::attach_workspace_once_and_sync_indexed` | `merge-into-caller` | Replaced with `attach_workspace_once` | Its only extra behavior was calling `sync_indexed_from_db`. | IPC and dashboard focused tests passed |
| `disconnect_dashboard_attached_workspaces` state parameter | `merge-into-caller` | Removed unused `AppState` parameter | State was only used to reach the removed pool sync. | Dashboard focused test passed |

## Verification Ledger

| Scope | Invariant | Command | Commit | Result | Time |
| --- | --- | --- | --- | --- | --- |
| worker-red-green | WorkspacePool still returns initialized runtime entries after removing indexed flag fossils. | `cargo nextest run --lib test_get_returns_some_after_init 2>&1 \| tail -40` | working-tree at `84b83c06` | PASS | 2026-05-03T19:40:00Z |
| worker-red-green | Dashboard open action still warms a workspace without leaking a session count after removing dashboard pool sync. | `cargo nextest run --lib test_projects_open_action_warms_workspace_without_leaking_session_count 2>&1 \| tail -40` | working-tree at `84b83c06` | PASS | 2026-05-03T19:40:00Z |
| worker-red-green | IPC cleanup still detaches startup and rebound primary workspaces after removing pool sync. | `cargo nextest run --lib test_handle_ipc_session_cleanup_disconnects_startup_and_rebound_primary 2>&1 \| tail -40` | working-tree at `84b83c06` | PASS | 2026-05-03T19:41:00Z |
| worker-red-green | WorkspacePool module behavior stays green after deleting stale indexed flag tests and methods. | `cargo nextest run --lib tests::daemon::workspace_pool 2>&1 \| tail -50` | working-tree at `84b83c06` | PASS, 16 tests | 2026-05-03T19:42:00Z |
| affected-change | Changed-file gate passes after workspace-runtime cleanup; runner fell back to dev because `src/handler.rs` is an exact-file fallback. | `cargo xtask test changed` | working-tree at `84b83c06` | PASS, 22 buckets in 375.6s | 2026-05-03T19:50:00Z |
