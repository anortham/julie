// Workspace management operations

use super::*;
use anyhow::Result;
use tracing::info;

impl SymbolDatabase {
    pub fn delete_workspace_data(&mut self) -> Result<WorkspaceCleanupStats> {
        let tx = self.conn.transaction()?;

        // Count data before deletion for reporting
        let symbols_count: i64 =
            tx.query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;

        let relationships_count: i64 =
            tx.query_row("SELECT COUNT(*) FROM relationships", [], |row| row.get(0))?;

        let files_count: i64 = tx.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;

        // Delete all workspace data in proper order (relationships first due to foreign keys)
        tx.execute("DELETE FROM relationships", [])?;
        tx.execute("DELETE FROM symbols", [])?;
        tx.execute("DELETE FROM files", [])?;

        tx.commit()?;

        let stats = WorkspaceCleanupStats {
            symbols_deleted: symbols_count,
            relationships_deleted: relationships_count,
            files_deleted: files_count,
        };

        info!(
            "Deleted workspace data: {} symbols, {} relationships, {} files",
            symbols_count, relationships_count, files_count
        );

        Ok(stats)
    }

    /// Get workspace usage statistics
    pub fn get_workspace_usage_stats(&self, workspace_id: &str) -> Result<WorkspaceUsageStats> {
        // Use separate COUNT queries to avoid the CROSS JOIN cartesian product bug.
        // A CROSS JOIN of symbols × files produces symbol_count × file_count rows,
        // which inflates both counts and SUM(size) catastrophically on large workspaces.
        let symbol_count: i64 =
            self.conn
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
