use anyhow::{Result, anyhow};
use rusqlite::params;
use tracing::debug;

use super::SymbolDatabase;

fn get_unix_timestamp() -> Result<i64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .map_err(|e| anyhow!("System time error: {}", e))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IndexingRepairRecord {
    pub(crate) path: String,
    pub(crate) reason: String,
    pub(crate) detail: Option<String>,
    pub(crate) updated_at: i64,
}

impl SymbolDatabase {
    pub(crate) fn record_indexing_repair(
        &self,
        path: &str,
        reason: &str,
        detail: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO indexing_repairs (path, reason, detail, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![path, reason, detail, get_unix_timestamp()?],
        )?;
        debug!("Recorded indexing repair for {}", path);
        Ok(())
    }

    pub(crate) fn clear_indexing_repair(&self, path: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM indexing_repairs WHERE path = ?1",
            params![path],
        )?;
        Ok(())
    }

    pub(crate) fn clear_indexing_repairs(&self, paths: &[String]) -> Result<()> {
        for path in paths {
            self.clear_indexing_repair(path)?;
        }
        Ok(())
    }

    pub(crate) fn list_indexing_repairs(&self) -> Result<Vec<IndexingRepairRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, reason, detail, updated_at
             FROM indexing_repairs
             ORDER BY updated_at ASC, path ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(IndexingRepairRecord {
                path: row.get(0)?,
                reason: row.get(1)?,
                detail: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }

        Ok(records)
    }
}
