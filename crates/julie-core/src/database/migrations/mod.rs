// Schema migration system for database versioning

use super::*;
use anyhow::{Result, anyhow};
use rusqlite::params;
use tracing::{debug, info};

mod embedding_generation;
mod receiver_type;
mod v1_to_v8;
mod v21_to_v30;
mod v9_to_v20;

/// Current schema version - increment when adding migrations
pub const LATEST_SCHEMA_VERSION: i32 = 32;

fn get_unix_timestamp() -> Result<i64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .map_err(|e| anyhow!("System time error: {}", e))
}

impl SymbolDatabase {
    // ============================================================
    // SCHEMA MIGRATION SYSTEM
    // ============================================================

    /// Run all pending schema migrations
    pub(super) fn run_migrations(&mut self) -> Result<()> {
        // Create schema_version table if it doesn't exist
        self.create_schema_version_table()?;

        let current_version = self.get_schema_version()?;
        let target_version = LATEST_SCHEMA_VERSION;

        if current_version >= target_version {
            debug!(
                "Database schema is up-to-date at version {}",
                current_version
            );
            return Ok(());
        }

        info!(
            "Running schema migrations: version {} -> {}",
            current_version, target_version
        );

        // Run migrations sequentially
        for version in (current_version + 1)..=target_version {
            info!("Applying migration to version {}", version);
            self.apply_migration(version)?;
            self.record_migration(version)?;
            info!("✅ Migration to version {} completed", version);
        }

        Ok(())
    }

    /// Create the schema_version table
    fn create_schema_version_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                applied_at INTEGER NOT NULL,
                description TEXT NOT NULL
            )",
            [],
        )?;
        Ok(())
    }

    /// Get the current schema version
    pub fn get_schema_version(&self) -> Result<i32> {
        // Check if schema_version table exists
        let table_exists: bool = self.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type='table' AND name='schema_version'",
            [],
            |row| {
                let count: i32 = row.get(0)?;
                Ok(count > 0)
            },
        )?;

        if !table_exists {
            // Fresh database - will be at latest version after init
            return Ok(0);
        }

        // Get the latest migration version
        let version: Result<i32, rusqlite::Error> = self.conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        );

        Ok(version.unwrap_or(0))
    }

    /// Apply a specific migration
    fn apply_migration(&mut self, version: i32) -> Result<()> {
        match version {
            1 => self.migration_001_initial_schema()?,
            2 => self.migration_002_add_content_column()?,
            3 => self.migration_003_add_relationship_location()?,
            4 => self.migration_004_add_content_type()?,
            5 => self.migration_005_add_fts_prefix_indexes()?,
            6 => self.migration_006_add_types_table()?,
            7 => self.migration_007_drop_fts5()?,
            8 => self.migration_008_drop_embedding_tables()?,
            9 => self.migration_009_add_reference_score()?,
            10 => self.migration_010_add_symbol_vectors()?,
            11 => self.migration_011_add_embedding_config()?,
            12 => self.migration_012_add_memory_vectors()?,
            13 => self.migration_013_add_tool_calls_and_line_count()?,
            14 => self.migration_014_add_embedding_format_version()?,
            15 => self.migration_015_add_indexing_repairs()?,
            16 => self.migration_016_add_canonical_revisions()?,
            17 => self.migration_017_add_projection_states()?,
            18 => self.migration_018_add_projected_revision_to_projection_states()?,
            19 => self.migration_019_add_revision_file_changes()?,
            20 => self.migration_020_add_symbol_annotations()?,
            21 => self.migration_021_add_early_warning_reports()?,
            22 => self.migration_022_add_sql_performance_indexes()?,
            23 => self.migration_023_add_tool_call_input_bytes()?,
            24 => self.migration_024_add_index_engine_state()?,
            25 => self.migration_025_add_symbol_body_fields()?,
            26 => self.migration_026_add_external_extract_metadata()?,
            27 => self.migration_027_add_type_arguments()?,
            28 => self.migration_028_add_literals()?,
            29 => self.migration_029_add_extractor_enrichments()?,
            30 => self.migration_030_add_web_edges()?,
            31 => self.migration_031_add_identifier_receiver_type()?,
            32 => self.migration_032_add_embedding_generations()?,
            _ => return Err(anyhow!("Unknown migration version: {}", version)),
        }
        Ok(())
    }

    /// Record a completed migration
    fn record_migration(&self, version: i32) -> Result<()> {
        let description = match version {
            1 => "Initial schema",
            2 => "Add content column for CASCADE FTS5",
            3 => "Add file_path and line_number to relationships",
            4 => "Add content_type field to symbols for documentation",
            5 => "Add FTS5 prefix indexes for faster wildcard queries",
            6 => "Add types table for type intelligence",
            7 => "Drop FTS5 tables and triggers (replaced by Tantivy)",
            8 => "Drop embedding tables (embedding engine removed)",
            9 => "Add reference_score for graph centrality ranking",
            10 => "Add symbol_vectors virtual table for semantic embeddings",
            11 => "Add embedding config table",
            12 => "Add memory vectors table",
            13 => "Add tool_calls table and line_count column",
            14 => "Add format_version to embedding_config",
            15 => "Add indexing_repairs table",
            16 => "Add canonical_revisions table",
            17 => "Add projection_states table",
            18 => "Add projected_revision to projection_states",
            19 => "Add revision_file_changes table",
            20 => "Add symbol_annotations table",
            21 => "Add early_warning_reports table",
            22 => "Add SQL performance indexes",
            23 => "Add input_bytes to tool_calls",
            24 => "Add index_engine_state table",
            25 => "Add symbol body span and hash columns",
            26 => "Add external extract metadata table",
            27 => "Add type_arguments table",
            28 => "Add literals table",
            29 => "Add extractor enrichment tables",
            30 => "Add web_edges table for derived web navigation edges",
            31 => "Add receiver_type column to identifiers table",
            32 => "Add embedding_generations table for model and vector compatibility",
            _ => "Unknown migration",
        };

        self.conn.execute(
            "INSERT OR REPLACE INTO schema_version (version, applied_at, description)
             VALUES (?, ?, ?)",
            params![version, get_unix_timestamp()?, description],
        )?;

        Ok(())
    }

    /// Helper: Check if a column exists in a table
    pub fn has_column(&self, table: &str, column: &str) -> Result<bool> {
        // Validate table name to prevent SQL injection via PRAGMA interpolation.
        // PRAGMA table_info() does not support parameter binding in rusqlite.
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

    pub(crate) fn table_exists(&self, table: &str) -> Result<bool> {
        self.conn
            .query_row(
                "SELECT EXISTS(
                    SELECT 1
                    FROM sqlite_master
                    WHERE type = 'table' AND name = ?1
                )",
                [table],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }
}
