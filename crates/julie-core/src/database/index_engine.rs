use anyhow::{Result, anyhow};
use rusqlite::{OptionalExtension, params};

use super::SymbolDatabase;

fn get_unix_timestamp() -> Result<i64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .map_err(|e| anyhow!("System time error: {}", e))
}

impl SymbolDatabase {
    pub fn create_index_engine_state_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS index_engine_state (
                workspace_id TEXT NOT NULL,
                component TEXT NOT NULL,
                version TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (workspace_id, component)
            );
            CREATE INDEX IF NOT EXISTS idx_index_engine_state_component
            ON index_engine_state(component);",
        )?;
        Ok(())
    }

    pub fn get_index_engine_version(
        &self,
        workspace_id: &str,
        component: &str,
    ) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT version
                 FROM index_engine_state
                 WHERE workspace_id = ?1 AND component = ?2",
                params![workspace_id, component],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn index_engine_version_matches(
        &self,
        workspace_id: &str,
        component: &str,
        expected_version: &str,
    ) -> Result<bool> {
        Ok(self
            .get_index_engine_version(workspace_id, component)?
            .as_deref()
            == Some(expected_version))
    }

    pub fn set_index_engine_version(
        &self,
        workspace_id: &str,
        component: &str,
        version: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO index_engine_state (workspace_id, component, version, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(workspace_id, component) DO UPDATE SET
                 version = excluded.version,
                 updated_at = excluded.updated_at",
            params![workspace_id, component, version, get_unix_timestamp()?],
        )?;
        Ok(())
    }
    /// Highest version recorded in `schema_version`; 0 when the table is absent.
    pub fn get_schema_version(&self) -> Result<i32> {
        match self.conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        ) {
            Ok(version) => Ok(version),
            Err(rusqlite::Error::SqliteFailure(_, Some(message)))
                if message.contains("no such table") =>
            {
                Ok(0)
            }
            Err(err) => Err(err.into()),
        }
    }

    pub fn has_column(&self, table: &str, column: &str) -> Result<bool> {
        assert!(
            table.chars().all(|c| c.is_alphanumeric() || c == '_'),
            "has_column: table name must contain only alphanumeric chars and underscores: {:?}",
            table
        );
        let mut stmt = self
            .conn
            .prepare(&format!("PRAGMA table_info({})", table))?;
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(columns.contains(&column.to_string()))
    }
}
