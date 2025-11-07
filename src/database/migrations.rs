// Schema migration system for database versioning

use super::*;
use anyhow::{anyhow, Result};
use rusqlite::params;
use tracing::{debug, info, warn};

/// Current schema version - increment when adding migrations
pub const LATEST_SCHEMA_VERSION: i32 = 4;

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
            _ => "Unknown migration",
        };

        self.conn.execute(
            "INSERT OR REPLACE INTO schema_version (version, applied_at, description)
             VALUES (?, ?, ?)",
            params![
                version,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
                description
            ],
        )?;

        Ok(())
    }

    /// Helper: Check if a column exists in a table
    pub fn has_column(&self, table: &str, column: &str) -> Result<bool> {
        let mut stmt = self
            .conn
            .prepare(&format!("PRAGMA table_info({})", table))?;

        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(columns.contains(&column.to_string()))
    }

    // ============================================================
    // INDIVIDUAL MIGRATIONS
    // ============================================================

    /// Migration 001: Initial schema (for tracking purposes)
    /// Note: This is a no-op as the schema is created by initialize_schema
    fn migration_001_initial_schema(&self) -> Result<()> {
        // No-op: Schema is created by initialize_schema()
        // This migration exists only for version tracking
        Ok(())
    }

    /// Migration 002: Add content column to files table for CASCADE FTS5
    fn migration_002_add_content_column(&mut self) -> Result<()> {
        info!("Migration 002: Adding content column to files table");

        // Check if files table exists (fresh database won't have it yet)
        let table_exists: bool = self.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type='table' AND name='files'",
            [],
            |row| {
                let count: i32 = row.get(0)?;
                Ok(count > 0)
            },
        )?;

        if !table_exists {
            debug!("Files table doesn't exist yet (fresh database), skipping migration");
            return Ok(());
        }

        // Check if column already exists (idempotency)
        if self.has_column("files", "content")? {
            warn!("Content column already exists, skipping migration");
            return Ok(());
        }

        // Drop existing FTS triggers that reference the content column
        self.conn.execute("DROP TRIGGER IF EXISTS files_ai", [])?;
        self.conn.execute("DROP TRIGGER IF EXISTS files_ad", [])?;
        self.conn.execute("DROP TRIGGER IF EXISTS files_au", [])?;

        // Add the content column
        self.conn
            .execute("ALTER TABLE files ADD COLUMN content TEXT", [])?;

        info!("✅ Content column added to files table");

        // Recreate FTS table and triggers (will be done by initialize_schema)
        // Note: We let initialize_schema handle this to avoid duplication

        Ok(())
    }

    /// Migration 003: Add file_path and line_number to relationships table
    fn migration_003_add_relationship_location(&mut self) -> Result<()> {
        info!("Migration 003: Adding file_path and line_number to relationships table");

        // Check if relationships table exists (fresh database won't have it yet)
        let table_exists: bool = self.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type='table' AND name='relationships'",
            [],
            |row| {
                let count: i32 = row.get(0)?;
                Ok(count > 0)
            },
        )?;

        if !table_exists {
            debug!("Relationships table doesn't exist yet (fresh database), skipping migration");
            return Ok(());
        }

        // Check if file_path column already exists (idempotency)
        if self.has_column("relationships", "file_path")? {
            warn!("file_path column already exists in relationships table, skipping migration");
            return Ok(());
        }

        // Add file_path column (TEXT, empty string default for existing rows)
        self.conn.execute(
            "ALTER TABLE relationships ADD COLUMN file_path TEXT NOT NULL DEFAULT ''",
            [],
        )?;

        // Add line_number column (INTEGER, 0 default for existing rows)
        self.conn.execute(
            "ALTER TABLE relationships ADD COLUMN line_number INTEGER NOT NULL DEFAULT 0",
            [],
        )?;

        info!("✅ file_path and line_number columns added to relationships table");

        Ok(())
    }

    /// Migration 004: Add content_type field to symbols table for documentation
    /// This allows distinguishing documentation (markdown) from code symbols
    fn migration_004_add_content_type(&mut self) -> Result<()> {
        info!("Migration 004: Adding content_type field to symbols table");

        // Check if symbols table exists (should always exist)
        let table_exists: bool = self.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type='table' AND name='symbols'",
            [],
            |row| {
                let count: i32 = row.get(0)?;
                Ok(count > 0)
            },
        )?;

        if !table_exists {
            debug!("Symbols table doesn't exist yet (fresh database), skipping migration");
            return Ok(());
        }

        // Check if content_type column already exists (idempotency)
        if self.has_column("symbols", "content_type")? {
            warn!("content_type column already exists in symbols table, skipping migration");
            return Ok(());
        }

        // Add content_type column (TEXT, NULL default for existing code symbols)
        // NULL = code (default), 'documentation' = markdown docs
        self.conn.execute(
            "ALTER TABLE symbols ADD COLUMN content_type TEXT DEFAULT NULL",
            [],
        )?;

        // Update existing markdown symbols to have content_type = 'documentation'
        self.conn.execute(
            "UPDATE symbols SET content_type = 'documentation' WHERE language = 'markdown'",
            [],
        )?;

        let updated_count = self.conn.changes();
        info!("✅ content_type column added to symbols table, {} markdown symbols marked as documentation", updated_count);

        Ok(())
    }
}
