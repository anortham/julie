# v6 Phase 2: Shared Workspaces Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **IMPORTANT:** Use Julie's MCP tools (fast_search, deep_dive, get_symbols, fast_refs) for ALL code investigation. No grep, no guessing APIs.

**Goal:** Persistent workspace registry in `daemon.db`, shared reference workspaces with instant attach, reference-counted file watchers, and codehealth trend snapshots.

**Architecture:** A new `DaemonDatabase` wraps a SQLite connection to `~/.julie/daemon.db`. The `WorkspacePool` gains persistent backing. A `WatcherPool` manages reference-counted `IncrementalIndexer` instances. Codehealth snapshots are captured automatically after indexing and displayed as trend comparisons in `query_metrics`.

**Tech Stack:** Rust, SQLite (rusqlite), tokio async runtime, existing tree-sitter/Tantivy infrastructure.

**Spec:** `docs/superpowers/specs/2026-03-22-v6-phase2-shared-workspaces-design.md`

---

## File Map

### New Files

| File | Responsibility |
|------|---------------|
| `src/daemon/database.rs` | `DaemonDatabase` struct: open/migrate daemon.db, CRUD for workspaces/references/snapshots/tool_calls |
| `src/daemon/watcher_pool.rs` | `WatcherPool` struct: ref-counted `IncrementalIndexer` instances with grace period lifecycle |
| `src/tools/metrics/trend.rs` | Codehealth trend formatting: snapshot history table, before/after comparison |
| `src/tests/daemon/database.rs` | Tests for `DaemonDatabase` |
| `src/tests/daemon/watcher_pool.rs` | Tests for `WatcherPool` |
| `src/tests/tools/metrics/trend_tests.rs` | Tests for trend output |

### Modified Files

| File | Changes |
|------|---------|
| `src/daemon/mod.rs` | Open `daemon.db` in `run_daemon()`, pass to pool/handlers, handle session disconnect cleanup, auto-attach references |
| `src/daemon/workspace_pool.rs` | Add `daemon_db` field, persist workspace state on get_or_init/disconnect |
| `src/paths.rs` | Add `daemon_db()` method to `DaemonPaths` |
| `src/handler.rs` | Add `daemon_db: Option<Arc<Mutex<DaemonDatabase>>>` field, add `workspace_id: Option<String>`, update `new_with_shared_workspace`, redirect `record_tool_call` |
| `src/migration.rs` | Insert workspace row into `daemon.db` after successful index migration |
| `src/tools/metrics/mod.rs` | Add `"trend"` category, append comparison to `"code_health"`, redirect `"history"` to daemon.db |
| `src/tools/workspace/commands/registry/add_remove.rs` | Replace `WorkspaceRegistryService` with `DaemonDatabase` for add/remove |
| `src/tools/workspace/indexing/processor.rs` | Call `daemon_db.snapshot_codehealth()` after indexing completes |
| `src/tests/mod.rs` | Register new test modules |

### Removed/Deprecated Files

| File | Action |
|------|--------|
| `src/workspace/registry_service.rs` (916 lines) | Remove entirely. Functionality moves to `DaemonDatabase` |
| `src/workspace/registry.rs` (398 lines) | Keep only `generate_workspace_id()`, `sanitize_name()`, `normalize_path()`, `current_timestamp()`. Remove `WorkspaceRegistry`, `WorkspaceEntry`, `RegistryConfig`, `RegistryStatistics`, `OrphanedIndex`, `OrphanReason`, `WorkspaceStatus`, `WorkspaceType` |
| `src/tools/workspace/commands/registry/list_clean.rs` | Update to use `DaemonDatabase` instead of `WorkspaceRegistryService` |
| `src/tools/workspace/commands/registry/health.rs` | Update to use `DaemonDatabase` |
| `src/tools/workspace/commands/registry/refresh_stats.rs` | Update to use `DaemonDatabase` |

---

## Teammate Tracks

Tasks are organized into three parallel tracks matching the agent team structure. Tasks within a track are sequential. Cross-track dependencies are noted explicitly.

---

## Track A: Registry Teammate

Owns: `DaemonDatabase`, workspaces table, workspace_references, migrations, daemon startup integration.

### Task A1: DaemonDatabase Schema and Migrations

**Files:**
- Create: `src/daemon/database.rs`
- Modify: `src/daemon/mod.rs` (add `pub mod database;`)
- Modify: `src/paths.rs` (add `daemon_db()` method)
- Create: `src/tests/daemon/database.rs`

- [ ] **Step 1: Write failing test for DaemonDatabase creation**

```rust
// src/tests/daemon/database.rs
use crate::daemon::database::DaemonDatabase;
use tempfile::TempDir;

#[test]
fn test_daemon_db_create_and_migrate() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("daemon.db");
    let db = DaemonDatabase::open(&db_path).unwrap();

    // Verify tables exist
    assert!(db.table_exists("workspaces"));
    assert!(db.table_exists("workspace_references"));
    assert!(db.table_exists("codehealth_snapshots"));
    assert!(db.table_exists("tool_calls"));
}
```

