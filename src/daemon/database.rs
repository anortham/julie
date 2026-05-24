//! Persistent daemon state: workspace registry, codehealth snapshots, tool call history.
//!
//! `DaemonDatabase` wraps a single SQLite connection to `~/.julie/daemon.db`.
//! It is shared across all sessions as `Arc<DaemonDatabase>`. The internal
//! `Mutex<Connection>` makes it safe to call from multiple tokio tasks.

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::Path;
use tracing::warn;

mod migrations;
mod search_compare;
mod tool_calls;

pub use search_compare::{
    SearchCompareCaseInput, SearchCompareCaseRow, SearchCompareRunInput, SearchCompareRunRow,
};
pub use tool_calls::SearchToolCallRow;

/// Thread-safe daemon database. Shared across sessions as `Arc<DaemonDatabase>`.
///
/// Uses an internal `Mutex<Connection>` so callers don't need to lock externally.
/// This is the same pattern used by `SymbolDatabase`, which is held externally as
/// `Arc<Mutex<SymbolDatabase>>`. Here the lock is internal for ergonomics.
pub struct DaemonDatabase {
    conn: std::sync::Mutex<Connection>,
}

impl DaemonDatabase {
    /// Open (or create) the daemon database at `path`, running migrations as needed.
    ///
    /// If the database is corrupt, deletes and recreates it (corruption recovery).
    pub fn open(path: &Path) -> Result<Self> {
        let conn = match Connection::open(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to open daemon.db ({}), attempting recovery", e);
                if path.exists() {
                    std::fs::remove_file(path)?;
                }
                Connection::open(path).with_context(|| {
                    format!("Failed to create fresh daemon.db at {}", path.display())
                })?
            }
        };

        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;",
        )?;

        let db = Self {
            conn: std::sync::Mutex::new(conn),
        };

        {
            let mut conn = db.conn.lock().unwrap_or_else(|p| p.into_inner());
            migrations::run_migrations(&mut conn)?;
        }

        Ok(db)
    }

    /// Returns true if a table with the given name exists in the database.
    pub fn table_exists(&self, table_name: &str) -> bool {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            params![table_name],
            |row| row.get::<_, i32>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false)
    }

    // -------------------------------------------------------------------------
    // Workspace CRUD
    // -------------------------------------------------------------------------

    /// Insert or update a workspace row. `status` should be one of:
    /// `pending`, `indexing`, `ready`, `error`.
    ///
    /// On conflict with an existing path, updates `status` and `updated_at`.
    /// A "ready" workspace is never downgraded to "pending" by the upsert;
    /// use `update_workspace_status` for explicit status changes.
    pub fn upsert_workspace(&self, workspace_id: &str, path: &str, status: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let now = now_unix();
        conn.execute(
            "INSERT INTO workspaces (workspace_id, path, status, session_count,
                created_at, updated_at)
             VALUES (?1, ?2, ?3, 0, ?4, ?4)
             ON CONFLICT(path) DO UPDATE SET
                status     = CASE
                    WHEN workspaces.status = 'ready' AND excluded.status = 'pending'
                    THEN 'ready'
                    ELSE excluded.status
                END,
                updated_at = excluded.updated_at",
            params![workspace_id, path, status, now],
        )?;
        Ok(())
    }

    /// Get a workspace row by ID, returns `None` if it doesn't exist.
    pub fn get_workspace(&self, workspace_id: &str) -> Result<Option<WorkspaceRow>> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare_cached(
            "SELECT workspace_id, path, status, session_count, last_indexed,
                    symbol_count, file_count, embedding_model, vector_count,
                    created_at, updated_at, last_index_duration_ms
             FROM workspaces WHERE workspace_id = ?1",
        )?;
        let mut rows = stmt.query(params![workspace_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(WorkspaceRow::from_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// Get a workspace row by filesystem path, returns `None` if it doesn't exist.
    pub fn get_workspace_by_path(&self, path: &str) -> Result<Option<WorkspaceRow>> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare_cached(
            "SELECT workspace_id, path, status, session_count, last_indexed,
                    symbol_count, file_count, embedding_model, vector_count,
                    created_at, updated_at, last_index_duration_ms
             FROM workspaces WHERE path = ?1",
        )?;
        let mut rows = stmt.query(params![path])?;
        if let Some(row) = rows.next()? {
            Ok(Some(WorkspaceRow::from_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// Normalize workspace paths and restore stuck statuses on daemon startup.
    ///
    /// Two fixes applied on every startup:
    /// 1. (Windows only) Convert forward-slash paths to native backslashes,
    ///    fixing paths stored by the adapter's previous `.replace('\\', "/")`
    /// 2. (All platforms) Restore "ready" status for workspaces that have
    ///    symbols (were previously indexed) but are stuck at "pending"
    pub fn normalize_workspace_paths(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let now = now_unix();

        let mut count = 0;
        let mut stmt =
            conn.prepare("SELECT workspace_id, path, status, symbol_count FROM workspaces")?;
        let rows: Vec<(String, String, String, Option<i64>)> = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);

        for (workspace_id, path, status, symbol_count) in &rows {
            // On Windows, convert forward slashes to backslashes.
            // On Unix, paths are already correct.
            let native_path = if cfg!(windows) {
                path.replace('/', "\\")
            } else {
                path.clone()
            };
            let needs_path_fix = native_path != *path;

            // Restore "ready" for workspaces that were indexed but stuck at "pending"
            let needs_status_fix = *status == "pending" && symbol_count.unwrap_or(0) > 0;

            if needs_path_fix || needs_status_fix {
                let new_status = if needs_status_fix {
                    "ready"
                } else {
                    status.as_str()
                };
                conn.execute(
                    "UPDATE workspaces SET path = ?1, status = ?2, updated_at = ?3 WHERE workspace_id = ?4",
                    params![native_path, new_status, now, workspace_id],
                )?;
                count += 1;
            }
        }

        Ok(count)
    }

    /// Update just the `status` column (e.g. `pending` -> `indexing` -> `ready`).
    pub fn update_workspace_status(&self, workspace_id: &str, status: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "UPDATE workspaces SET status = ?1, updated_at = ?2 WHERE workspace_id = ?3",
            params![status, now_unix(), workspace_id],
        )?;
        Ok(())
    }

    /// Record stats from a completed indexing run. Also sets `last_indexed` to now.
    pub fn update_workspace_stats(
        &self,
        workspace_id: &str,
        symbol_count: i64,
        file_count: i64,
        embedding_model: Option<&str>,
        vector_count: Option<i64>,
        index_duration_ms: Option<u64>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let now = now_unix();
        conn.execute(
            "UPDATE workspaces
             SET symbol_count    = ?1,
                 file_count      = ?2,
                 embedding_model = COALESCE(?3, embedding_model),
                 vector_count    = COALESCE(?4, vector_count),
                 last_indexed    = ?5,
                 updated_at      = ?5,
                 last_index_duration_ms = COALESCE(?7, last_index_duration_ms)
             WHERE workspace_id  = ?6",
            params![
                symbol_count,
                file_count,
                embedding_model,
                vector_count,
                now,
                workspace_id,
                index_duration_ms.map(|d| d as i64),
            ],
        )?;
        Ok(())
    }

    /// Update just the `vector_count` column after an embedding pipeline completes.
    pub fn update_vector_count(&self, workspace_id: &str, vector_count: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "UPDATE workspaces SET vector_count = ?1, updated_at = ?2 WHERE workspace_id = ?3",
            params![vector_count, now_unix(), workspace_id],
        )?;
        Ok(())
    }

    /// Update just the `embedding_model` column.
    pub fn update_embedding_model(&self, workspace_id: &str, model: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "UPDATE workspaces SET embedding_model = ?1, updated_at = ?2 WHERE workspace_id = ?3",
            params![model, now_unix(), workspace_id],
        )?;
        Ok(())
    }

    /// Increment `session_count` for a workspace (called when a session attaches).
    pub fn increment_session_count(&self, workspace_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "UPDATE workspaces
             SET session_count = session_count + 1, updated_at = ?1
             WHERE workspace_id = ?2",
            params![now_unix(), workspace_id],
        )?;
        Ok(())
    }

    /// Decrement `session_count`, clamping to 0 (called when a session detaches).
    pub fn decrement_session_count(&self, workspace_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "UPDATE workspaces
             SET session_count = MAX(0, session_count - 1), updated_at = ?1
             WHERE workspace_id = ?2",
            params![now_unix(), workspace_id],
        )?;
        Ok(())
    }

    /// Reset all session counts to 0. Called on daemon startup to recover from
    /// a crash that left counts non-zero.
    pub fn reset_all_session_counts(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "UPDATE workspaces SET session_count = 0, updated_at = ?1",
            params![now_unix()],
        )?;
        Ok(())
    }

    /// List all known workspaces.
    pub fn list_workspaces(&self) -> Result<Vec<WorkspaceRow>> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare_cached(
            "SELECT workspace_id, path, status, session_count, last_indexed,
                    symbol_count, file_count, embedding_model, vector_count,
                    created_at, updated_at, last_index_duration_ms
             FROM workspaces ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| WorkspaceRow::from_row(row))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Delete a workspace row. Cascades to `codehealth_snapshots` (via `ON DELETE CASCADE`).
    pub fn delete_workspace(&self, workspace_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "DELETE FROM workspaces WHERE workspace_id = ?1",
            params![workspace_id],
        )?;
        Ok(())
    }

    /// Record a workspace cleanup event and trim the log to the newest 50 rows.
    pub fn insert_cleanup_event(
        &self,
        workspace_id: &str,
        path: &str,
        action: &str,
        reason: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "INSERT INTO workspace_cleanup_events (workspace_id, path, action, reason, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![workspace_id, path, action, reason, now_unix()],
        )?;
        conn.execute(
            "DELETE FROM workspace_cleanup_events
             WHERE id NOT IN (
                 SELECT id
                 FROM workspace_cleanup_events
                 ORDER BY timestamp DESC, id DESC
                 LIMIT 50
             )",
            [],
        )?;
        Ok(())
    }

    /// Return recent workspace cleanup events, newest first.
    pub fn list_cleanup_events(&self, limit: u32) -> Result<Vec<WorkspaceCleanupEventRow>> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare_cached(
            "SELECT id, workspace_id, path, action, reason, timestamp
             FROM workspace_cleanup_events
             ORDER BY timestamp DESC, id DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(WorkspaceCleanupEventRow {
                id: row.get(0)?,
                workspace_id: row.get(1)?,
                path: row.get(2)?,
                action: row.get(3)?,
                reason: row.get(4)?,
                timestamp: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Direct connection access for tests only.
    #[cfg(test)]
    pub fn conn_for_test(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    // -------------------------------------------------------------------------
    // Codehealth Snapshots
    // -------------------------------------------------------------------------

    /// Persist a codehealth snapshot for a workspace. Called automatically after
    /// each indexing pass completes (when `daemon_db` is present on the handler).
    pub fn insert_codehealth_snapshot(
        &self,
        workspace_id: &str,
        snapshot: &CodehealthSnapshot,
    ) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Lock: {e}"))?;
        conn.execute(
            "INSERT INTO codehealth_snapshots (workspace_id, timestamp, total_symbols, total_files)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                workspace_id,
                now_unix(),
                snapshot.total_symbols,
                snapshot.total_files
            ],
        )?;
        Ok(())
    }

    /// Retrieve the most recently inserted snapshot for a workspace, or `None`.
    pub fn get_latest_snapshot(&self, workspace_id: &str) -> Result<Option<CodehealthSnapshotRow>> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare_cached(
            "SELECT id, workspace_id, timestamp, total_symbols, total_files
             FROM codehealth_snapshots WHERE workspace_id = ?1
             ORDER BY timestamp DESC LIMIT 1",
        )?;
        let mut rows = stmt.query(params![workspace_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(CodehealthSnapshotRow::from_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// Retrieve the N most recent snapshots for a workspace, newest first.
    pub fn get_snapshot_history(
        &self,
        workspace_id: &str,
        limit: u32,
    ) -> Result<Vec<CodehealthSnapshotRow>> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare_cached(
            "SELECT id, workspace_id, timestamp, total_symbols, total_files
             FROM codehealth_snapshots WHERE workspace_id = ?1
             ORDER BY timestamp DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![workspace_id, limit as i64], |row| {
            CodehealthSnapshotRow::from_row(row)
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Query aggregate codehealth metrics from a symbols database and store
    /// a snapshot. Called automatically after each indexing pass completes.
    ///
    /// LOCK ORDERING: callers must acquire `symbol_db` lock before calling this
    /// function, which then acquires the internal `DaemonDatabase` lock. Always
    /// lock symbol_db first, then daemon_db — never in the reverse order.
    pub fn snapshot_codehealth_from_db(
        &self,
        workspace_id: &str,
        symbols_db: &crate::database::SymbolDatabase,
    ) -> Result<()> {
        let conn = &symbols_db.conn;

        let total_symbols: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM symbols WHERE kind NOT IN ('import', 'export') \
                 AND (content_type IS NULL OR content_type != 'documentation')",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let total_files: i64 = conn
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
            .unwrap_or(0);

        let snapshot = CodehealthSnapshot {
            total_symbols,
            total_files,
        };

        self.insert_codehealth_snapshot(workspace_id, &snapshot)
    }

    // -------------------------------------------------------------------------
    // Workspace ID Migration
    // -------------------------------------------------------------------------

    /// Batch-migrate workspace IDs across all tables.
    ///
    /// Given a map of old_id -> new_id, updates workspace_cleanup_events,
    /// codehealth_snapshots, tool_calls, and workspaces in a single transaction.
    /// FK checks are temporarily disabled to allow PK updates.
    pub fn migrate_workspace_ids(
        &self,
        id_map: &std::collections::HashMap<String, String>,
    ) -> Result<()> {
        if id_map.is_empty() {
            return Ok(());
        }

        let mut conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute_batch("PRAGMA foreign_keys = OFF;")?;

        // Scope guard: ensure FK enforcement is restored on ALL exit paths.
        // Without this, an early `?` return would leave FKs disabled for
        // all future callers sharing this connection.
        let result = (|| -> Result<()> {
            let tx = conn.transaction()?;

            for (old_id, new_id) in id_map {
                if old_id == new_id {
                    continue;
                }

                let old_exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM workspaces WHERE workspace_id = ?1)",
                    params![old_id],
                    |row| row.get::<_, i64>(0).map(|value| value != 0),
                )?;
                let new_exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM workspaces WHERE workspace_id = ?1)",
                    params![new_id],
                    |row| row.get::<_, i64>(0).map(|value| value != 0),
                )?;

                // Update child tables first
                tx.execute(
                    "UPDATE workspace_cleanup_events SET workspace_id = ?1
                     WHERE workspace_id = ?2",
                    params![new_id, old_id],
                )?;
                tx.execute(
                    "UPDATE codehealth_snapshots SET workspace_id = ?1
                     WHERE workspace_id = ?2",
                    params![new_id, old_id],
                )?;
                tx.execute(
                    "UPDATE tool_calls SET workspace_id = ?1
                     WHERE workspace_id = ?2",
                    params![new_id, old_id],
                )?;

                if !old_exists {
                    continue;
                }

                if new_exists {
                    tx.execute(
                        "UPDATE workspaces
                         SET status = CASE
                                 WHEN status = 'ready' THEN status
                                 WHEN (SELECT status FROM workspaces WHERE workspace_id = ?2) = 'ready'
                                     THEN 'ready'
                                 ELSE status
                             END,
                             session_count = MAX(
                                 session_count,
                                 (SELECT session_count FROM workspaces WHERE workspace_id = ?2)
                             ),
                             last_indexed = COALESCE(
                                 last_indexed,
                                 (SELECT last_indexed FROM workspaces WHERE workspace_id = ?2)
                             ),
                             symbol_count = COALESCE(
                                 symbol_count,
                                 (SELECT symbol_count FROM workspaces WHERE workspace_id = ?2)
                             ),
                             file_count = COALESCE(
                                 file_count,
                                 (SELECT file_count FROM workspaces WHERE workspace_id = ?2)
                             ),
                             embedding_model = COALESCE(
                                 embedding_model,
                                 (SELECT embedding_model FROM workspaces WHERE workspace_id = ?2)
                             ),
                             vector_count = COALESCE(
                                 vector_count,
                                 (SELECT vector_count FROM workspaces WHERE workspace_id = ?2)
                             ),
                             created_at = MIN(
                                 created_at,
                                 (SELECT created_at FROM workspaces WHERE workspace_id = ?2)
                             ),
                             updated_at = MAX(
                                 updated_at,
                                 (SELECT updated_at FROM workspaces WHERE workspace_id = ?2)
                             ),
                             last_index_duration_ms = COALESCE(
                                 last_index_duration_ms,
                                 (SELECT last_index_duration_ms FROM workspaces WHERE workspace_id = ?2)
                             )
                         WHERE workspace_id = ?1",
                        params![new_id, old_id],
                    )?;
                    tx.execute(
                        "DELETE FROM workspaces WHERE workspace_id = ?1",
                        params![old_id],
                    )?;
                } else {
                    // Update workspace row itself (PK change)
                    tx.execute(
                        "UPDATE workspaces SET workspace_id = ?1
                         WHERE workspace_id = ?2",
                        params![new_id, old_id],
                    )?;
                }
            }

            // Verify FK integrity before committing
            let violations: i64 =
                tx.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get(0)
                })?;
            if violations > 0 {
                anyhow::bail!(
                    "FK integrity check failed after migration ({violations} violations)"
                );
            }

            tx.commit()?;
            Ok(())
        })();

        // ALWAYS re-enable FK enforcement, even if the transaction failed
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        result
    }
}

