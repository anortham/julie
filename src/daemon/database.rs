//! Persistent daemon state: workspace registry, codehealth snapshots, tool call history.
//!
//! `DaemonDatabase` wraps a single SQLite connection to `~/.julie/daemon.db`.
//! It is shared across all sessions as `Arc<DaemonDatabase>`. The internal
//! `Mutex<Connection>` makes it safe to call from multiple tokio tasks.

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::Path;
use tracing::{info, warn};

use crate::database::{HistorySummary, ToolCallSummary};


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

        if current < 1 {
            Self::migration_001_initial_schema(conn)?;
        }
        if current < 2 {
            Self::migration_002_add_index_duration(conn)?;
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

    fn migration_002_add_index_duration(conn: &mut Connection) -> Result<()> {
        info!("daemon.db migration 002: add index duration column");
        let tx = conn.transaction()?;
        tx.execute_batch(
            "ALTER TABLE workspaces ADD COLUMN last_index_duration_ms INTEGER;
             INSERT OR REPLACE INTO schema_version (version, applied_at)
             VALUES (2, unixepoch());",
        )?;
        tx.commit()?;
        info!("daemon.db migration 002 complete");
        Ok(())
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

    /// Normalize path separators to the platform-native format for all workspaces.
    ///
    /// Fixes paths stored with forward slashes by the adapter's previous
    /// `.replace('\\', "/")` normalization. Also restores "ready" status for
    /// workspaces that have stats (were previously indexed) but are stuck at
    /// "pending" due to the early-return bug.
    pub fn normalize_workspace_paths(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let now = now_unix();

        // On Windows, convert forward slashes to backslashes.
        // On Unix this is a no-op (paths should already use forward slashes).
        if !cfg!(windows) {
            return Ok(0);
        }

        let mut count = 0;
        let mut stmt = conn.prepare(
            "SELECT workspace_id, path, status, symbol_count FROM workspaces",
        )?;
        let rows: Vec<(String, String, String, Option<i64>)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);

        for (workspace_id, path, status, symbol_count) in &rows {
            let native_path = path.replace('/', "\\");
            let needs_path_fix = native_path != *path;
            // Restore "ready" for workspaces that were indexed but stuck at "pending"
            let needs_status_fix = *status == "pending"
                && symbol_count.unwrap_or(0) > 0;

            if needs_path_fix || needs_status_fix {
                let new_status = if needs_status_fix { "ready" } else { status.as_str() };
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
                 embedding_model = ?3,
                 vector_count    = ?4,
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

    /// Delete a workspace row. Cascades to `workspace_references` and
    /// `codehealth_snapshots` (via `ON DELETE CASCADE`).
    pub fn delete_workspace(&self, workspace_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
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
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
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
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "DELETE FROM workspace_references
             WHERE primary_workspace_id = ?1 AND reference_workspace_id = ?2",
            params![primary_id, reference_id],
        )?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Tool Calls
    // -------------------------------------------------------------------------

    /// Insert one tool call record. `workspace_id` is the primary workspace for
    /// the session that made the call.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_tool_call(
        &self,
        workspace_id: &str,
        session_id: &str,
        tool_name: &str,
        duration_ms: f64,
        result_count: Option<u32>,
        source_bytes: Option<u64>,
        output_bytes: Option<u64>,
        success: bool,
        metadata: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "INSERT INTO tool_calls
                (workspace_id, session_id, timestamp, tool_name, duration_ms,
                 result_count, source_bytes, output_bytes, success, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                workspace_id,
                session_id,
                now_unix(),
                tool_name,
                duration_ms,
                result_count.map(|v| v as i64),
                source_bytes.map(|v| v as i64),
                output_bytes.map(|v| v as i64),
                if success { 1 } else { 0 },
                metadata,
            ],
        )?;
        Ok(())
    }

    /// Query aggregated tool call history for a workspace over the last `days` days.
    pub fn query_tool_call_history(&self, workspace_id: &str, days: u32) -> Result<HistorySummary> {
        use std::collections::HashMap;

        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let cutoff = now_unix() - (days as i64 * 86400);

        let session_count: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT session_id) FROM tool_calls
             WHERE workspace_id = ?1 AND timestamp >= ?2",
            params![workspace_id, cutoff],
            |row| row.get(0),
        )?;

        let total_calls: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tool_calls
             WHERE workspace_id = ?1 AND timestamp >= ?2",
            params![workspace_id, cutoff],
            |row| row.get(0),
        )?;

        let (total_source, total_output): (i64, i64) = conn.query_row(
            "SELECT COALESCE(SUM(source_bytes), 0), COALESCE(SUM(output_bytes), 0)
             FROM tool_calls WHERE workspace_id = ?1 AND timestamp >= ?2",
            params![workspace_id, cutoff],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let mut stmt = conn.prepare(
            "SELECT tool_name, COUNT(*), AVG(duration_ms),
                    COALESCE(SUM(source_bytes), 0), COALESCE(SUM(output_bytes), 0)
             FROM tool_calls WHERE workspace_id = ?1 AND timestamp >= ?2
             GROUP BY tool_name ORDER BY COUNT(*) DESC",
        )?;
        let per_tool = stmt
            .query_map(params![workspace_id, cutoff], |row| {
                Ok(ToolCallSummary {
                    tool_name: row.get(0)?,
                    call_count: row.get::<_, i64>(1)? as u64,
                    avg_duration_ms: row.get(2)?,
                    total_source_bytes: row.get::<_, i64>(3)? as u64,
                    total_output_bytes: row.get::<_, i64>(4)? as u64,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut dur_stmt = conn.prepare(
            "SELECT tool_name, duration_ms FROM tool_calls
             WHERE workspace_id = ?1 AND timestamp >= ?2
             ORDER BY tool_name",
        )?;
        let mut durations_by_tool: HashMap<String, Vec<f64>> = HashMap::new();
        let rows = dur_stmt.query_map(params![workspace_id, cutoff], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?;
        for row in rows {
            let (name, dur) = row?;
            durations_by_tool.entry(name).or_default().push(dur);
        }

        Ok(HistorySummary {
            session_count: session_count as u64,
            total_calls: total_calls as u64,
            total_source_bytes: total_source as u64,
            total_output_bytes: total_output as u64,
            per_tool,
            durations_by_tool,
        })
    }

    /// Delete tool call records older than `retention_days`. Called on daemon startup.
    pub fn prune_tool_calls(&self, retention_days: u32) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let cutoff = now_unix() - (retention_days as i64 * 86400);
        conn.execute(
            "DELETE FROM tool_calls WHERE timestamp < ?1",
            params![cutoff],
        )?;
        Ok(())
    }

    /// Direct connection access for tests only.
    #[cfg(test)]
    pub fn conn_for_test(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    // -------------------------------------------------------------------------
    // Workspace References CRUD (continued below)
    // -------------------------------------------------------------------------

    /// List all reference workspaces for a given primary workspace, returning
    /// their full `WorkspaceRow` data (JOIN with workspaces table).
    pub fn list_references(&self, primary_id: &str) -> Result<Vec<WorkspaceRow>> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare_cached(
            "SELECT w.workspace_id, w.path, w.status, w.session_count, w.last_indexed,
                    w.symbol_count, w.file_count, w.embedding_model, w.vector_count,
                    w.created_at, w.updated_at, w.last_index_duration_ms
             FROM workspace_references r
             JOIN workspaces w ON w.workspace_id = r.reference_workspace_id
             WHERE r.primary_workspace_id = ?1
             ORDER BY r.added_at",
        )?;
        let rows = stmt.query_map(params![primary_id], |row| WorkspaceRow::from_row(row))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
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
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "INSERT INTO codehealth_snapshots
                (workspace_id, timestamp, total_symbols, total_files,
                 security_high, security_medium, security_low,
                 change_high, change_medium, change_low,
                 symbols_tested, symbols_untested,
                 avg_centrality, max_centrality)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                workspace_id,
                now_unix(),
                snapshot.total_symbols,
                snapshot.total_files,
                snapshot.security_high,
                snapshot.security_medium,
                snapshot.security_low,
                snapshot.change_high,
                snapshot.change_medium,
                snapshot.change_low,
                snapshot.symbols_tested,
                snapshot.symbols_untested,
                snapshot.avg_centrality,
                snapshot.max_centrality,
            ],
        )?;
        Ok(())
    }

    /// Retrieve the most recently inserted snapshot for a workspace, or `None`.
    pub fn get_latest_snapshot(&self, workspace_id: &str) -> Result<Option<CodehealthSnapshotRow>> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare_cached(
            "SELECT id, workspace_id, timestamp, total_symbols, total_files,
                    security_high, security_medium, security_low,
                    change_high, change_medium, change_low,
                    symbols_tested, symbols_untested,
                    avg_centrality, max_centrality
             FROM codehealth_snapshots
             WHERE workspace_id = ?1
             ORDER BY timestamp DESC
             LIMIT 1",
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
            "SELECT id, workspace_id, timestamp, total_symbols, total_files,
                    security_high, security_medium, security_low,
                    change_high, change_medium, change_low,
                    symbols_tested, symbols_untested,
                    avg_centrality, max_centrality
             FROM codehealth_snapshots
             WHERE workspace_id = ?1
             ORDER BY timestamp DESC
             LIMIT ?2",
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

        let (security_high, security_medium, security_low) = conn
            .query_row(
                "SELECT \
                 COALESCE(SUM(CASE WHEN json_extract(metadata, '$.security_risk.label') = 'HIGH' THEN 1 ELSE 0 END), 0), \
                 COALESCE(SUM(CASE WHEN json_extract(metadata, '$.security_risk.label') = 'MEDIUM' THEN 1 ELSE 0 END), 0), \
                 COALESCE(SUM(CASE WHEN json_extract(metadata, '$.security_risk.label') = 'LOW' THEN 1 ELSE 0 END), 0) \
                 FROM symbols WHERE kind NOT IN ('import', 'export')",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)),
            )
            .unwrap_or((0, 0, 0));

        let (change_high, change_medium, change_low) = conn
            .query_row(
                "SELECT \
                 COALESCE(SUM(CASE WHEN json_extract(metadata, '$.change_risk.label') = 'HIGH' THEN 1 ELSE 0 END), 0), \
                 COALESCE(SUM(CASE WHEN json_extract(metadata, '$.change_risk.label') = 'MEDIUM' THEN 1 ELSE 0 END), 0), \
                 COALESCE(SUM(CASE WHEN json_extract(metadata, '$.change_risk.label') = 'LOW' THEN 1 ELSE 0 END), 0) \
                 FROM symbols WHERE kind NOT IN ('import', 'export')",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)),
            )
            .unwrap_or((0, 0, 0));

        let (symbols_tested, symbols_untested) = conn
            .query_row(
                "SELECT \
                 COALESCE(SUM(CASE WHEN json_extract(metadata, '$.test_coverage.test_count') > 0 THEN 1 ELSE 0 END), 0), \
                 COALESCE(SUM(CASE WHEN (json_extract(metadata, '$.test_coverage.test_count') = 0 \
                              OR json_extract(metadata, '$.test_coverage.test_count') IS NULL) THEN 1 ELSE 0 END), 0) \
                 FROM symbols WHERE kind NOT IN ('import', 'export') \
                 AND (json_extract(metadata, '$.is_test') IS NULL OR json_extract(metadata, '$.is_test') != 1)",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )
            .unwrap_or((0, 0));

        let (avg_centrality, max_centrality) = conn
            .query_row(
                "SELECT AVG(reference_score), MAX(reference_score) FROM symbols \
                 WHERE kind NOT IN ('import', 'export')",
                [],
                |r| Ok((r.get::<_, Option<f64>>(0)?, r.get::<_, Option<f64>>(1)?)),
            )
            .unwrap_or((None, None));

        let snapshot = CodehealthSnapshot {
            total_symbols,
            total_files,
            security_high: security_high as i32,
            security_medium: security_medium as i32,
            security_low: security_low as i32,
            change_high: change_high as i32,
            change_medium: change_medium as i32,
            change_low: change_low as i32,
            symbols_tested,
            symbols_untested,
            avg_centrality,
            max_centrality,
        };

        self.insert_codehealth_snapshot(workspace_id, &snapshot)
    }

    // -------------------------------------------------------------------------
    // Workspace ID Migration
    // -------------------------------------------------------------------------

    /// Batch-migrate workspace IDs across all tables.
    ///
    /// Given a map of old_id -> new_id, updates workspace_references,
    /// codehealth_snapshots, tool_calls, and workspaces in a single transaction.
    /// FK checks are temporarily disabled to allow PK updates.
    pub fn migrate_workspace_ids(&self, id_map: &std::collections::HashMap<String, String>) -> Result<()> {
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
                // Update child tables first
                tx.execute(
                    "UPDATE workspace_references SET primary_workspace_id = ?1
                     WHERE primary_workspace_id = ?2",
                    params![new_id, old_id],
                )?;
                tx.execute(
                    "UPDATE workspace_references SET reference_workspace_id = ?1
                     WHERE reference_workspace_id = ?2",
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
                // Update workspace row itself (PK change)
                tx.execute(
                    "UPDATE workspaces SET workspace_id = ?1
                     WHERE workspace_id = ?2",
                    params![new_id, old_id],
                )?;
            }

            // Verify FK integrity before committing
            let violations: i64 = tx.query_row(
                "SELECT count(*) FROM pragma_foreign_key_check",
                [],
                |row| row.get(0),
            )?;
            if violations > 0 {
                anyhow::bail!("FK integrity check failed after migration ({violations} violations)");
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

/// Metrics captured after a completed indexing pass.
///
/// Passed to `DaemonDatabase::insert_codehealth_snapshot`. All risk counts
/// default to 0 so callers can use struct update syntax (`..Default::default()`).
#[derive(Debug, Clone, Default)]
pub struct CodehealthSnapshot {
    pub total_symbols: i64,
    pub total_files: i64,
    pub security_high: i32,
    pub security_medium: i32,
    pub security_low: i32,
    pub change_high: i32,
    pub change_medium: i32,
    pub change_low: i32,
    pub symbols_tested: i64,
    pub symbols_untested: i64,
    pub avg_centrality: Option<f64>,
    pub max_centrality: Option<f64>,
}

/// A row from the `codehealth_snapshots` table.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CodehealthSnapshotRow {
    pub id: i64,
    pub workspace_id: String,
    pub timestamp: i64,
    pub total_symbols: i64,
    pub total_files: i64,
    pub security_high: i32,
    pub security_medium: i32,
    pub security_low: i32,
    pub change_high: i32,
    pub change_medium: i32,
    pub change_low: i32,
    pub symbols_tested: i64,
    pub symbols_untested: i64,
    pub avg_centrality: Option<f64>,
    pub max_centrality: Option<f64>,
}

impl CodehealthSnapshotRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            workspace_id: row.get(1)?,
            timestamp: row.get(2)?,
            total_symbols: row.get(3)?,
            total_files: row.get(4)?,
            security_high: row.get(5)?,
            security_medium: row.get(6)?,
            security_low: row.get(7)?,
            change_high: row.get(8)?,
            change_medium: row.get(9)?,
            change_low: row.get(10)?,
            symbols_tested: row.get(11)?,
            symbols_untested: row.get(12)?,
            avg_centrality: row.get(13)?,
            max_centrality: row.get(14)?,
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