Run: `cargo test --lib test_daemon_db_create_and_migrate 2>&1 | tail -10`
Expected: FAIL (module doesn't exist)

- [ ] **Step 2: Add daemon_db path to DaemonPaths**

In `src/paths.rs`, add to `impl DaemonPaths`:
```rust
pub fn daemon_db(&self) -> PathBuf {
    self.julie_home.join("daemon.db")
}
```

- [ ] **Step 3: Write DaemonDatabase with schema**

Create `src/daemon/database.rs`:
```rust
use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::Path;
use tracing::info;

const DAEMON_SCHEMA_VERSION: i32 = 1;

/// Thread-safe daemon database. Shared across sessions as `Arc<DaemonDatabase>`.
/// Uses internal Mutex (same pattern as SymbolDatabase in the codebase, which is
/// always held as `Arc<Mutex<SymbolDatabase>>`). DaemonDatabase uses its own
/// internal Mutex so callers don't need to lock externally.
pub struct DaemonDatabase {
    conn: std::sync::Mutex<Connection>,
}

impl DaemonDatabase {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = match Connection::open(path) {
            Ok(c) => c,
            Err(e) => {
                // Corruption recovery: if the database is corrupt, delete and recreate
                warn!("Failed to open daemon.db ({}), attempting recovery", e);
                if path.exists() {
                    std::fs::remove_file(path)?;
                }
                Connection::open(path)
                    .with_context(|| format!("Failed to create fresh daemon.db at {}", path.display()))?
            }
        };

        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;"
        )?;

        let db = Self { conn: std::sync::Mutex::new(conn) };
        {
            let mut conn = db.conn.lock().unwrap();
            Self::run_migrations(&mut conn)?;
        }
        Ok(db)
    }

    fn run_migrations(conn: &mut Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                applied_at INTEGER NOT NULL
            );"
        )?;

        let current: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [], |row| row.get(0),
            )?;

        if current < 1 {
            Self::migration_001_initial_schema(conn)?;
        }

        Ok(())
    }

    fn migration_001_initial_schema(conn: &mut Connection) -> Result<()> {
        info!("daemon.db migration 001: initial schema");
        let tx = conn.transaction()?;

        tx.execute_batch(
            "CREATE TABLE workspaces (
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

            CREATE TABLE workspace_references (
                primary_workspace_id    TEXT NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
                reference_workspace_id  TEXT NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
                added_at                INTEGER NOT NULL,
                PRIMARY KEY (primary_workspace_id, reference_workspace_id)
            );

            CREATE TABLE codehealth_snapshots (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace_id    TEXT NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
                timestamp       INTEGER NOT NULL,
                total_symbols   INTEGER NOT NULL,
                total_files     INTEGER NOT NULL,
                security_high   INTEGER NOT NULL DEFAULT 0,
                security_medium INTEGER NOT NULL DEFAULT 0,
                security_low    INTEGER NOT NULL DEFAULT 0,
                change_high     INTEGER NOT NULL DEFAULT 0,
                change_medium   INTEGER NOT NULL DEFAULT 0,
                change_low      INTEGER NOT NULL DEFAULT 0,
                symbols_tested    INTEGER NOT NULL DEFAULT 0,
                symbols_untested  INTEGER NOT NULL DEFAULT 0,
                avg_centrality  REAL,
                max_centrality  REAL
            );
            CREATE INDEX idx_snapshots_workspace_time
                ON codehealth_snapshots(workspace_id, timestamp);

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

            INSERT INTO schema_version (version, applied_at)
            VALUES (1, unixepoch());"
        )?;

        tx.commit()?;
        info!("daemon.db migration 001 complete");
        Ok(())
    }

    pub fn table_exists(&self, table_name: &str) -> bool {
        let conn = self.conn.lock().unwrap();
        conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                params![table_name],
                |row| row.get::<_, i32>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false)
    }
}
```

Add `pub mod database;` to `src/daemon/mod.rs`.
Register `src/tests/daemon/database.rs` in `src/tests/mod.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_daemon_db_create_and_migrate 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/daemon/database.rs src/daemon/mod.rs src/paths.rs src/tests/daemon/database.rs src/tests/mod.rs
git commit -m "feat(v6): add DaemonDatabase with initial schema and migrations"
```

---

### Task A2: Workspace CRUD Operations

**Files:**
- Modify: `src/daemon/database.rs`
- Modify: `src/tests/daemon/database.rs`

- [ ] **Step 1: Write failing tests for workspace CRUD**

```rust
#[test]
fn test_upsert_and_get_workspace() {
    let db = create_test_db();
    db.upsert_workspace("julie_a1b2c3d4", "/Users/test/julie", "ready").unwrap();

    let ws = db.get_workspace("julie_a1b2c3d4").unwrap().unwrap();
    assert_eq!(ws.path, "/Users/test/julie");
    assert_eq!(ws.status, "ready");
    assert_eq!(ws.session_count, 0);
}

#[test]
fn test_increment_decrement_session_count() {
    let db = create_test_db();
    db.upsert_workspace("ws1", "/path", "ready").unwrap();

    db.increment_session_count("ws1").unwrap();
    db.increment_session_count("ws1").unwrap();
    assert_eq!(db.get_workspace("ws1").unwrap().unwrap().session_count, 2);

    db.decrement_session_count("ws1").unwrap();
    assert_eq!(db.get_workspace("ws1").unwrap().unwrap().session_count, 1);
}

#[test]
fn test_reset_all_session_counts() {
    let db = create_test_db();
    db.upsert_workspace("ws1", "/a", "ready").unwrap();
    db.upsert_workspace("ws2", "/b", "ready").unwrap();
    db.increment_session_count("ws1").unwrap();
    db.increment_session_count("ws2").unwrap();

    db.reset_all_session_counts().unwrap();
    assert_eq!(db.get_workspace("ws1").unwrap().unwrap().session_count, 0);
    assert_eq!(db.get_workspace("ws2").unwrap().unwrap().session_count, 0);
}

#[test]
fn test_update_workspace_stats() {
    let db = create_test_db();
    db.upsert_workspace("ws1", "/path", "ready").unwrap();
    db.update_workspace_stats("ws1", 100, 50, Some("jina-code-v2"), Some(80)).unwrap();

    let ws = db.get_workspace("ws1").unwrap().unwrap();
    assert_eq!(ws.symbol_count, Some(100));
    assert_eq!(ws.file_count, Some(50));
    assert_eq!(ws.embedding_model, Some("jina-code-v2".to_string()));
    assert_eq!(ws.vector_count, Some(80));
}
```

Run: `cargo test --lib test_upsert_and_get_workspace 2>&1 | tail -10`
Expected: FAIL

- [ ] **Step 2: Implement workspace CRUD methods**

Add to `src/daemon/database.rs`:
- `WorkspaceRow` struct (workspace_id, path, status, session_count, last_indexed, symbol_count, file_count, embedding_model, vector_count)
- `upsert_workspace(workspace_id, path, status)` - INSERT OR REPLACE with timestamps
- `get_workspace(workspace_id) -> Option<WorkspaceRow>`
- `get_workspace_by_path(path) -> Option<WorkspaceRow>`
- `update_workspace_status(workspace_id, status)`
- `update_workspace_stats(workspace_id, symbol_count, file_count, embedding_model, vector_count)`
- `increment_session_count(workspace_id)`
- `decrement_session_count(workspace_id)` - clamp to 0
- `reset_all_session_counts()` - `UPDATE workspaces SET session_count = 0`
- `list_workspaces() -> Vec<WorkspaceRow>`

- [ ] **Step 3: Run tests to verify**

Run: `cargo test --lib test_upsert_and_get_workspace test_increment_decrement test_reset_all test_update_workspace_stats 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/daemon/database.rs src/tests/daemon/database.rs
git commit -m "feat(v6): add workspace CRUD operations to DaemonDatabase"
```

---

### Task A3: Workspace References CRUD

**Files:**
- Modify: `src/daemon/database.rs`
- Modify: `src/tests/daemon/database.rs`

- [ ] **Step 1: Write failing tests for references**

```rust
#[test]
fn test_add_and_list_references() {
    let db = create_test_db();
    db.upsert_workspace("primary1", "/proj", "ready").unwrap();
    db.upsert_workspace("ref1", "/lib1", "ready").unwrap();
    db.upsert_workspace("ref2", "/lib2", "ready").unwrap();

    db.add_reference("primary1", "ref1").unwrap();
    db.add_reference("primary1", "ref2").unwrap();

    let refs = db.list_references("primary1").unwrap();
    assert_eq!(refs.len(), 2);
}

#[test]
fn test_remove_reference() {
    let db = create_test_db();
    db.upsert_workspace("p1", "/proj", "ready").unwrap();
    db.upsert_workspace("r1", "/lib", "ready").unwrap();
    db.add_reference("p1", "r1").unwrap();

    db.remove_reference("p1", "r1").unwrap();
    assert_eq!(db.list_references("p1").unwrap().len(), 0);
}

#[test]
fn test_cascade_delete_removes_references() {
    let db = create_test_db();
    db.upsert_workspace("p1", "/proj", "ready").unwrap();
    db.upsert_workspace("r1", "/lib", "ready").unwrap();
    db.add_reference("p1", "r1").unwrap();

    db.delete_workspace("r1").unwrap();
    assert_eq!(db.list_references("p1").unwrap().len(), 0);
}
```

Run: `cargo test --lib test_add_and_list_references 2>&1 | tail -10`
Expected: FAIL

- [ ] **Step 2: Implement reference CRUD**

Add to `src/daemon/database.rs`:
- `add_reference(primary_id, reference_id)` - INSERT OR IGNORE
- `remove_reference(primary_id, reference_id)` - DELETE
- `list_references(primary_id) -> Vec<WorkspaceRow>` - JOIN with workspaces table
- `delete_workspace(workspace_id)` - DELETE (cascades to references and snapshots)

- [ ] **Step 3: Run tests to verify**

Run: `cargo test --lib test_add_and_list_references test_remove_reference test_cascade_delete 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/daemon/database.rs src/tests/daemon/database.rs
git commit -m "feat(v6): add workspace reference CRUD to DaemonDatabase"
```

---

### Task A4: Tool Calls and Retention

**Files:**
- Modify: `src/daemon/database.rs`
- Modify: `src/tests/daemon/database.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn test_insert_and_query_tool_calls() {
    let db = create_test_db();
    db.insert_tool_call("ws1", "sess1", "fast_search", 12.5, Some(10), None, Some(500), true, None).unwrap();
    db.insert_tool_call("ws1", "sess1", "deep_dive", 45.0, Some(1), None, Some(1200), true, None).unwrap();

    let history = db.query_tool_call_history("ws1", 7).unwrap();
    assert_eq!(history.total_calls, 2);
    assert_eq!(history.per_tool.len(), 2);
}

#[test]
fn test_prune_old_tool_calls() {
    let db = create_test_db();
    // Insert a call with a very old timestamp
    db.conn.execute(
        "INSERT INTO tool_calls (workspace_id, session_id, timestamp, tool_name, duration_ms, success)
         VALUES ('ws1', 's1', 1000000, 'old_call', 1.0, 1)",
        [],
    ).unwrap();
    // Insert a recent call
    db.insert_tool_call("ws1", "s1", "new_call", 1.0, None, None, None, true, None).unwrap();

    db.prune_tool_calls(90).unwrap();

    let count: i32 = db.conn.query_row("SELECT COUNT(*) FROM tool_calls", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1); // only the recent one survives
}
```

Run: `cargo test --lib test_insert_and_query_tool_calls 2>&1 | tail -10`
Expected: FAIL

- [ ] **Step 2: Implement tool call methods**

Add to `src/daemon/database.rs`:
- `insert_tool_call(workspace_id, session_id, tool_name, duration_ms, result_count, source_bytes, output_bytes, success, metadata)` - same signature shape as existing `SymbolDatabase::insert_tool_call` plus `workspace_id`
- `query_tool_call_history(workspace_id, days) -> HistorySummary` - reuse existing `HistorySummary` and `ToolCallSummary` types from `src/database/tool_calls.rs`
- `prune_tool_calls(retention_days)` - `DELETE FROM tool_calls WHERE timestamp < ?`

- [ ] **Step 3: Run tests to verify**

Run: `cargo test --lib test_insert_and_query_tool_calls test_prune_old_tool_calls 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/daemon/database.rs src/tests/daemon/database.rs
git commit -m "feat(v6): add tool call persistence and retention to DaemonDatabase"
```

---

### Task A5: Codehealth Snapshot Storage

**Files:**
- Modify: `src/daemon/database.rs`
- Modify: `src/tests/daemon/database.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn test_snapshot_and_retrieve_codehealth() {
    let db = create_test_db();
    db.upsert_workspace("ws1", "/path", "ready").unwrap();

    let snapshot = CodehealthSnapshot {
        total_symbols: 7306,
        total_files: 434,
        security_high: 14,
        security_medium: 25,
        security_low: 100,
        change_high: 8,
        change_medium: 30,
        change_low: 200,
        symbols_tested: 180,
        symbols_untested: 47,
        avg_centrality: Some(0.42),
        max_centrality: Some(0.95),
    };

    db.insert_codehealth_snapshot("ws1", &snapshot).unwrap();

    let latest = db.get_latest_snapshot("ws1").unwrap().unwrap();
    assert_eq!(latest.total_symbols, 7306);
    assert_eq!(latest.security_high, 14);
}

#[test]
fn test_snapshot_history() {
    let db = create_test_db();
    db.upsert_workspace("ws1", "/path", "ready").unwrap();

    // Insert 3 snapshots
    for i in 0..3 {
        let snapshot = CodehealthSnapshot {
            total_symbols: 7000 + i * 100,
            total_files: 400,
            security_high: 14 - i as i32,
            ..Default::default()
        };
        db.insert_codehealth_snapshot("ws1", &snapshot).unwrap();
    }

    let history = db.get_snapshot_history("ws1", 10).unwrap();
    assert_eq!(history.len(), 3);
    // Most recent first
    assert_eq!(history[0].total_symbols, 7200);
}
```

Run: `cargo test --lib test_snapshot_and_retrieve_codehealth 2>&1 | tail -10`
Expected: FAIL

- [ ] **Step 2: Implement snapshot methods**

Add to `src/daemon/database.rs`:
- `CodehealthSnapshot` struct (all the metric fields, implements `Default`)
- `CodehealthSnapshotRow` struct (adds `id`, `workspace_id`, `timestamp` to the above)
- `insert_codehealth_snapshot(workspace_id, &CodehealthSnapshot)` - INSERT with current timestamp
- `get_latest_snapshot(workspace_id) -> Option<CodehealthSnapshotRow>` - ORDER BY timestamp DESC LIMIT 1
- `get_snapshot_history(workspace_id, limit) -> Vec<CodehealthSnapshotRow>` - ORDER BY timestamp DESC

- [ ] **Step 3: Run tests to verify**

Run: `cargo test --lib test_snapshot_and_retrieve test_snapshot_history 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/daemon/database.rs src/tests/daemon/database.rs
git commit -m "feat(v6): add codehealth snapshot storage to DaemonDatabase"
```

---

### Task A6: Daemon Startup Integration

**Files:**
- Modify: `src/daemon/mod.rs`
- Modify: `src/daemon/workspace_pool.rs`

**Blocked by:** A1-A5 (DaemonDatabase must be complete)

- [ ] **Step 1: Write failing test for WorkspacePool with daemon_db**

```rust
#[tokio::test]
async fn test_workspace_pool_accepts_daemon_db() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&db_path).unwrap());
    let indexes_dir = tmp.path().join("indexes");
    std::fs::create_dir_all(&indexes_dir).unwrap();
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));

    // This should compile and construct without panic
    let pool = WorkspacePool::new(indexes_dir.clone(), Some(daemon_db.clone()), watcher_pool);

    // Verify the constructor wired daemon_db (pool starts empty)
    assert_eq!(pool.active_count().await, 0);
}

#[test]
fn test_daemon_db_upsert_on_workspace_init() {
    // Test that the DB methods the pool will call work correctly
    let tmp = TempDir::new().unwrap();
    let daemon_db = DaemonDatabase::open(&tmp.path().join("daemon.db")).unwrap();

    daemon_db.upsert_workspace("test_ws", "/tmp/test", "pending").unwrap();
    daemon_db.update_workspace_status("test_ws", "ready").unwrap();

    let ws = daemon_db.get_workspace("test_ws").unwrap().unwrap();
    assert_eq!(ws.status, "ready");
}
```

Run: `cargo test --lib test_workspace_pool_accepts_daemon_db 2>&1 | tail -10`
Expected: FAIL (constructor signature doesn't match yet)

- [ ] **Step 2: Add daemon_db to WorkspacePool**

Modify `src/daemon/workspace_pool.rs`:
- Add `daemon_db: Option<Arc<DaemonDatabase>>` field
- Update constructor: `pub fn new(indexes_dir: PathBuf, daemon_db: Option<Arc<DaemonDatabase>>) -> Self`
- In `get_or_init`: after initializing workspace, call `daemon_db.upsert_workspace()`
- In `mark_indexed`: call `daemon_db.update_workspace_status(id, "ready")`
- Add `pub async fn disconnect_session(&self, workspace_id: &str)` that decrements session_count

- [ ] **Step 3: Update run_daemon to open daemon.db**

Modify `src/daemon/mod.rs` `run_daemon()`:
- After PID file creation, before pool creation:
```rust
let daemon_db = Arc::new(
    DaemonDatabase::open(&paths.daemon_db())
        .context("Failed to open daemon.db")?
);
daemon_db.reset_all_session_counts()?;
daemon_db.prune_tool_calls(90)?;
info!("Daemon database ready");
```
- Pass `Some(daemon_db.clone())` to `WorkspacePool::new()`
- Pass `Some(daemon_db.clone())` to handler creation in `handle_ipc_session`

- [ ] **Step 4: Update handle_ipc_session for disconnect cleanup**

In `handle_ipc_session` after `service.waiting()` completes:
```rust
pool.disconnect_session(&full_workspace_id).await;
```

- [ ] **Step 5: Run test and verify**

Run: `cargo test --lib test_workspace_pool_persists 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/daemon/mod.rs src/daemon/workspace_pool.rs src/tests/daemon/
git commit -m "feat(v6): integrate DaemonDatabase into daemon startup and WorkspacePool"
```

---

### Task A7: Auto-Inherit Reference Workspaces on Session Connect

**Files:**
- Modify: `src/daemon/mod.rs` (`handle_ipc_session`)
- Modify: `src/daemon/workspace_pool.rs`

**Blocked by:** A3 (reference CRUD), A6 (daemon_db integrated)

This implements the spec's "instant attach" flow: when a session connects to a primary workspace, it automatically gets all reference workspaces via the `workspace_references` table.

- [ ] **Step 1: Write failing test**

```rust
#[tokio::test]
async fn test_session_inherits_references() {
    let tmp = TempDir::new().unwrap();
    let daemon_db = Arc::new(DaemonDatabase::open(&tmp.path().join("daemon.db")).unwrap());

    // Pre-populate: primary workspace with a reference
    daemon_db.upsert_workspace("primary_abc", "/proj", "ready").unwrap();
    daemon_db.upsert_workspace("ref_xyz", "/lib", "ready").unwrap();
    daemon_db.add_reference("primary_abc", "ref_xyz").unwrap();

    // Query references for the primary workspace
    let refs = daemon_db.list_references("primary_abc").unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].workspace_id, "ref_xyz");
}
```

Run: `cargo test --lib test_session_inherits_references 2>&1 | tail -10`
Expected: PASS (the DB query is already implemented in A3; this verifies the flow)

- [ ] **Step 2: Add reference loading to handle_ipc_session**

In `src/daemon/mod.rs` `handle_ipc_session`, after creating the handler, load and attach reference workspaces:

```rust
// Auto-attach reference workspaces
if let Some(ref daemon_db) = daemon_db {
    let refs = daemon_db.list_references(&full_workspace_id).unwrap_or_default();
    for ref_ws in &refs {
        match pool.get_or_init(&ref_ws.workspace_id, PathBuf::from(&ref_ws.path)).await {
            Ok(ref_workspace) => {
                // Attach reference to the handler's workspace
                // The handler needs a method to register a reference workspace
                info!(
                    session_id = %session_id,
                    reference = %ref_ws.workspace_id,
                    "Auto-attached reference workspace"
                );
            }
            Err(e) => {
                warn!(
                    reference = %ref_ws.workspace_id,
                    "Failed to auto-attach reference workspace: {}", e
                );
            }
        }
    }
}
```

Note: Use `deep_dive` on the handler to find how reference workspaces are currently registered for tool routing. The `workspace` parameter in tool calls routes to reference workspaces via `get_database_for_workspace`. Ensure the auto-attached references are visible through that path.

- [ ] **Step 3: Compile and verify**

Run: `cargo build 2>&1 | grep -E '^error' | head -10`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add src/daemon/mod.rs src/daemon/workspace_pool.rs src/tests/daemon/
git commit -m "feat(v6): auto-inherit reference workspaces on session connect"
```

---

### Task A8: Index Migration Update

**Files:**
- Modify: `src/migration.rs` (or wherever Phase 1 migration lives)

**Blocked by:** A2 (workspace upsert method)

When Phase 1's index migration copies indexes from `{project}/.julie/indexes/` to `~/.julie/indexes/`, it must also register the workspace in daemon.db.

- [ ] **Step 1: Locate migration code**

Use: `fast_search(query="migrate_workspace_index copy", search_target="definitions")`
Find the Phase 1 migration function that copies indexes.

- [ ] **Step 2: Add daemon_db registration after successful migration**

After the copy-and-validate step succeeds, add:
```rust
if let Some(ref daemon_db) = daemon_db {
    daemon_db.upsert_workspace(workspace_id, path_str, "ready")?;
    info!(workspace_id, "Registered migrated workspace in daemon.db");
}
```

Thread `Option<Arc<DaemonDatabase>>` through the migration function signature.

- [ ] **Step 3: Compile and verify**

Run: `cargo build 2>&1 | grep -E '^error' | head -10`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add src/migration.rs src/daemon/mod.rs
git commit -m "feat(v6): register migrated indexes in daemon.db"
```

---

## Track B: Watchers Teammate

Owns: `WatcherPool`, reference-counting, grace period reaper, workspace watcher detachment.

### Task B1: WatcherPool Core

**Files:**
- Create: `src/daemon/watcher_pool.rs`
- Modify: `src/daemon/mod.rs` (add `pub mod watcher_pool;`)
- Create: `src/tests/daemon/watcher_pool.rs`

- [ ] **Step 1: Write failing test for WatcherPool attach/detach**

```rust
// src/tests/daemon/watcher_pool.rs
use crate::daemon::watcher_pool::WatcherPool;
use std::time::Duration;

#[tokio::test]
async fn test_watcher_pool_attach_detach_ref_count() {
    let pool = WatcherPool::new(Duration::from_secs(300));

    // Attach should return true (created new watcher) on first call
    // We can't create real IncrementalIndexers in tests without a real workspace,
    // so test the ref-counting logic with a mock/stub approach.
    // See step 2 for the actual design.

    pool.increment_ref("ws1").await;
    assert_eq!(pool.ref_count("ws1").await, 1);

    pool.increment_ref("ws1").await;
    assert_eq!(pool.ref_count("ws1").await, 2);

    pool.decrement_ref("ws1").await;
    assert_eq!(pool.ref_count("ws1").await, 1);

    pool.decrement_ref("ws1").await;
    assert_eq!(pool.ref_count("ws1").await, 0);
    // Grace deadline should now be set
    assert!(pool.has_grace_deadline("ws1").await);
}

#[tokio::test]
async fn test_watcher_pool_reattach_cancels_grace() {
    let pool = WatcherPool::new(Duration::from_secs(300));
    pool.increment_ref("ws1").await;
    pool.decrement_ref("ws1").await;
    assert!(pool.has_grace_deadline("ws1").await);

    // Reattach should cancel the grace deadline
    pool.increment_ref("ws1").await;
    assert!(!pool.has_grace_deadline("ws1").await);
    assert_eq!(pool.ref_count("ws1").await, 1);
}
```

Run: `cargo test --lib test_watcher_pool_attach_detach 2>&1 | tail -10`
Expected: FAIL

- [ ] **Step 2: Implement WatcherPool**

Create `src/daemon/watcher_pool.rs`:

```rust
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, warn};

pub struct WatcherPool {
    entries: RwLock<HashMap<String, WatcherEntry>>,
    grace_period: Duration,
}

struct WatcherEntry {
    watcher: Option<IncrementalIndexer>,  // None until start_watching() called
    ref_count: usize,
    grace_deadline: Option<Instant>,
}

impl WatcherPool {
    pub fn new(grace_period: Duration) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            grace_period,
        }
    }

    pub async fn increment_ref(&self, workspace_id: &str) {
        let mut guard = self.entries.write().await;
        let entry = guard.entry(workspace_id.to_string()).or_insert(WatcherEntry {
            watcher: None,
            ref_count: 0,
            grace_deadline: None,
        });
        entry.ref_count += 1;
        entry.grace_deadline = None; // cancel any pending grace period
    }

    pub async fn decrement_ref(&self, workspace_id: &str) {
        let mut guard = self.entries.write().await;
        if let Some(entry) = guard.get_mut(workspace_id) {
            entry.ref_count = entry.ref_count.saturating_sub(1);
            if entry.ref_count == 0 {
                entry.grace_deadline = Some(Instant::now() + self.grace_period);
                info!(workspace_id, "Watcher grace period started");
            }
        }
    }

    pub async fn ref_count(&self, workspace_id: &str) -> usize {
        let guard = self.entries.read().await;
        guard.get(workspace_id).map(|e| e.ref_count).unwrap_or(0)
    }

    pub async fn has_grace_deadline(&self, workspace_id: &str) -> bool {
        let guard = self.entries.read().await;
        guard.get(workspace_id).and_then(|e| e.grace_deadline).is_some()
    }

    /// Reap expired entries. Call this from a periodic background task.
    pub async fn reap_expired(&self) -> Vec<String> {
        let mut guard = self.entries.write().await;
        let now = Instant::now();
        let mut reaped = Vec::new();

        guard.retain(|id, entry| {
            if let Some(deadline) = entry.grace_deadline {
                if now >= deadline {
                    info!(workspace_id = %id, "Reaping expired watcher");
                    reaped.push(id.clone());
                    return false; // remove
                }
            }
            true
        });

        reaped
    }
}
```

Add `pub mod watcher_pool;` to `src/daemon/mod.rs`.
Register test module in `src/tests/mod.rs`.

- [ ] **Step 3: Run tests to verify**

Run: `cargo test --lib test_watcher_pool_attach_detach test_watcher_pool_reattach 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/daemon/watcher_pool.rs src/daemon/mod.rs src/tests/daemon/watcher_pool.rs src/tests/mod.rs
git commit -m "feat(v6): add WatcherPool with ref-counting and grace period"
```

---

### Task B2: Background Reaper Task

**Files:**
- Modify: `src/daemon/watcher_pool.rs`
- Modify: `src/tests/daemon/watcher_pool.rs`

- [ ] **Step 1: Write failing test for reaper**

```rust
#[tokio::test]
async fn test_reaper_removes_expired_entries() {
    // Use a very short grace period for testing
    let pool = WatcherPool::new(Duration::from_millis(50));
    pool.increment_ref("ws1").await;
    pool.decrement_ref("ws1").await;
    assert!(pool.has_grace_deadline("ws1").await);

    // Wait for grace period to expire
    tokio::time::sleep(Duration::from_millis(100)).await;

    let reaped = pool.reap_expired().await;
    assert_eq!(reaped, vec!["ws1"]);
    assert_eq!(pool.ref_count("ws1").await, 0);
    assert!(!pool.has_grace_deadline("ws1").await);
}
```

Run: `cargo test --lib test_reaper_removes_expired 2>&1 | tail -10`
Expected: PASS (reap_expired is already implemented in B1)

- [ ] **Step 2: Add spawn_reaper method**

Add to `WatcherPool`:
```rust
/// Spawn a background task that reaps expired watchers every `interval`.
pub fn spawn_reaper(self: &Arc<Self>, interval: Duration) -> tokio::task::JoinHandle<()> {
    let pool = Arc::clone(self);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(interval);
        loop {
            tick.tick().await;
            let reaped = pool.reap_expired().await;
            if !reaped.is_empty() {
                info!(count = reaped.len(), "Reaped expired watchers");
            }
        }
    })
}
```

- [ ] **Step 3: Commit**

```bash
git add src/daemon/watcher_pool.rs src/tests/daemon/watcher_pool.rs
git commit -m "feat(v6): add background reaper task to WatcherPool"
```

---

### Task B3: WatcherPool Creates Real IncrementalIndexers

**Files:**
- Modify: `src/daemon/watcher_pool.rs`
- Modify: `src/tests/daemon/watcher_pool.rs`

**Blocked by:** B1-B2 (WatcherPool core must be complete)

The ref-counting from B1-B2 tracks when watchers should exist, but doesn't create actual `IncrementalIndexer` instances. This task adds methods to create and stop real file watchers.

- [ ] **Step 1: Add attach/detach methods that create IncrementalIndexers**

Add to `WatcherPool`:
```rust
/// Attach a session to a workspace's watcher. Creates the IncrementalIndexer
/// on first attach, reuses it on subsequent attaches.
pub async fn attach(
    &self,
    workspace_id: &str,
    workspace: &JulieWorkspace,
) -> Result<()> {
    let mut guard = self.entries.write().await;
    let entry = guard.entry(workspace_id.to_string()).or_insert(WatcherEntry {
        watcher: None,
        ref_count: 0,
        grace_deadline: None,
    });
    entry.ref_count += 1;
    entry.grace_deadline = None;

    // Create watcher on first attach
    if entry.watcher.is_none() {
        if let (Some(db), Some(search_index)) = (&workspace.db, &workspace.search_index) {
            let extractor_mgr = Arc::new(ExtractorManager::new());
            let mut indexer = IncrementalIndexer::new(
                workspace.root.clone(),
                db.clone(),
                extractor_mgr,
                Some(search_index.clone()),
                workspace.embedding_provider.clone(),
            )?;
            indexer.start_watching().await?;
            entry.watcher = Some(indexer);
            info!(workspace_id, "File watcher created and started");
        }
    }
    Ok(())
}

/// Detach a session. Starts grace period when ref_count hits 0.
pub async fn detach(&self, workspace_id: &str) {
    let mut guard = self.entries.write().await;
    if let Some(entry) = guard.get_mut(workspace_id) {
        entry.ref_count = entry.ref_count.saturating_sub(1);
        if entry.ref_count == 0 {
            entry.grace_deadline = Some(Instant::now() + self.grace_period);
            info!(workspace_id, "Watcher grace period started");
        }
    }
}
```

- [ ] **Step 2: Update reap_expired to stop watchers**

```rust
pub async fn reap_expired(&self) -> Vec<String> {
    let mut guard = self.entries.write().await;
    let now = Instant::now();
    let mut reaped = Vec::new();

    guard.retain(|id, entry| {
        if let Some(deadline) = entry.grace_deadline {
            if now >= deadline {
                // Stop the watcher before removing
                if let Some(mut watcher) = entry.watcher.take() {
                    let id_clone = id.clone();
                    tokio::spawn(async move {
                        if let Err(e) = watcher.stop().await {
                            warn!(workspace_id = %id_clone, "Failed to stop watcher: {}", e);
                        }
                    });
                }
                info!(workspace_id = %id, "Reaped expired watcher");
                reaped.push(id.clone());
                return false;
            }
        }
        true
    });

    reaped
}
```

- [ ] **Step 3: Compile and test**

Run: `cargo test --lib tests::daemon::watcher_pool 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/daemon/watcher_pool.rs src/tests/daemon/watcher_pool.rs
git commit -m "feat(v6): WatcherPool creates and stops real IncrementalIndexers"
```

---

### Task B4: Wire WatcherPool into Daemon and Disable Workspace-Level Watchers

**Files:**
- Modify: `src/daemon/mod.rs`
- Modify: `src/daemon/workspace_pool.rs`
- Modify: `src/workspace/mod.rs`

**Blocked by:** B3 (real watcher creation), A6 (daemon startup)

- [ ] **Step 1: Add WatcherPool to WorkspacePool**

In `src/daemon/workspace_pool.rs`:
- Add `watcher_pool: Arc<WatcherPool>` field
- Update constructor to accept `watcher_pool`
- In `get_or_init`: after initializing workspace, call `self.watcher_pool.attach(workspace_id, &workspace)`
- In `disconnect_session`: call `self.watcher_pool.detach(workspace_id)`

- [ ] **Step 2: Ensure JulieWorkspace::watcher is None in daemon mode**

In `WorkspacePool::init_workspace` (already in `src/daemon/workspace_pool.rs`), the workspace is built with `watcher: None`. Verify this is the case. The WatcherPool owns all watchers in daemon mode; the workspace must not create its own.

Also check `new_with_shared_workspace` in `src/handler.rs` - the cloned workspace must also have `watcher: None`. Use `deep_dive(symbol="new_with_shared_workspace")` to verify.

- [ ] **Step 3: Create and spawn WatcherPool in run_daemon**

In `src/daemon/mod.rs` `run_daemon()`:
```rust
let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));
let _reaper = watcher_pool.spawn_reaper(Duration::from_secs(60));

let pool = Arc::new(WorkspacePool::new(
    paths.indexes_dir(),
    Some(daemon_db.clone()),
    watcher_pool.clone(),
));
```

- [ ] **Step 4: Update existing WorkspacePool tests**

Update `src/tests/daemon/workspace_pool.rs` to pass the new constructor args.

- [ ] **Step 5: Run tests**

Run: `cargo test --lib tests::daemon 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/daemon/mod.rs src/daemon/workspace_pool.rs src/workspace/mod.rs src/tests/daemon/
git commit -m "feat(v6): wire WatcherPool into daemon lifecycle, disable workspace-level watchers"
```

---

## Track C: Routing Teammate

Owns: `manage_workspace` registry integration, codehealth snapshots, tool_calls redirection, query_metrics trend.

### Task C1: Add daemon_db to Handler

**Files:**
- Modify: `src/handler.rs`

**Blocked by:** A1 (DaemonDatabase struct must exist)

- [ ] **Step 1: Add daemon_db field to JulieServerHandler**

In `src/handler.rs`, add to `JulieServerHandler` struct:
```rust
/// Daemon-level database for persistent metrics and workspace registry.
/// None in stdio mode (no daemon), Some in daemon mode.
pub(crate) daemon_db: Option<Arc<DaemonDatabase>>,
```

- [ ] **Step 2: Update new_with_shared_workspace to accept daemon_db**

Update the constructor signature:
```rust
pub async fn new_with_shared_workspace(
    workspace: Arc<JulieWorkspace>,
    workspace_root: PathBuf,
    daemon_db: Option<Arc<DaemonDatabase>>,
) -> Result<Self> {
```

Set `self.daemon_db = daemon_db` in the constructor body.

- [ ] **Step 3: Update new() (stdio mode) to set daemon_db = None**

In the existing `new()` constructor, set `daemon_db: None`.

- [ ] **Step 4: Update handle_ipc_session call site**

In `src/daemon/mod.rs` `handle_ipc_session`, pass `daemon_db` to the handler:
```rust
let handler = JulieServerHandler::new_with_shared_workspace(
    workspace,
    workspace_path,
    Some(daemon_db.clone()),
).await?;
```

This requires `handle_ipc_session` to receive `daemon_db` as a parameter. Update its signature and the call in `accept_loop`.

- [ ] **Step 5: Compile check**

Run: `cargo build 2>&1 | grep -E '^error' -A3 | head -20`
Expected: No errors

- [ ] **Step 6: Commit**

```bash
git add src/handler.rs src/daemon/mod.rs
git commit -m "feat(v6): add daemon_db field to JulieServerHandler"
```

---

### Task C2: Redirect tool_calls to daemon.db

**Files:**
- Modify: `src/handler.rs` (`record_tool_call`)
- Modify: `src/tests/daemon/database.rs` (verify integration)

**Blocked by:** C1 (handler has daemon_db), A4 (tool call methods exist)

- [ ] **Step 1: Write failing test for daemon_db branching in record_tool_call**

```rust
#[test]
fn test_daemon_db_tool_call_with_workspace_id() {
    let tmp = TempDir::new().unwrap();
    let daemon_db = DaemonDatabase::open(&tmp.path().join("daemon.db")).unwrap();

    // Simulate what record_tool_call will do: write with workspace_id
    daemon_db.insert_tool_call(
        "julie_abc123", "session_001", "fast_search",
        5.0, Some(3), None, Some(200), true, None,
    ).unwrap();
    daemon_db.insert_tool_call(
        "julie_abc123", "session_001", "deep_dive",
        45.0, Some(1), None, Some(1200), true, None,
    ).unwrap();

    // Verify cross-workspace query works
    let history = daemon_db.query_tool_call_history("julie_abc123", 7).unwrap();
    assert_eq!(history.total_calls, 2);
    assert_eq!(history.per_tool.len(), 2);
}
```

Run: `cargo test --lib test_daemon_db_tool_call_with_workspace 2>&1 | tail -10`
Expected: PASS (DB methods from A4; this validates the workspace_id flow)

- [ ] **Step 2: Update record_tool_call in handler.rs**

In the `record_tool_call` method, after the existing fire-and-forget `tokio::spawn`, add a daemon_db write path:

```rust
// Write to daemon.db if available (daemon mode)
if let Some(ref daemon_db) = self.daemon_db {
    let daemon_db = daemon_db.clone();
    let workspace_id = self.workspace_id().unwrap_or_default();
    let session_id = self.session_metrics.session_id.clone();
    let tool_name = tool_name.to_string();
    let duration_ms = duration.as_secs_f64() * 1000.0;
    let result_count = report.result_count;
    let output_bytes = report.output_bytes;
    let metadata_str = /* same as existing */;

    tokio::task::spawn_blocking(move || {
        if let Err(e) = daemon_db.insert_tool_call(
            &workspace_id, &session_id, &tool_name,
            duration_ms, result_count, None, output_bytes,
            true, metadata_str.as_deref(),
        ) {
            tracing::warn!("Failed to write tool call to daemon.db: {}", e);
        }
    });
}
```

Note: Need to add a `workspace_id()` helper to the handler that returns the current workspace ID. Use `deep_dive` on the handler to find how workspace_id is resolved (it's computed in `handle_ipc_session` and should be stored on the handler).

- [ ] **Step 3: Add workspace_id field to handler**

Add `pub(crate) workspace_id: Option<String>` to `JulieServerHandler`. Set it in `new_with_shared_workspace` (passed as a parameter or computed from workspace_root).

- [ ] **Step 4: Compile and test**

Run: `cargo build 2>&1 | grep -E '^error' | head -10`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add src/handler.rs
git commit -m "feat(v6): redirect tool call recording to daemon.db in daemon mode"
```

---

### Task C3: Codehealth Snapshot Capture After Indexing

**Files:**
- Modify: `src/tools/workspace/indexing/processor.rs`
- Modify: `src/daemon/database.rs` (add `snapshot_codehealth_from_db` method)

**Blocked by:** A5 (snapshot storage methods exist), C1 (handler has daemon_db)

- [ ] **Step 1: Write failing test for snapshot_codehealth_from_db**

```rust
#[test]
fn test_snapshot_codehealth_from_symbols_db() {
    let daemon_db = create_test_daemon_db();
    daemon_db.upsert_workspace("ws1", "/path", "ready").unwrap();

    // Create a symbols.db with some test data
    let tmp = TempDir::new().unwrap();
    let symbols_db = SymbolDatabase::open(tmp.path().join("symbols.db")).unwrap();
    // ... insert test symbols with metadata containing risk scores ...

    daemon_db.snapshot_codehealth_from_db("ws1", &symbols_db).unwrap();

    let snapshot = daemon_db.get_latest_snapshot("ws1").unwrap().unwrap();
    assert!(snapshot.total_symbols > 0);
}
```

Run: `cargo test --lib test_snapshot_codehealth_from_symbols 2>&1 | tail -10`
Expected: FAIL

- [ ] **Step 2: Implement snapshot_codehealth_from_db**

Add to `src/daemon/database.rs`:
```rust
/// Query aggregate stats from a symbols.db and insert a codehealth snapshot.
pub fn snapshot_codehealth_from_db(
    &self,
    workspace_id: &str,
    symbols_db: &SymbolDatabase,
) -> Result<()> {
    // Query symbols_db for aggregate metrics:
    // - COUNT(*) for total_symbols
    // - COUNT(DISTINCT file_path) for total_files (from files table)
    // - COUNT with metadata JSON extraction for risk levels
    // - Test coverage from metadata
    // - AVG/MAX reference_score for centrality
    let snapshot = /* build CodehealthSnapshot from queries */;
    self.insert_codehealth_snapshot(workspace_id, &snapshot)
}
```

Use `deep_dive` on `SymbolDatabase` to find the right query patterns for metadata extraction. The metadata column stores JSON with keys like `security_risk`, `change_risk`, `test_coverage`.

- [ ] **Step 3: Hook into process_files_optimized**

At the end of `process_files_optimized` in `src/tools/workspace/indexing/processor.rs`, after all analysis passes complete (around line 540-549), add:

```rust
// Capture codehealth snapshot in daemon.db
if let Some(ref daemon_db) = handler.daemon_db {
    if let Some(ref db_arc) = ws.db {
        if let Ok(db) = db_arc.lock() {
            if let Err(e) = daemon_db.snapshot_codehealth_from_db(&workspace_id, &db) {
                warn!("Failed to capture codehealth snapshot: {}", e);
            } else {
                info!(workspace_id = %workspace_id, "Codehealth snapshot captured");
            }
        }
    }
}
```

- [ ] **Step 4: Run test to verify**

Run: `cargo test --lib test_snapshot_codehealth_from_symbols 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/daemon/database.rs src/tools/workspace/indexing/processor.rs src/tests/daemon/database.rs
git commit -m "feat(v6): capture codehealth snapshot after indexing"
```

---

### Task C4: query_metrics Trend Comparison and New Category

**Files:**
- Create: `src/tools/metrics/trend.rs`
- Modify: `src/tools/metrics/mod.rs`
- Create: `src/tests/tools/metrics/trend_tests.rs`

**Blocked by:** A5 (snapshot methods), C1 (handler has daemon_db)

- [ ] **Step 1: Write failing test for trend formatting**

```rust
// src/tests/tools/metrics/trend_tests.rs
use crate::tools::metrics::trend;
use crate::daemon::database::{CodehealthSnapshot, CodehealthSnapshotRow};

#[test]
fn test_format_comparison() {
    let previous = CodehealthSnapshotRow {
        timestamp: 1711100000,
        total_symbols: 7306,
        security_high: 14,
        symbols_untested: 47,
        avg_centrality: Some(0.42),
        ..Default::default()
    };
    let current = CodehealthSnapshot {
        total_symbols: 7412,
        security_high: 9,
        symbols_untested: 31,
        avg_centrality: Some(0.45),
        ..Default::default()
    };

    let output = trend::format_comparison(&current, &previous);
    assert!(output.contains("14 → 9"));
    assert!(output.contains("↓5"));
}

#[test]
fn test_format_trend_history() {
    let snapshots = vec![
        CodehealthSnapshotRow {
            id: 3, workspace_id: "ws1".into(), timestamp: 1711150000,
            total_symbols: 7412, total_files: 440,
            security_high: 9, security_medium: 20, security_low: 95,
            change_high: 5, change_medium: 25, change_low: 190,
            symbols_tested: 200, symbols_untested: 31,
            avg_centrality: Some(0.45), max_centrality: Some(0.97),
        },
        CodehealthSnapshotRow {
            id: 2, workspace_id: "ws1".into(), timestamp: 1710980000,
            total_symbols: 7306, total_files: 434,
            security_high: 14, security_medium: 25, security_low: 100,
            change_high: 8, change_medium: 30, change_low: 200,
            symbols_tested: 180, symbols_untested: 47,
            avg_centrality: Some(0.42), max_centrality: Some(0.95),
        },
        CodehealthSnapshotRow {
            id: 1, workspace_id: "ws1".into(), timestamp: 1710800000,
            total_symbols: 7201, total_files: 430,
            security_high: 16, security_medium: 28, security_low: 105,
            change_high: 10, change_medium: 32, change_low: 195,
            symbols_tested: 165, symbols_untested: 52,
            avg_centrality: Some(0.41), max_centrality: Some(0.94),
        },
    ];
    let output = trend::format_trend_table(&snapshots);
    assert!(output.contains("Date"));
    assert!(output.contains("7412"));
    assert!(output.contains("7306"));
}
```

Run: `cargo test --lib test_format_comparison 2>&1 | tail -10`
Expected: FAIL

- [ ] **Step 2: Implement trend.rs**

Create `src/tools/metrics/trend.rs`:
- `format_comparison(current: &CodehealthSnapshot, previous: &CodehealthSnapshotRow) -> String` - produces the arrow-format comparison block
- `format_trend_table(snapshots: &[CodehealthSnapshotRow]) -> String` - produces the tabular history
- Helper: `format_delta(old: i32, new: i32) -> String` - "14 → 9  (↓5, -36%)"

- [ ] **Step 3: Wire into query_metrics call_tool**

In `src/tools/metrics/mod.rs` `call_tool()`:
- Add `"trend"` match arm: query daemon_db snapshot history, format with `trend::format_trend_table`
- In the `"code_health"` (default) arm: after building the existing output, append `trend::format_comparison` if daemon_db has a previous snapshot
- In the `"history"` arm: if daemon_db is available, read from daemon_db instead of per-workspace symbols.db

Add `pub(crate) mod trend;` to `src/tools/metrics/mod.rs`.
Register test module in `src/tests/mod.rs`.

- [ ] **Step 4: Run tests to verify**

Run: `cargo test --lib test_format_comparison test_format_trend 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/tools/metrics/trend.rs src/tools/metrics/mod.rs src/tests/tools/metrics/trend_tests.rs src/tests/mod.rs
git commit -m "feat(v6): add codehealth trend comparison and history to query_metrics"
```

---

### Task C5: Replace WorkspaceRegistryService in manage_workspace

**Files:**
- Modify: `src/tools/workspace/commands/registry/add_remove.rs`
- Modify: `src/tools/workspace/commands/registry/list_clean.rs`
- Modify: `src/tools/workspace/commands/registry/health.rs`
- Modify: `src/tools/workspace/commands/registry/refresh_stats.rs`

**Blocked by:** A2-A3 (workspace and reference CRUD in DaemonDatabase)

- [ ] **Step 1: Update handle_add_command**

In `src/tools/workspace/commands/registry/add_remove.rs`:
- Replace `WorkspaceRegistryService::new()` and `register_workspace()` with `handler.daemon_db` methods
- Replace `update_workspace_statistics()` with `daemon_db.update_workspace_stats()`
- Replace the reference workspace registration with `daemon_db.add_reference(primary_id, ref_id)`
- If `daemon_db` is None (stdio mode), fall back to the existing WorkspaceRegistryService behavior (or return an error, since we said no fallback)

Key flow:
1. Compute ref workspace_id from path
2. Check `daemon_db.get_workspace(ref_id)` - if `status = "ready"`, instant attach
3. If not found: `daemon_db.upsert_workspace(ref_id, path, "indexing")`, then index, then update to "ready"
4. `daemon_db.add_reference(primary_id, ref_id)`

- [ ] **Step 2: Update handle_remove_command**

Replace `registry_service.unregister_workspace()` with `daemon_db.remove_reference()`. Note: we remove the reference, not the workspace itself (other projects may reference it).

- [ ] **Step 3: Update list_clean.rs**

Replace `registry_service.get_workspace()` / list operations with `daemon_db.list_workspaces()` and `daemon_db.list_references()`.

- [ ] **Step 4: Update health.rs and refresh_stats.rs**

Replace registry service calls with daemon_db equivalents.

- [ ] **Step 5: Compile and test**

Run: `cargo build 2>&1 | grep -E '^error' | head -20`
Then: `cargo test --lib tests::tools::workspace 2>&1 | tail -20`
Expected: No errors, tests pass

- [ ] **Step 6: Commit**

```bash
git add src/tools/workspace/commands/registry/
git commit -m "refactor(v6): replace WorkspaceRegistryService with DaemonDatabase in manage_workspace"
```

---

### Task C6: Remove Deprecated Registry Code

**Files:**
- Remove: `src/workspace/registry_service.rs`
- Modify: `src/workspace/registry.rs` (keep only utility functions)
- Modify: `src/workspace/mod.rs` (remove registry_service module declaration)
- Modify: any files that import from the removed modules

**Blocked by:** C5 (all callers migrated)

- [ ] **Step 1: Use fast_refs to find all remaining callers**

Use `fast_refs(symbol="WorkspaceRegistryService")` and `fast_refs(symbol="WorkspaceRegistry")` to find any remaining references. Fix each one.

- [ ] **Step 2: Remove registry_service.rs**

Delete the file. Remove `pub mod registry_service;` from `src/workspace/mod.rs`.

- [ ] **Step 3: Trim registry.rs**

Keep: `generate_workspace_id()`, `sanitize_name()`, `normalize_path()`, `extract_workspace_name()`, `current_timestamp()`.
Remove: `WorkspaceRegistry`, `WorkspaceEntry`, `RegistryConfig`, `RegistryStatistics`, `OrphanedIndex`, `OrphanReason`, `WorkspaceStatus`, `WorkspaceType`, and all their impls.

- [ ] **Step 4: Compile and test**

Run: `cargo build 2>&1 | grep -E '^error' | head -20`
Then: `cargo test --lib 2>&1 | tail -5`
Expected: Clean build, all tests pass

- [ ] **Step 5: Commit**

```bash
git add -A src/workspace/
git commit -m "refactor(v6): remove deprecated WorkspaceRegistryService and trim registry.rs"
```

---

## Integration Tasks (Lead)

These tasks run after all three tracks complete their work.

### Task I1: Integration Test

**Files:**
- Modify: `src/tests/integration/` (add daemon Phase 2 integration test)

**Blocked by:** All tracks complete

- [ ] **Step 1: Write integration test**

Test the full flow: daemon starts, opens daemon.db, two sessions connect, one adds a reference workspace, second session sees it, codehealth snapshot is captured, tool calls are recorded, disconnect decrements session counts.

- [ ] **Step 2: Run cargo xtask test dev**

Run: `cargo xtask test dev 2>&1 | tail -20`
Expected: All green

- [ ] **Step 3: Commit**

```bash
git add src/tests/integration/
git commit -m "test(v6): add Phase 2 integration tests for shared workspaces"
```

---

### Task I2: Update CLAUDE.md and Documentation

**Files:**
- Modify: `CLAUDE.md` (update architecture section)
- Modify: `TODO.md` (check off "Multiple Instances" tech debt item)

- [ ] **Step 1: Update architecture sections**

Add daemon.db to the architecture overview. Update the storage layout. Note Phase 2 features.

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md TODO.md
git commit -m "docs(v6): update architecture docs for Phase 2"
```