// -----------------------------------------------------------------------------
// Row types
// -----------------------------------------------------------------------------

/// A row from the `workspaces` table.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkspaceRow {
    pub workspace_id: String,
    pub path: String,
    pub status: String,
    pub session_count: i64,
    pub last_indexed: Option<i64>,
    pub symbol_count: Option<i64>,
    pub file_count: Option<i64>,
    pub embedding_model: Option<String>,
    pub vector_count: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_index_duration_ms: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkspaceCleanupEventRow {
    pub id: i64,
    pub workspace_id: String,
    pub path: String,
    pub action: String,
    pub reason: String,
    pub timestamp: i64,
}

impl WorkspaceRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            workspace_id: row.get(0)?,
            path: row.get(1)?,
            status: row.get(2)?,
            session_count: row.get(3)?,
            last_indexed: row.get(4)?,
            symbol_count: row.get(5)?,
            file_count: row.get(6)?,
            embedding_model: row.get(7)?,
            vector_count: row.get(8)?,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
            last_index_duration_ms: row.get(11).unwrap_or(None),
        })
    }
}

/// Passed to `DaemonDatabase::insert_codehealth_snapshot`. Only tracks
/// symbol and file counts now that risk/coverage metrics are shelved.
#[derive(Debug, Clone, Default)]
pub struct CodehealthSnapshot {
    pub total_symbols: i64,
    pub total_files: i64,
}

/// A row from the `codehealth_snapshots` table. Only reads symbol/file
/// counts; legacy risk columns remain in the schema but are ignored.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CodehealthSnapshotRow {
    pub id: i64,
    pub workspace_id: String,
    pub timestamp: i64,
    pub total_symbols: i64,
    pub total_files: i64,
}

impl CodehealthSnapshotRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            workspace_id: row.get(1)?,
            timestamp: row.get(2)?,
            total_symbols: row.get(3)?,
            total_files: row.get(4)?,
        })
    }
}

// -----------------------------------------------------------------------------
// Utility
// -----------------------------------------------------------------------------

fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
