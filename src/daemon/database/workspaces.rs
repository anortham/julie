use anyhow::Result;
use rusqlite::params;

use super::{DaemonDatabase, now_unix};

impl DaemonDatabase {
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
        let rows = stmt.query_map([], WorkspaceRow::from_row)?;
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
}

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
