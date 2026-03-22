# Julie v6 Phase 2: Shared Workspaces, Persistent Registry & Metrics

## Summary

Phase 2 evolves the v6 daemon from "multiple sessions, one project" to "multiple sessions, shared workspaces with persistent state." A central `daemon.db` replaces the planned `registry.json` and becomes the daemon's long-term memory: workspace registry, codehealth snapshots, and tool call history. Shared file watchers eliminate duplicate OS watches. Reference workspaces attach instantly when already indexed.

## Problem

Phase 1 solved the immediate concurrency pain (LockBusy, VRAM duplication). But several gaps remain:

1. **Workspace state is ephemeral.** The `WorkspacePool` is in-memory only. Daemon restart means re-discovering all workspaces from scratch.
2. **Reference workspaces still duplicate.** `manage_workspace add` reindexes the reference project every time, even if another session already indexed it.
3. **File watchers multiply.** N sessions on the same project create N watchers on the same directory tree.
4. **Metrics disappear on reindex.** `tool_calls` and analysis scores live in `symbols.db`, which is replaced during a full reindex. Users can't track codehealth improvement over time.
5. **No transparency signal.** Users have no way to see that Julie is delivering value beyond trusting that searches work. Historical usage data (call counts, latency trends) could build that trust.

## Design

### daemon.db: The Daemon's Persistent State

A new SQLite database at `~/.julie/daemon.db`, opened by the daemon on startup with WAL mode for concurrent reads. Uses the same migration pattern as `symbols.db` (versioned schema, idempotent migrations). Initial schema version: `daemon_v1`. Migration code lives in `src/daemon/database.rs`.

The daemon holds a single `Arc<DaemonDatabase>` and passes it to `WorkspacePool` and each `JulieServerHandler`.

**Supersedes:** The Phase 1 spec described a `registry.json` for Phase 2. This is replaced by the `workspaces` table in `daemon.db`, which provides ACID guarantees, concurrent read access via WAL mode, and a natural home for metrics data.

