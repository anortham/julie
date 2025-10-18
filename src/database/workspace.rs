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

        // Note: We could also delete embeddings, but they might be shared across workspaces
        // For now, leave embeddings and clean them up separately if needed

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
        let mut stmt = self.conn.prepare(
            "SELECT
                COUNT(DISTINCT s.id) as symbol_count,
                COUNT(DISTINCT f.path) as file_count,
                COALESCE(SUM(f.size), 0) as total_size_bytes
             FROM symbols s
             CROSS JOIN files f",
        )?;

        let stats = stmt.query_row([], |row| {
            Ok(WorkspaceUsageStats {
                workspace_id: workspace_id.to_string(),
                symbol_count: row.get("symbol_count").unwrap_or(0),
                file_count: row.get("file_count").unwrap_or(0),
                total_size_bytes: row.get("total_size_bytes").unwrap_or(0),
            })
        })?;

        Ok(stats)
    }

    /// Get last activity time for this workspace
    /// LRU eviction logic should be handled at the registry level
    pub fn get_last_activity_time(&self) -> Result<Option<i64>> {
        let result = self.conn.query_row(
            "SELECT MAX(last_modified) as last_activity FROM files",
            [],
            |row| row.get(0),
        );

        match result {
            Ok(time) => Ok(Some(time)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow!("Database error: {}", e)),
        }
    }
}
