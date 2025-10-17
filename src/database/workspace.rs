// Workspace management operations

use super::*;
use anyhow::Result;
use rusqlite::params;
use tracing::info;

impl SymbolDatabase {
    pub fn delete_workspace_data(&self, workspace_id: &str) -> Result<WorkspaceCleanupStats> {
        let tx = self.conn.unchecked_transaction()?;

        // Count data before deletion for reporting
        let symbols_count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM symbols WHERE workspace_id = ?1",
            params![workspace_id],
            |row| row.get(0),
        )?;

        let relationships_count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM relationships WHERE workspace_id = ?1",
            params![workspace_id],
            |row| row.get(0),
        )?;

        let files_count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM files WHERE workspace_id = ?1",
            params![workspace_id],
            |row| row.get(0),
        )?;

        // Delete all workspace data in proper order (relationships first due to foreign keys)
        tx.execute(
            "DELETE FROM relationships WHERE workspace_id = ?1",
            params![workspace_id],
        )?;

        tx.execute(
            "DELETE FROM symbols WHERE workspace_id = ?1",
            params![workspace_id],
        )?;

        tx.execute(
            "DELETE FROM files WHERE workspace_id = ?1",
            params![workspace_id],
        )?;

        // Note: We could also delete embeddings, but they might be shared across workspaces
        // For now, leave embeddings and clean them up separately if needed

        tx.commit()?;

        let stats = WorkspaceCleanupStats {
            symbols_deleted: symbols_count,
            relationships_deleted: relationships_count,
            files_deleted: files_count,
        };

        info!(
            "Deleted workspace '{}' data: {} symbols, {} relationships, {} files",
            workspace_id, symbols_count, relationships_count, files_count
        );

        Ok(stats)
    }

    /// Get workspace usage statistics for LRU eviction
    pub fn get_workspace_usage_stats(&self) -> Result<Vec<WorkspaceUsageStats>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                COALESCE(s.workspace_id, f.workspace_id) as workspace_id,
                COUNT(DISTINCT s.id) as symbol_count,
                COUNT(DISTINCT f.path) as file_count,
                SUM(f.size) as total_size_bytes
             FROM symbols s
             FULL OUTER JOIN files f ON s.workspace_id = f.workspace_id
             GROUP BY COALESCE(s.workspace_id, f.workspace_id)
             ORDER BY workspace_id",
        )?;

        let stats_iter = stmt.query_map([], |row| {
            Ok(WorkspaceUsageStats {
                workspace_id: row.get("workspace_id")?,
                symbol_count: row.get("symbol_count").unwrap_or(0),
                file_count: row.get("file_count").unwrap_or(0),
                total_size_bytes: row.get("total_size_bytes").unwrap_or(0),
            })
        })?;

        let mut stats = Vec::new();
        for stat_result in stats_iter {
            stats.push(stat_result?);
        }

        Ok(stats)
    }

    /// Get workspaces ordered by last accessed time (for LRU eviction)
    pub fn get_workspaces_by_lru(&self) -> Result<Vec<String>> {
        // This would need integration with the registry service
        // For now, return workspaces ordered by some heuristic based on file modification times
        let mut stmt = self.conn.prepare(
            "SELECT workspace_id, MAX(last_modified) as last_activity
             FROM files
             GROUP BY workspace_id
             ORDER BY last_activity ASC",
        )?;

        let workspace_iter = stmt.query_map([], |row| row.get::<_, String>("workspace_id"))?;

        let mut workspaces = Vec::new();
        for workspace_result in workspace_iter {
            workspaces.push(workspace_result?);
        }

        Ok(workspaces)
    }
}
