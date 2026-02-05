// Schema migration system for database versioning

use super::*;
use anyhow::{Result, anyhow};
use rusqlite::params;
use tracing::{debug, info, warn};

/// Current schema version - increment when adding migrations
pub const LATEST_SCHEMA_VERSION: i32 = 8;

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
        info!(
            "✅ content_type column added to symbols table, {} markdown symbols marked as documentation",
            updated_count
        );

        Ok(())
    }

    /// Migration 005: Add FTS5 prefix indexes for faster wildcard queries
    ///
    /// Performance improvement: Prefix queries like `auth*` or `getUserData*` will be
    /// 10-100x faster with dedicated prefix indexes.
    ///
    /// This migration:
    /// 1. Drops FTS triggers (to allow table modification)
    /// 2. Drops existing FTS tables
    /// 3. Recreates FTS tables with `prefix='2 3 4 5'` parameter
    /// 4. Recreates triggers
    /// 5. Rebuilds FTS indexes from base tables
    /// 6. Optimizes FTS indexes for better query performance
    fn migration_005_add_fts_prefix_indexes(&mut self) -> Result<()> {
        info!("Running migration 005: Add FTS5 prefix indexes");

        // Check if BOTH base tables exist - if not, skip this migration
        // (initialize_schema will create FTS tables with prefix indexes)
        let files_exists: bool = self.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='files'",
            [],
            |row| row.get::<_, i32>(0).map(|c| c > 0),
        )?;

        let symbols_exists: bool = self.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='symbols'",
            [],
            |row| row.get::<_, i32>(0).map(|c| c > 0),
        )?;

        if !files_exists || !symbols_exists {
            debug!(
                "Skipping migration 005: Base tables don't exist yet (files={}, symbols={})",
                files_exists, symbols_exists
            );
            return Ok(());
        }

        // Step 1: Drop FTS triggers (files)
        self.conn.execute("DROP TRIGGER IF EXISTS files_ai", [])?;
        self.conn.execute("DROP TRIGGER IF EXISTS files_ad", [])?;
        self.conn.execute("DROP TRIGGER IF EXISTS files_au", [])?;
        debug!("Dropped files FTS triggers");

        // Step 2: Drop FTS triggers (symbols)
        self.conn.execute("DROP TRIGGER IF EXISTS symbols_ai", [])?;
        self.conn.execute("DROP TRIGGER IF EXISTS symbols_ad", [])?;
        self.conn.execute("DROP TRIGGER IF EXISTS symbols_au", [])?;
        debug!("Dropped symbols FTS triggers");

        // Step 3: Drop existing FTS tables
        self.conn.execute("DROP TABLE IF EXISTS files_fts", [])?;
        self.conn.execute("DROP TABLE IF EXISTS symbols_fts", [])?;
        debug!("Dropped existing FTS tables");

        // Step 4: Recreate files_fts with prefix indexes
        self.conn.execute(
            r#"CREATE VIRTUAL TABLE files_fts USING fts5(
                path,
                content,
                tokenize = "unicode61 separators '_::->.'",
                prefix='2 3 4 5',
                content='files',
                content_rowid='rowid'
            )"#,
            [],
        )?;
        debug!("Recreated files_fts with prefix indexes");

        // Step 5: Recreate symbols_fts with prefix indexes
        self.conn.execute(
            r#"CREATE VIRTUAL TABLE symbols_fts USING fts5(
                name,
                signature,
                doc_comment,
                code_context,
                tokenize = "unicode61 separators '_::->.'",
                prefix='2 3 4 5',
                content='symbols',
                content_rowid='rowid'
            )"#,
            [],
        )?;
        debug!("Recreated symbols_fts with prefix indexes");

        // Step 6: Recreate files FTS triggers
        self.conn.execute(
            "CREATE TRIGGER files_ai AFTER INSERT ON files BEGIN
                INSERT INTO files_fts(rowid, path, content)
                VALUES (new.rowid, new.path, new.content);
            END",
            [],
        )?;
        self.conn.execute(
            "CREATE TRIGGER files_ad AFTER DELETE ON files BEGIN
                INSERT INTO files_fts(files_fts, rowid, path, content)
                VALUES('delete', old.rowid, old.path, old.content);
            END",
            [],
        )?;
        self.conn.execute(
            "CREATE TRIGGER files_au AFTER UPDATE ON files BEGIN
                INSERT INTO files_fts(files_fts, rowid, path, content)
                VALUES('delete', old.rowid, old.path, old.content);
                INSERT INTO files_fts(rowid, path, content)
                VALUES (new.rowid, new.path, new.content);
            END",
            [],
        )?;
        debug!("Recreated files FTS triggers");

        // Step 7: Recreate symbols FTS triggers
        self.conn.execute(
            "CREATE TRIGGER symbols_ai AFTER INSERT ON symbols BEGIN
                INSERT INTO symbols_fts(rowid, name, signature, doc_comment, code_context)
                VALUES (new.rowid, new.name, new.signature, new.doc_comment, new.code_context);
            END",
            [],
        )?;
        self.conn.execute(
            "CREATE TRIGGER symbols_ad AFTER DELETE ON symbols BEGIN
                INSERT INTO symbols_fts(symbols_fts, rowid, name, signature, doc_comment, code_context)
                VALUES('delete', old.rowid, old.name, old.signature, old.doc_comment, old.code_context);
            END",
            [],
        )?;
        self.conn.execute(
            "CREATE TRIGGER symbols_au AFTER UPDATE ON symbols BEGIN
                INSERT INTO symbols_fts(symbols_fts, rowid, name, signature, doc_comment, code_context)
                VALUES('delete', old.rowid, old.name, old.signature, old.doc_comment, old.code_context);
                INSERT INTO symbols_fts(rowid, name, signature, doc_comment, code_context)
                VALUES (new.rowid, new.name, new.signature, new.doc_comment, new.code_context);
            END",
            [],
        )?;
        debug!("Recreated symbols FTS triggers");

        // Step 8: Rebuild FTS indexes from base tables
        self.conn
            .execute("INSERT INTO files_fts(files_fts) VALUES('rebuild')", [])?;
        self.conn
            .execute("INSERT INTO symbols_fts(symbols_fts) VALUES('rebuild')", [])?;
        debug!("Rebuilt FTS indexes");

        // Step 9: Optimize FTS indexes for better performance
        self.conn
            .execute("INSERT INTO files_fts(files_fts) VALUES('optimize')", [])?;
        self.conn.execute(
            "INSERT INTO symbols_fts(symbols_fts) VALUES('optimize')",
            [],
        )?;
        debug!("Optimized FTS indexes");

        info!("✅ FTS5 prefix indexes added successfully");
        Ok(())
    }

    /// Migration 006: Add types table for type intelligence
    ///
    /// This migration adds support for storing type information extracted from code.
    /// Supports 8 languages: Python, Java, C#, PHP, Kotlin, Dart, Go, C++
    fn migration_006_add_types_table(&self) -> Result<()> {
        info!("Running migration 006: Add types table for type intelligence");

        // Check if types table already exists (idempotent)
        let table_exists: bool = self.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='types'",
            [],
            |row| row.get::<_, i32>(0).map(|c| c > 0),
        )?;

        if table_exists {
            debug!("Types table already exists, skipping migration 006");
            return Ok(());
        }

        // Create types table with schema
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS types (
                -- Primary key: one type per symbol (1:1 relationship)
                symbol_id TEXT PRIMARY KEY REFERENCES symbols(id) ON DELETE CASCADE,

                -- Type information
                resolved_type TEXT NOT NULL,       -- e.g., \"String\", \"Vec<User>\", \"Promise<Data>\"
                generic_params TEXT,               -- JSON array: [\"T\", \"U\"] or NULL
                constraints TEXT,                  -- JSON array: [\"T: Clone\"] or NULL
                is_inferred INTEGER NOT NULL,      -- 0 = explicit, 1 = inferred

                -- Metadata
                language TEXT NOT NULL,            -- Programming language
                metadata TEXT,                     -- JSON object for language-specific data

                -- Infrastructure
                last_indexed INTEGER DEFAULT 0     -- Unix timestamp of last update
            )",
            [],
        )?;
        debug!("Created types table");

        // Create essential indexes
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_types_language ON types(language)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_types_resolved ON types(resolved_type)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_types_inferred ON types(is_inferred)",
            [],
        )?;
        debug!("Created types table indexes");

        info!("✅ Types table created successfully");
        Ok(())
    }

    /// Migration 007: Drop FTS5 tables and triggers (replaced by Tantivy)
    ///
    /// FTS5 virtual tables and their sync triggers are no longer needed.
    /// All full-text search is now handled by Tantivy with CodeTokenizer.
    fn migration_007_drop_fts5(&self) -> Result<()> {
        // Drop FTS5 sync triggers
        for trigger in &[
            "symbols_ai", "symbols_ad", "symbols_au",
            "files_ai", "files_ad", "files_au",
        ] {
            self.conn.execute(
                &format!("DROP TRIGGER IF EXISTS {trigger}"),
                [],
            )?;
        }
        debug!("Dropped FTS5 sync triggers");

        // Drop FTS5 virtual tables
        self.conn.execute("DROP TABLE IF EXISTS symbols_fts", [])?;
        self.conn.execute("DROP TABLE IF EXISTS files_fts", [])?;
        debug!("Dropped FTS5 virtual tables");

        info!("✅ FTS5 tables and triggers removed (replaced by Tantivy)");
        Ok(())
    }

    /// Migration 008: Drop embedding tables (embedding engine removed)
    ///
    /// The embedding storage layer (embeddings + embedding_vectors tables) is dead code.
    /// The embedding engine was removed; all semantic search now uses HNSW vectors
    /// stored externally. Drop the tables to clean up the schema.
    fn migration_008_drop_embedding_tables(&self) -> Result<()> {
        info!("Running migration 008: Drop embedding tables");
        self.conn
            .execute("DROP TABLE IF EXISTS embedding_vectors", [])?;
        self.conn.execute("DROP TABLE IF EXISTS embeddings", [])?;
        debug!("Dropped embedding_vectors and embeddings tables");
        info!("✅ Embedding tables removed (embedding engine removed)");
        Ok(())
    }
}
