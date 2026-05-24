use anyhow::Result;
use rusqlite::params;

use crate::database::SymbolDatabase;

use super::{DaemonDatabase, now_unix};

impl DaemonDatabase {
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
            params![
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
        symbols_db: &SymbolDatabase,
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