**Replaces existing registry:** The current `WorkspaceRegistryService` (`src/workspace/registry_service.rs`) and its backing JSON file (`workspace_registry.json` in each project's `.julie/`) are deprecated. Their functionality (workspace tracking, orphan detection, TTL expiration, cleanup) moves into `DaemonDatabase`. The `WorkspaceRegistryService`, `WorkspaceRegistry`, `WorkspaceEntry`, `OrphanedIndex`, and related types in `src/workspace/registry.rs` are removed. The `generate_workspace_id()` function is retained (moved to `src/daemon/database.rs` or kept in a shared utility module).

**Corruption recovery:** If `daemon.db` is corrupted or deleted, the daemon can rebuild the `workspaces` table by scanning `~/.julie/indexes/` (directory names contain workspace IDs). Tool call history and codehealth snapshots would be lost, but workspace functionality recovers automatically.

#### Schema

**`workspaces`** (replaces planned `registry.json`):

```sql
CREATE TABLE workspaces (
    workspace_id    TEXT PRIMARY KEY,
    path            TEXT NOT NULL UNIQUE,
    status          TEXT NOT NULL DEFAULT 'pending',
    session_count   INTEGER NOT NULL DEFAULT 0,
    last_indexed    INTEGER,
    symbol_count    INTEGER,
    file_count      INTEGER,
    embedding_model TEXT,
    vector_count    INTEGER,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);
```

Status values: `pending` (known but not yet indexed), `indexing` (in progress), `ready` (indexed and available), `error` (indexing failed).

**`workspace_references`** (which workspaces reference which):

```sql
CREATE TABLE workspace_references (
    primary_workspace_id    TEXT NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
    reference_workspace_id  TEXT NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
    added_at                INTEGER NOT NULL,
    PRIMARY KEY (primary_workspace_id, reference_workspace_id)
);
```

Reference relationships are workspace-to-workspace, not session-to-session. When session A on project Julie adds LabHandbook as a reference, session B on project Julie gets it automatically.

**`codehealth_snapshots`** (auto-captured after each indexing pass):

```sql
CREATE TABLE codehealth_snapshots (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id      TEXT NOT NULL REFERENCES workspaces(workspace_id),
    timestamp         INTEGER NOT NULL,
    total_symbols     INTEGER NOT NULL,
    total_files       INTEGER NOT NULL,
    security_high     INTEGER NOT NULL DEFAULT 0,
    security_medium   INTEGER NOT NULL DEFAULT 0,
    security_low      INTEGER NOT NULL DEFAULT 0,
    change_high       INTEGER NOT NULL DEFAULT 0,
    change_medium     INTEGER NOT NULL DEFAULT 0,
    change_low        INTEGER NOT NULL DEFAULT 0,
    symbols_tested    INTEGER NOT NULL DEFAULT 0,
    symbols_untested  INTEGER NOT NULL DEFAULT 0,
    avg_centrality    REAL,
    max_centrality    REAL
);
CREATE INDEX idx_snapshots_workspace_time
    ON codehealth_snapshots(workspace_id, timestamp);
```

**`tool_calls`** (moved from per-workspace `symbols.db`):

```sql
CREATE TABLE tool_calls (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id  TEXT NOT NULL,
    session_id    TEXT NOT NULL,
    timestamp     INTEGER NOT NULL,
    tool_name     TEXT NOT NULL,
    duration_ms   REAL NOT NULL,
    result_count  INTEGER,
    source_bytes  INTEGER,
    output_bytes  INTEGER,
    success       INTEGER NOT NULL DEFAULT 1,
    metadata      TEXT
);
CREATE INDEX idx_tool_calls_timestamp ON tool_calls(timestamp);
CREATE INDEX idx_tool_calls_tool_name ON tool_calls(tool_name);
CREATE INDEX idx_tool_calls_session ON tool_calls(session_id);
CREATE INDEX idx_tool_calls_workspace ON tool_calls(workspace_id);
```

New `workspace_id` column enables cross-workspace queries ("show me all Julie usage this week").

**Retention policy:** Tool calls older than 90 days are pruned on daemon startup. A single `DELETE FROM tool_calls WHERE timestamp < ?` with the cutoff timestamp. Codehealth snapshots are not pruned (they're small and the historical trend is the whole point).

#### Lifecycle

- Daemon opens/creates `daemon.db` at startup in `run_daemon()`, right after PID file acquisition
- On startup: reset all `session_count` to 0 (`UPDATE workspaces SET session_count = 0`). This handles crash recovery: if the daemon died with nonzero counts, they're stale. Sessions re-increment as they reconnect.
- On startup: prune tool_calls older than 90 days
- `Arc<DaemonDatabase>` passed to `WorkspacePool` and each `JulieServerHandler`
- Per-workspace `symbols.db` retains: symbols, relationships, identifiers, types, files (index data that should be rebuilt on reindex)
- `daemon.db` retains: workspace registry, codehealth history, tool call history (data that must survive reindex)

### WorkspacePool Backed by daemon.db

The Phase 1 `WorkspacePool` gains a persistent backing store:

```rust
pub struct WorkspacePool {
    workspaces: RwLock<HashMap<String, Arc<JulieWorkspace>>>,
    daemon_db: Arc<DaemonDatabase>,
}
```

#### get_or_init Flow

1. **In-memory check** (fast path, unchanged from Phase 1)
2. **daemon.db check**: query `workspaces` table by workspace_id
   - Row exists, `status = "ready"`, index files present on disk: initialize `JulieWorkspace` from `~/.julie/indexes/{id}/`, add to in-memory map, increment `session_count`
   - Row exists but index files missing (crash recovery): set status to `"pending"`, fall through to fresh indexing
   - No row: insert with `status = "pending"`, initialize fresh workspace, index, update to `"ready"` when complete
3. **On session disconnect**: decrement `session_count` in `daemon.db`

Workspaces survive daemon restarts. On restart, the daemon reads the `workspaces` table and lazily rehydrates as sessions connect. No disk scanning needed.

### Shared Reference Workspaces

The core Phase 2 payoff. `manage_workspace(operation="add", path="/other/project")` flow:

1. Compute `ref_workspace_id` from path (existing `generate_workspace_id`)
2. Call `pool.get_or_init(ref_workspace_id, ref_path)`
3. If already in pool (another session/workspace indexed it): **instant attach**, zero reindexing
4. If not: index into `~/.julie/indexes/{ref_id}/`, register in daemon.db
5. Insert row into `workspace_references` (primary_workspace_id, reference_workspace_id)

When a new session connects to a primary workspace, it inherits all reference workspaces via the `workspace_references` table. No per-session re-adding needed.

### WatcherPool: Shared File Watchers

One file watcher per workspace, reference-counted across sessions.

```rust
pub struct WatcherPool {
    watchers: RwLock<HashMap<String, WatcherEntry>>,
}

struct WatcherEntry {
    watcher: Arc<IncrementalIndexer>,
    ref_count: usize,
    grace_deadline: Option<Instant>,
}
```

#### Lifecycle

- **Attach**: session connects to workspace (primary or reference) -> increment `ref_count`. Create watcher if it doesn't exist. Cancel grace deadline if one is set.
- **Detach**: session disconnects -> decrement `ref_count`. If it hits 0, set `grace_deadline` to `now + 5 minutes`.
- **Reaper**: background task checks every 60 seconds for entries past their grace deadline. Shuts down watcher and removes entry.
- **Reattach within grace**: new session before deadline -> reuse existing watcher, no gap in file watching.

#### Integration

- `WorkspacePool` owns `WatcherPool`
- `get_or_init` calls `watcher_pool.attach(workspace_id, workspace_path)` when creating/rehydrating
- Session disconnect calls `watcher_pool.detach(workspace_id)`
- `JulieWorkspace::watcher` field is `None` in daemon mode (the pool owns watchers, not the workspace)
- The watcher itself is unchanged: same `IncrementalIndexer`, same incremental reindexing logic
- **Manual refresh:** `manage_workspace(operation="refresh")` triggers re-indexing through the handler's indexing pipeline (unchanged). The `WatcherPool`'s `IncrementalIndexer` handles file-change-driven reindexing; manual refresh bypasses the watcher and calls the indexing pipeline directly, same as today.

### Codehealth Snapshots

#### Automatic Capture

After each indexing pass completes (at the end of `process_files_optimized`, after all analysis passes: centrality, test quality, test coverage, change risk, security risk). The handler holds `Option<Arc<DaemonDatabase>>` via `handler.daemon_db`. The indexing pipeline accesses it through the `&JulieServerHandler` reference already passed to `process_files_optimized`:

```rust
if let Some(daemon_db) = &handler.daemon_db {
    daemon_db.snapshot_codehealth(workspace_id, &symbols_db)?;
}
```

`snapshot_codehealth` runs aggregate queries against the freshly-indexed `symbols.db` (COUNT with GROUP BY on risk levels, etc.) and inserts one row into `codehealth_snapshots`. Estimated cost: ~5ms.

**Fresh indexes:** If analysis passes haven't run yet (e.g., extraction-only pass), all risk columns will be 0. This is correct and expected; the snapshot reflects the state at that point. The comparison will show "no previous data" for the first snapshot and meaningful deltas from the second onward.

#### Comparison in query_metrics

When `query_metrics(category="code_health")` runs, it computes the current stats from the live `symbols.db`, then queries the most recent `codehealth_snapshots` row for the workspace. If a previous snapshot exists, it appends a comparison (current live stats vs. last saved snapshot):

```
── Codehealth Trend ──────────────────────────────
Compared to last indexing (2026-03-20 14:30):
  Security HIGH:  14 → 9   (↓5, -36%)
  Untested:       47 → 31  (↓16, -34%)
  Avg centrality: 0.42 → 0.45  (↑0.03)
  Total symbols:  7306 → 7412  (+106)
──────────────────────────────────────────────────
```

#### New query_metrics Category

`category: "trend"` returns snapshot history for a workspace:

- Default limit: 10 most recent snapshots
- Shows trajectory without the full symbol-by-symbol risk table
- Useful for the `/codehealth` skill to give a quick progress summary

Example output:
```
── Codehealth Trend (last 5 snapshots) ───────────
Date          Symbols  SecHIGH  Untested  AvgCent
2026-03-22    7412     9        31        0.45
2026-03-20    7306     14       47        0.42
2026-03-18    7201     14       52        0.41
2026-03-16    7150     16       58        0.39
2026-03-14    7090     18       63        0.38
──────────────────────────────────────────────────
```

### Tool Calls Redirection

`record_tool_call` on the handler writes to `daemon.db` instead of per-workspace `symbols.db`:

- Adds `workspace_id` to each record (resolved from the handler's primary workspace)
- `query_metrics(category="history")` reads from `daemon.db`
- `query_metrics(category="session")` still reads from in-memory `SessionMetrics` (unchanged)
- Existing `tool_calls` rows in per-workspace `symbols.db` are **not migrated** (starting fresh is fine; old data is session-specific debug output)

### Non-Daemon Mode

No fallback for daemon.db features. `DaemonDatabase` is `Option<Arc<DaemonDatabase>>` on the handler:

- Daemon mode: `Some(daemon_db)`, all features active
- Stdio mode (dev/debug only): `None`, codehealth snapshots and persistent tool_calls silently skip
- `SessionMetrics` (in-memory) works in both modes
- Per-workspace `symbols.db` retains its `tool_calls` table (migration not removed), but daemon mode stops writing to it

### Index Migration Update

Phase 1's migration (copy from `{project}/.julie/indexes/` to `~/.julie/indexes/`) now also inserts a row into `daemon.db` workspaces table, so the registry knows about migrated indexes. The validation step (open SQLite, verify symbol count; open Tantivy, verify meta.json) is unchanged.

## Storage Layout

```
~/.julie/
+-- daemon.db                      # NEW: persistent daemon state
+-- daemon.sock                    # IPC endpoint
+-- daemon.lock                    # advisory file lock
+-- daemon.pid                     # PID file
+-- daemon.log                     # daemon lifecycle logs
+-- indexes/
|   +-- julie_a1b2c3d4/
|   |   +-- db/symbols.db         # symbols, relationships, types (index data)
|   |   +-- tantivy/
|   +-- labhandbook_e5f6g7h8/     # shared: referenced by julie + other projects
|   |   +-- db/symbols.db
|   |   +-- tantivy/
|   +-- sharedlib_cafe1234/
|       +-- db/symbols.db
|       +-- tantivy/

{project}/.julie/
+-- logs/julie.log.{date}         # project-scoped logs (unchanged)
+-- config.toml                    # optional per-project config (unchanged)
```

## Agent Team Structure

Three teammates, each owning distinct file sets:

| Teammate | Scope | Key New Files | Key Modified Files |
|----------|-------|---------------|-------------------|
| **Registry** | `DaemonDatabase`, workspaces table, workspace_references, migrations, daemon startup, deprecate `WorkspaceRegistryService` + `WorkspaceRegistry` | `src/daemon/database.rs` | `src/daemon/mod.rs`, `src/paths.rs`, `src/workspace/registry_service.rs` (remove), `src/workspace/registry.rs` (trim to keep `generate_workspace_id`) |
| **Watchers** | `WatcherPool`, ref-counting, grace period reaper, workspace watcher detachment | `src/daemon/watcher_pool.rs` | `src/workspace/mod.rs` |
| **Routing** | `manage_workspace` registry check, instant attach, codehealth snapshot, tool_calls redirection, query_metrics trend | `src/tools/metrics/trend.rs` | `src/tools/workspace/`, `src/handler.rs`, `src/tools/metrics/` |

Lead coordinates integration points in `src/daemon/mod.rs` and runs `cargo xtask test dev` after each batch.

## Dogfood Gate

Before proceeding to Phase 3:

1. Rebuild release, restart daemon
2. Verify multi-session stability (two Claude Code instances, same project, no LockBusy)
3. Add a reference workspace, verify instant attach on second add
4. Run `/codehealth`, verify trend comparison appears after reindex
5. Verify tool_calls survive a `manage_workspace(operation="refresh", force=true)` reindex
6. Verify watcher sharing: two sessions, one watcher (check daemon logs)
7. Kill daemon, restart, verify workspaces rehydrate from daemon.db without re-scanning

## Non-Goals

- Shared embedding pipeline (Phase 3)
- HTTP transport / Streamable HTTP (Phase 3+)
- Daemon-level aggregated metrics across sessions (future)
- Windows named pipe support (separate effort)
- Plugin distribution (Phase 4)
- Migration of existing tool_calls from per-workspace symbols.db

## References

- [v6 Phase 1 Design](2026-03-22-v6-daemon-adapter-architecture-design.md)
- [v6 Phase 1 Implementation Plan](../plans/2026-03-22-v6-phase1-daemon-adapter.md)
- [Operational Metrics Design](2026-03-19-operational-metrics-design.md)
