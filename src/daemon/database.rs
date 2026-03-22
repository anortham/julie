//! Persistent daemon state: workspace registry, codehealth snapshots, tool call history.
//!
//! `DaemonDatabase` wraps a single SQLite connection to `~/.julie/daemon.db`.
//! It is shared across all sessions as `Arc<DaemonDatabase>`. The internal
//! `Mutex<Connection>` makes it safe to call from multiple tokio tasks.

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::Path;
use tracing::{info, warn};

const DAEMON_SCHEMA_VERSION: i32 = 1;

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
                Connection::open(path)
                    .with_context(|| format!("Failed to create fresh daemon.db at {}", path.display()))?
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
            let mut conn = db.conn.lock().unwrap();
            Self::run_migrations(&mut conn)?;
        }

        Ok(db)
    }

    fn run_migrations(conn: &mut Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version    INTEGER PRIMARY KEY,
                applied_at INTEGER NOT NULL
            );",
        )?;

        let current: i32 = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )?;

        if current < DAEMON_SCHEMA_VERSION {
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
            CREATE INDEX idx_tool_calls_tool_name  ON tool_calls(tool_name);
            CREATE INDEX idx_tool_calls_session    ON tool_calls(session_id);
            CREATE INDEX idx_tool_calls_workspace  ON tool_calls(workspace_id);

            INSERT INTO schema_version (version, applied_at)
            VALUES (1, unixepoch());",
        )?;

        tx.commit()?;
        info!("daemon.db migration 001 complete");
        Ok(())
    }

    /// Returns true if a table with the given name exists in the database.
    pub fn table_exists(&self, table_name: &str) -> bool {
        let conn = self.conn.lock().unwrap();
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
    /// On conflict with an existing `workspace_id`, the `path`, `status`, and
    /// `updated_at` columns are updated. `session_count` and stats are preserved.
    pub fn upsert_workspace(
        &self,
        workspace_id: &str,
        path: &str,
        status: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = now_unix();
        conn.execute(
            "INSERT INTO workspaces (workspace_id, path, status, session_count,
                created_at, updated_at)
             VALUES (?1, ?2, ?3, 0, ?4, ?4)
             ON CONFLICT(workspace_id) DO UPDATE SET
                path       = excluded.path,
                status     = excluded.status,
                updated_at = excluded.updated_at",
            params![workspace_id, path, status, now],
        )?;
        Ok(())
    }

    /// Get a workspace row by ID, returns `None` if it doesn't exist.
    pub fn get_workspace(&self, workspace_id: &str) -> Result<Option<WorkspaceRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT workspace_id, path, status, session_count, last_indexed,
                    symbol_count, file_count, embedding_model, vector_count,
                    created_at, updated_at
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
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT workspace_id, path, status, session_count, last_indexed,
                    symbol_count, file_count, embedding_model, vector_count,
                    created_at, updated_at
             FROM workspaces WHERE path = ?1",
        )?;
        let mut rows = stmt.query(params![path])?;
        if let Some(row) = rows.next()? {
            Ok(Some(WorkspaceRow::from_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// Update just the `status` column (e.g. `pending` -> `indexing` -> `ready`).
    pub fn update_workspace_status(&self, workspace_id: &str, status: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
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
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = now_unix();
        conn.execute(
            "UPDATE workspaces
             SET symbol_count    = ?1,
                 file_count      = ?2,
                 embedding_model = ?3,
                 vector_count    = ?4,
                 last_indexed    = ?5,
                 updated_at      = ?5
             WHERE workspace_id  = ?6",
            params![symbol_count, file_count, embedding_model, vector_count, now, workspace_id],
        )?;
        Ok(())
    }

    /// Increment `session_count` for a workspace (called when a session attaches).
    pub fn increment_session_count(&self, workspace_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
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
        let conn = self.conn.lock().unwrap();
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
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE workspaces SET session_count = 0, updated_at = ?1",
            params![now_unix()],
        )?;
        Ok(())
    }

    /// List all known workspaces.
    pub fn list_workspaces(&self) -> Result<Vec<WorkspaceRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT workspace_id, path, status, session_count, last_indexed,
                    symbol_count, file_count, embedding_model, vector_count,
                    created_at, updated_at
             FROM workspaces ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| WorkspaceRow::from_row(row))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Delete a workspace row. Cascades to `workspace_references` and
    /// `codehealth_snapshots` (via `ON DELETE CASCADE`).
    pub fn delete_workspace(&self, workspace_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM workspaces WHERE workspace_id = ?1",
            params![workspace_id],
        )?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Workspace References CRUD
    // -------------------------------------------------------------------------

    /// Record that `primary_id` uses `reference_id` as a reference workspace.
    /// Silently ignores duplicate inserts.
    pub fn add_reference(&self, primary_id: &str, reference_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO workspace_references
                (primary_workspace_id, reference_workspace_id, added_at)
             VALUES (?1, ?2, ?3)",
            params![primary_id, reference_id, now_unix()],
        )?;
        Ok(())
    }

    /// Remove a reference relationship.
    pub fn remove_reference(&self, primary_id: &str, reference_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM workspace_references
             WHERE primary_workspace_id = ?1 AND reference_workspace_id = ?2",
            params![primary_id, reference_id],
        )?;
        Ok(())
    }

    /// List all reference workspaces for a given primary workspace, returning
    /// their full `WorkspaceRow` data (JOIN with workspaces table).
    pub fn list_references(&self, primary_id: &str) -> Result<Vec<WorkspaceRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT w.workspace_id, w.path, w.status, w.session_count, w.last_indexed,
                    w.symbol_count, w.file_count, w.embedding_model, w.vector_count,
                    w.created_at, w.updated_at
             FROM workspace_references r
             JOIN workspaces w ON w.workspace_id = r.reference_workspace_id
             WHERE r.primary_workspace_id = ?1
             ORDER BY r.added_at",
        )?;
        let rows = stmt.query_map(params![primary_id], |row| WorkspaceRow::from_row(row))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

// -----------------------------------------------------------------------------
// Row types
// -----------------------------------------------------------------------------

/// A row from the `workspaces` table.
#[derive(Debug, Clone)]
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
