// Workspace management operations

use super::revision_changes::{
    RevisionChangeKind, RevisionFileChange, record_revision_file_changes_tx,
    snapshot_file_hashes_tx,
};
use super::*;
use anyhow::Result;
use tracing::info;

impl SymbolDatabase {
    pub fn delete_orphaned_files_atomic(
        &mut self,
        workspace_id: &str,
        orphaned_files: &[String],
    ) -> Result<Option<i64>> {
        if orphaned_files.is_empty() {
            return Ok(None);
        }

        let tx = self.conn.transaction()?;
        let existing_file_hashes = snapshot_file_hashes_tx(&tx, orphaned_files)?;

        for file_path in orphaned_files {
            tx.execute(
                "DELETE FROM symbol_vectors WHERE symbol_id IN (
                    SELECT id FROM symbols WHERE file_path = ?1
                )",
                rusqlite::params![file_path],
            )?;
            tx.execute(
                "DELETE FROM relationships
                 WHERE from_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)
                    OR to_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
                rusqlite::params![file_path],
            )?;
            tx.execute(
                "DELETE FROM identifiers
                 WHERE file_path = ?1
                    OR containing_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
                rusqlite::params![file_path],
            )?;
            tx.execute(
                "DELETE FROM types WHERE symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
                rusqlite::params![file_path],
            )?;
            tx.execute(
                "DELETE FROM indexing_repairs WHERE path = ?1",
                rusqlite::params![file_path],
            )?;
            tx.execute(
                "DELETE FROM symbols WHERE file_path = ?1",
                rusqlite::params![file_path],
            )?;
            tx.execute(
                "DELETE FROM files WHERE path = ?1",
                rusqlite::params![file_path],
            )?;
        }

        let revision = super::revisions::record_canonical_revision_tx(
            &tx,
            workspace_id,
            CanonicalRevisionKind::Incremental,
            orphaned_files.len() as i64,
            0,
            0,
            0,
            0,
            0,
        )?;

        let revision_changes: Vec<RevisionFileChange> = orphaned_files
            .iter()
            .filter_map(|file_path| {
                existing_file_hashes
                    .get(file_path)
                    .map(|old_hash| RevisionFileChange {
                        revision,
                        workspace_id: workspace_id.to_string(),
                        file_path: file_path.clone(),
                        change_kind: RevisionChangeKind::Deleted,
                        old_hash: Some(old_hash.clone()),
                        new_hash: None,
                    })
            })
            .collect();
        record_revision_file_changes_tx(&tx, revision, workspace_id, &revision_changes)?;

        tx.commit()?;
        Ok(Some(revision))
    }

    pub fn delete_workspace_data(&mut self) -> Result<WorkspaceCleanupStats> {
        self.conn.execute_batch("PRAGMA foreign_keys = ON")?;
        let tx = self.conn.transaction()?;

        let symbols_count: i64 =
            tx.query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;
        let relationships_count: i64 =
            tx.query_row("SELECT COUNT(*) FROM relationships", [], |row| row.get(0))?;
        let files_count: i64 = tx.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        let revisions_count: i64 =
            tx.query_row("SELECT COUNT(*) FROM canonical_revisions", [], |row| {
                row.get(0)
            })?;
        let projections_count: i64 =
            tx.query_row("SELECT COUNT(*) FROM projection_states", [], |row| {
                row.get(0)
            })?;

        // Explicit deletes for every workspace-owned table — don't trust FK cascade alone
        // because foreign_keys pragma state is per-connection. Order is dependent-first.
        tx.execute("DELETE FROM symbol_vectors", [])?;
        tx.execute("DELETE FROM identifiers", [])?;
        tx.execute("DELETE FROM types", [])?;
        tx.execute("DELETE FROM relationships", [])?;
        tx.execute("DELETE FROM symbols", [])?;
        tx.execute("DELETE FROM files", [])?;
        tx.execute("DELETE FROM indexing_repairs", [])?;
        tx.execute("DELETE FROM canonical_revisions", [])?;
        tx.execute("DELETE FROM revision_file_changes", [])?;
        tx.execute("DELETE FROM projection_states", [])?;

        tx.commit()?;

        let stats = WorkspaceCleanupStats {
            symbols_deleted: symbols_count,
            relationships_deleted: relationships_count,
            files_deleted: files_count,
            revisions_deleted: revisions_count,
            projections_deleted: projections_count,
        };

        info!(
            "Deleted workspace data: {} symbols, {} relationships, {} files, {} revisions, {} projections",
            symbols_count, relationships_count, files_count, revisions_count, projections_count
        );

        Ok(stats)
    }

    /// Get workspace usage statistics
    pub fn get_workspace_usage_stats(&self, workspace_id: &str) -> Result<WorkspaceUsageStats> {
        // Use separate COUNT queries to avoid the CROSS JOIN cartesian product bug.
        // A CROSS JOIN of symbols × files produces symbol_count × file_count rows,
        // which inflates both counts and SUM(size) catastrophically on large workspaces.
        let symbol_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;

        let (file_count, total_size_bytes): (i64, i64) = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(size), 0) FROM files",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        Ok(WorkspaceUsageStats {
            workspace_id: workspace_id.to_string(),
            symbol_count,
            file_count,
            total_size_bytes,
            canonical_revision: self.get_current_canonical_revision(workspace_id)?,
        })
    }

    /// Get last activity time for this workspace
    /// LRU eviction logic should be handled at the registry level
    pub fn get_last_activity_time(&self) -> Result<Option<i64>> {
        let result = self.conn.query_row(
            "SELECT MAX(last_modified) as last_activity FROM files",
            [],
            |row| row.get::<_, Option<i64>>(0),
        );

        match result {
            Ok(time) => Ok(time),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow!("Database error: {}", e)),
        }
    }
}
