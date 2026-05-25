//! Persistent daemon state: workspace registry, codehealth snapshots, tool call history.
//!
//! `DaemonDatabase` wraps a single SQLite connection to `~/.julie/daemon.db`.
//! It is shared across all sessions as `Arc<DaemonDatabase>`. The internal
//! `Mutex<Connection>` makes it safe to call from multiple tokio tasks.

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::Path;
use tracing::warn;

mod codehealth;
mod migrations;
mod search_compare;
mod tool_calls;
mod workspaces;

pub use codehealth::{CodehealthSnapshot, CodehealthSnapshotRow};
pub use search_compare::{
    SearchCompareCaseInput, SearchCompareCaseRow, SearchCompareRunInput, SearchCompareRunRow,
};
pub use tool_calls::SearchToolCallRow;
pub use workspaces::{WorkspaceCleanupEventRow, WorkspaceRow};

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

    /// Direct connection access for tests only.
    #[cfg(test)]
    pub fn conn_for_test(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
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
// Utility
// -----------------------------------------------------------------------------

fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
