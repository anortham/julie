// Julie's Database Module - SQLite Source of Truth
//!
//! This module provides persistent storage for symbols, relationships, files, and metadata
//! using SQLite as the foundation of Julie's three-pillar architecture.
//!
//! Key features:
//! - Comprehensive schema for cross-language symbol storage
//! - File tracking with Blake3 hashing for incremental updates
//! - Rich relationship mapping for data flow tracing
//! - Efficient indexes for sub-100ms query performance

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use crate::extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};

/// Current schema version - increment when adding migrations
const LATEST_SCHEMA_VERSION: i32 = 3;

/// The main database connection and operations
pub struct SymbolDatabase {
    conn: Connection,
    file_path: PathBuf,
}

/// File tracking information with Blake3 hashing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub language: String,
    pub hash: String, // Blake3 hash
    pub size: i64,
    pub last_modified: i64, // Unix timestamp
    pub last_indexed: i64,  // Unix timestamp
    pub symbol_count: i32,
    /// CASCADE: Full file content for FTS5 search
    pub content: Option<String>,
}

/// Embedding metadata linking symbols to vector store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingInfo {
    pub symbol_id: String,
    pub vector_id: String,
    pub model_name: String,
    pub embedding_hash: String,
    pub created_at: i64,
}

/// File search result from FTS5 queries
#[derive(Debug, Clone)]
pub struct FileSearchResult {
    pub path: String,
    pub snippet: String,
    pub rank: f32,
}

/// Database statistics for health monitoring
#[derive(Debug, Default)]
pub struct DatabaseStats {
    pub total_symbols: i64,
    pub total_relationships: i64,
    pub total_files: i64,
    pub total_embeddings: i64,
    pub languages: Vec<String>,
    pub db_size_mb: f64,
}

impl SymbolDatabase {
    /// Create a new database connection and initialize schema
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let file_path = db_path.as_ref().to_path_buf();

        info!("Initializing SQLite database at: {}", file_path.display());

        let conn =
            Connection::open(&file_path).map_err(|e| anyhow!("Failed to open database: {}", e))?;

        // Set busy timeout for concurrent access - wait up to 5 seconds for locks
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;

        let mut db = Self { conn, file_path };

        // Run schema migrations BEFORE initializing schema
        db.run_migrations()?;

        db.initialize_schema()?;

        info!("Database initialized successfully");
        Ok(db)
    }

    // ============================================================
    // SCHEMA MIGRATION SYSTEM
    // ============================================================

    /// Run all pending schema migrations
    fn run_migrations(&mut self) -> Result<()> {
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

    // ============================================================
    // END MIGRATION SYSTEM
    // ============================================================

    /// Initialize the complete database schema
    fn initialize_schema(&mut self) -> Result<()> {
        debug!("Creating database schema");

        // Enable foreign key constraints
        self.conn.execute("PRAGMA foreign_keys = ON", [])?;

        // Set WAL mode for better concurrency (this returns results, so ignore them)
        self.conn
            .query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;

        // Create tables in dependency order
        self.create_workspaces_table()?;
        self.create_files_table()?;
        self.create_symbols_table()?;
        self.create_identifiers_table()?; // Reference tracking
        self.create_relationships_table()?;
        self.create_embeddings_table()?;

        debug!("Database schema created successfully");
        Ok(())
    }

    /// Create the workspaces table for tracking workspace metadata
    fn create_workspaces_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS workspaces (
                id TEXT PRIMARY KEY,
                path TEXT NOT NULL,
                name TEXT NOT NULL,
                type TEXT NOT NULL CHECK(type IN ('primary', 'reference', 'session')),
                indexed_at INTEGER,
                last_accessed INTEGER,
                expires_at INTEGER,
                file_count INTEGER DEFAULT 0,
                symbol_count INTEGER DEFAULT 0
            )",
            [],
        )?;

        // Indexes for workspace queries
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_workspaces_type ON workspaces(type)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_workspaces_expires ON workspaces(expires_at)",
            [],
        )?;

        debug!("Created workspaces table and indexes");
        Ok(())
    }

    /// Create the files table for tracking source files
    fn create_files_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS files (
                path TEXT PRIMARY KEY,
                language TEXT NOT NULL,
                hash TEXT NOT NULL,
                size INTEGER NOT NULL,
                last_modified INTEGER NOT NULL,
                last_indexed INTEGER DEFAULT 0,
                parse_cache BLOB,
                symbol_count INTEGER DEFAULT 0,
                content TEXT,  -- CASCADE: Full file content for FTS

                -- For multi-workspace support
                workspace_id TEXT NOT NULL DEFAULT 'primary'
            )",
            [],
        )?;

        // Indexes for file queries
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_language ON files(language)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_modified ON files(last_modified)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_workspace ON files(workspace_id)",
            [],
        )?;

        debug!("Created files table and indexes");

        // CASCADE: Create FTS5 table and triggers
        self.create_files_fts_table()?;
        self.create_files_fts_triggers()?;

        Ok(())
    }

    /// CASCADE: Create FTS5 virtual table for full-text search on file content
    fn create_files_fts_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS files_fts USING fts5(
                path,
                content,
                content='files',
                content_rowid='rowid'
            )",
            [],
        )?;
        debug!("Created files_fts virtual table");
        Ok(())
    }

    /// CASCADE: Create triggers to keep FTS5 in sync with files table
    fn create_files_fts_triggers(&self) -> Result<()> {
        // Trigger for INSERT
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS files_ai AFTER INSERT ON files BEGIN
                INSERT INTO files_fts(rowid, path, content)
                VALUES (new.rowid, new.path, new.content);
            END",
            [],
        )?;

        // Trigger for DELETE
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS files_ad AFTER DELETE ON files BEGIN
                DELETE FROM files_fts WHERE rowid = old.rowid;
            END",
            [],
        )?;

        // Trigger for UPDATE
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS files_au AFTER UPDATE ON files BEGIN
                UPDATE files_fts
                SET path = new.path, content = new.content
                WHERE rowid = old.rowid;
            END",
            [],
        )?;

        debug!("Created FTS5 synchronization triggers");
        Ok(())
    }

    /// Create the symbols table with rich metadata
    fn create_symbols_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS symbols (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                language TEXT NOT NULL,
                file_path TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
                signature TEXT,
                start_line INTEGER,
                start_col INTEGER,
                end_line INTEGER,
                end_col INTEGER,
                start_byte INTEGER,
                end_byte INTEGER,
                doc_comment TEXT,
                visibility TEXT,
                code_context TEXT,
                parent_id TEXT REFERENCES symbols(id),
                metadata TEXT,  -- JSON blob

                -- For incremental updates
                file_hash TEXT,
                last_indexed INTEGER DEFAULT 0,

                -- For cross-language linking
                semantic_group TEXT,
                confidence REAL DEFAULT 1.0,

                -- For multi-workspace support
                workspace_id TEXT NOT NULL DEFAULT 'primary'
            )",
            [],
        )?;

        // Essential indexes for fast queries
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols(kind)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_language ON symbols(language)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_path)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_semantic ON symbols(semantic_group)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_parent ON symbols(parent_id)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_workspace ON symbols(workspace_id)",
            [],
        )?;

        debug!("Created symbols table and indexes");

        // CASCADE: Create FTS5 table and triggers for symbols
        self.create_symbols_fts_table()?;
        self.create_symbols_fts_triggers()?;

        Ok(())
    }

    /// CASCADE: Create FTS5 virtual table for full-text search on symbols
    /// Indexes name, signature, doc_comment, and code_context for fast relevance-ranked search
    fn create_symbols_fts_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS symbols_fts USING fts5(
                name,
                signature,
                doc_comment,
                code_context,
                tokenize='porter unicode61',
                content='symbols',
                content_rowid='rowid'
            )",
            [],
        )?;
        debug!("Created symbols_fts virtual table with porter unicode61 tokenizer");
        Ok(())
    }

    /// CASCADE: Create triggers to keep symbols_fts in sync with symbols table
    fn create_symbols_fts_triggers(&self) -> Result<()> {
        // Trigger for INSERT
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS symbols_ai AFTER INSERT ON symbols BEGIN
                INSERT INTO symbols_fts(rowid, name, signature, doc_comment, code_context)
                VALUES (new.rowid, new.name, new.signature, new.doc_comment, new.code_context);
            END",
            [],
        )?;

        // Trigger for DELETE
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS symbols_ad AFTER DELETE ON symbols BEGIN
                DELETE FROM symbols_fts WHERE rowid = old.rowid;
            END",
            [],
        )?;

        // Trigger for UPDATE
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS symbols_au AFTER UPDATE ON symbols BEGIN
                UPDATE symbols_fts
                SET name = new.name,
                    signature = new.signature,
                    doc_comment = new.doc_comment,
                    code_context = new.code_context
                WHERE rowid = old.rowid;
            END",
            [],
        )?;

        debug!("Created symbols_fts synchronization triggers");
        Ok(())
    }

    /// Create the identifiers table for reference tracking
    fn create_identifiers_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS identifiers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,  -- call, variable_ref, type_usage, member_access
                language TEXT NOT NULL,

                -- Location
                file_path TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
                start_line INTEGER NOT NULL,
                start_col INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                end_col INTEGER NOT NULL,
                start_byte INTEGER,
                end_byte INTEGER,

                -- Semantic links (target_symbol_id is NULL until resolved on-demand)
                containing_symbol_id TEXT REFERENCES symbols(id) ON DELETE CASCADE,
                target_symbol_id TEXT REFERENCES symbols(id) ON DELETE SET NULL,
                confidence REAL DEFAULT 1.0,

                -- Context
                code_context TEXT,

                -- Infrastructure
                workspace_id TEXT NOT NULL DEFAULT 'primary',
                last_indexed INTEGER DEFAULT 0
            )",
            [],
        )?;

        // Essential indexes for fast queries
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_identifiers_name ON identifiers(name)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_identifiers_file ON identifiers(file_path)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_identifiers_containing ON identifiers(containing_symbol_id)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_identifiers_target ON identifiers(target_symbol_id)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_identifiers_kind ON identifiers(kind)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_identifiers_workspace ON identifiers(workspace_id)",
            [],
        )?;

        debug!("Created identifiers table and indexes");
        Ok(())
    }

    /// Create the relationships table for tracing data flow
    fn create_relationships_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS relationships (
                id TEXT PRIMARY KEY,
                from_symbol_id TEXT NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
                to_symbol_id TEXT NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
                kind TEXT NOT NULL,
                file_path TEXT NOT NULL DEFAULT '',  -- Location where relationship occurs
                line_number INTEGER NOT NULL DEFAULT 0,  -- Line number where relationship occurs (1-based)
                confidence REAL DEFAULT 1.0,
                metadata TEXT,  -- JSON blob
                created_at INTEGER DEFAULT 0,

                -- For multi-workspace support
                workspace_id TEXT NOT NULL DEFAULT 'primary'
            )",
            [],
        )?;

        // Indexes for relationship traversal
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_rel_from ON relationships(from_symbol_id)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_rel_to ON relationships(to_symbol_id)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_rel_kind ON relationships(kind)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_rel_workspace ON relationships(workspace_id)",
            [],
        )?;

        debug!("Created relationships table and indexes");
        Ok(())
    }

    /// Create the embeddings table for vector mapping
    fn create_embeddings_table(&self) -> Result<()> {
        // Metadata table: maps symbol_id to vector_id
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS embeddings (
                symbol_id TEXT NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
                vector_id TEXT NOT NULL,
                model_name TEXT NOT NULL,
                embedding_hash TEXT,
                created_at INTEGER DEFAULT 0,

                PRIMARY KEY (symbol_id, model_name)
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_embeddings_vector ON embeddings(vector_id)",
            [],
        )?;

        // Vector data table: stores actual f32 vector arrays as BLOBs
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS embedding_vectors (
                vector_id TEXT PRIMARY KEY,
                dimensions INTEGER NOT NULL,
                vector_data BLOB NOT NULL,
                model_name TEXT NOT NULL,
                created_at INTEGER DEFAULT 0
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_embedding_vectors_model ON embedding_vectors(model_name)",
            [],
        )?;

        debug!("Created embeddings and embedding_vectors tables with indexes");
        Ok(())
    }

    /// Store file information with Blake3 hash (regular method for incremental updates)
    pub fn store_file_info(&self, file_info: &FileInfo, workspace_id: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        self.conn.execute(
            "INSERT OR REPLACE INTO files
             (path, language, hash, size, last_modified, last_indexed, symbol_count, workspace_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                file_info.path,
                file_info.language,
                file_info.hash,
                file_info.size,
                file_info.last_modified,
                now, // Use calculated timestamp instead of unixepoch()
                file_info.symbol_count,
                workspace_id
            ],
        )?;

        debug!("Stored file info for: {}", file_info.path);
        Ok(())
    }

    /// 🚀 BLAZING-FAST bulk file storage for initial indexing
    pub fn bulk_store_files(&self, files: &[FileInfo], workspace_id: &str) -> Result<()> {
        if files.is_empty() {
            return Ok(());
        }

        let start_time = std::time::Instant::now();
        info!(
            "🚀 Starting blazing-fast bulk insert of {} files",
            files.len()
        );

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Drop file indexes
        self.drop_file_indexes()?;

        let tx = self.conn.unchecked_transaction()?;
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO files
             (path, language, hash, size, last_modified, last_indexed, symbol_count, workspace_id, content)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;

        for file in files {
            stmt.execute(params![
                file.path,
                file.language,
                file.hash,
                file.size,
                file.last_modified,
                now,
                file.symbol_count,
                workspace_id,
                file.content.as_deref().unwrap_or("") // CASCADE: Include content
            ])?;
        }

        // Drop statement before committing transaction
        drop(stmt);
        tx.commit()?;

        // Rebuild file indexes
        self.create_file_indexes()?;

        let duration = start_time.elapsed();
        info!(
            "✅ Bulk file insert complete! {} files in {:.2}ms",
            files.len(),
            duration.as_millis()
        );

        Ok(())
    }

    /// Drop all file table indexes for bulk operations
    fn drop_file_indexes(&self) -> Result<()> {
        let indexes = [
            "idx_files_language",
            "idx_files_modified",
            "idx_files_workspace",
        ];

        for index in &indexes {
            if let Err(e) = self
                .conn
                .execute(&format!("DROP INDEX IF EXISTS {}", index), [])
            {
                debug!("Note: Could not drop index {}: {}", index, e);
            }
        }

        Ok(())
    }

    /// Recreate all file table indexes after bulk operations
    fn create_file_indexes(&self) -> Result<()> {
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_language ON files(language)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_modified ON files(last_modified)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_workspace ON files(workspace_id)",
            [],
        )?;

        Ok(())
    }

    // ========================================
    // CASCADE ARCHITECTURE: File Content Storage & FTS
    // ========================================

    /// CASCADE: Store file with full content for FTS search
    #[allow(clippy::too_many_arguments)] // Legacy API, refactor later
    pub fn store_file_with_content(
        &self,
        path: &str,
        language: &str,
        hash: &str,
        size: u64,
        last_modified: u64,
        content: &str,
        workspace_id: &str,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        self.conn.execute(
            "INSERT OR REPLACE INTO files
             (path, language, hash, size, last_modified, last_indexed, symbol_count, content, workspace_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8)",
            params![path, language, hash, size as i64, last_modified as i64, now, content, workspace_id],
        )?;

        Ok(())
    }

    /// CASCADE: Get file content from database
    pub fn get_file_content(
        &self,
        path: &str,
        workspace_id: Option<&str>,
    ) -> Result<Option<String>> {
        match workspace_id {
            Some(ws_id) => {
                let mut stmt = self
                    .conn
                    .prepare("SELECT content FROM files WHERE path = ?1 AND workspace_id = ?2")?;

                match stmt.query_row(params![path, ws_id], |row| row.get::<_, Option<String>>(0)) {
                    Ok(content) => Ok(content),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(e) => Err(anyhow!("Database error: {}", e)),
                }
            }
            None => {
                let mut stmt = self
                    .conn
                    .prepare("SELECT content FROM files WHERE path = ?1")?;

                match stmt.query_row(params![path], |row| row.get::<_, Option<String>>(0)) {
                    Ok(content) => Ok(content),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(e) => Err(anyhow!("Database error: {}", e)),
                }
            }
        }
    }

    /// CASCADE: Get all file contents for workspace (for rebuilding Tantivy)
    pub fn get_all_file_contents(&self, workspace_id: &str) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, content FROM files WHERE workspace_id = ?1 AND content IS NOT NULL",
        )?;

        let rows = stmt.query_map(params![workspace_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }

    /// Get recently modified files (last N days)
    pub fn get_recent_files(
        &self,
        workspace_id: Option<&str>,
        days: u32,
        limit: usize,
    ) -> Result<Vec<FileInfo>> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let cutoff_time = now - (days as i64 * 86400); // days * seconds_per_day

        let mut results = Vec::new();

        if let Some(ws_id) = workspace_id {
            let mut stmt = self.conn.prepare(
                "SELECT path, language, hash, size, last_modified, last_indexed, symbol_count, content
                 FROM files
                 WHERE workspace_id = ?1 AND last_modified >= ?2
                 ORDER BY last_modified DESC
                 LIMIT ?3",
            )?;

            let rows = stmt.query_map(params![ws_id, cutoff_time, limit], |row| {
                Ok(FileInfo {
                    path: row.get(0)?,
                    language: row.get(1)?,
                    hash: row.get(2)?,
                    size: row.get(3)?,
                    last_modified: row.get(4)?,
                    last_indexed: row.get(5)?,
                    symbol_count: row.get(6)?,
                    content: row.get(7)?,
                })
            })?;

            for row in rows {
                results.push(row?);
            }
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT path, language, hash, size, last_modified, last_indexed, symbol_count, content
                 FROM files
                 WHERE last_modified >= ?1
                 ORDER BY last_modified DESC
                 LIMIT ?2",
            )?;

            let rows = stmt.query_map(params![cutoff_time, limit], |row| {
                Ok(FileInfo {
                    path: row.get(0)?,
                    language: row.get(1)?,
                    hash: row.get(2)?,
                    size: row.get(3)?,
                    last_modified: row.get(4)?,
                    last_indexed: row.get(5)?,
                    symbol_count: row.get(6)?,
                    content: row.get(7)?,
                })
            })?;

            for row in rows {
                results.push(row?);
            }
        }

        Ok(results)
    }

    /// CASCADE: Search file content using FTS5
    pub fn search_file_content_fts(
        &self,
        query: &str,
        workspace_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<FileSearchResult>> {
        let mut results = Vec::new();

        if let Some(ws_id) = workspace_id {
            let mut stmt = self.conn.prepare(
                "SELECT files_fts.path, snippet(files_fts, 1, '<mark>', '</mark>', '...', 32) as snippet, bm25(files_fts) as rank
                 FROM files_fts
                 JOIN files ON files_fts.path = files.path
                 WHERE files_fts MATCH ?1 AND files.workspace_id = ?2
                 ORDER BY rank DESC
                 LIMIT ?3"
            )?;

            let rows = stmt.query_map(params![query, ws_id, limit], |row| {
                Ok(FileSearchResult {
                    path: row.get(0)?,
                    snippet: row.get(1)?,
                    rank: row.get::<_, f64>(2)? as f32,
                })
            })?;

            for row in rows {
                results.push(row?);
            }
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT path, snippet(files_fts, 1, '<mark>', '</mark>', '...', 32) as snippet, bm25(files_fts) as rank
                 FROM files_fts
                 WHERE files_fts MATCH ?1
                 ORDER BY rank DESC
                 LIMIT ?2"
            )?;

            let rows = stmt.query_map(params![query, limit], |row| {
                Ok(FileSearchResult {
                    path: row.get(0)?,
                    snippet: row.get(1)?,
                    rank: row.get::<_, f64>(2)? as f32,
                })
            })?;

            for row in rows {
                results.push(row?);
            }
        }

        Ok(results)
    }

    /// Get file hash for change detection
    pub fn get_file_hash(&self, file_path: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT hash FROM files WHERE path = ?1")?;

        let result = stmt.query_row(params![file_path], |row| row.get::<_, String>(0));

        match result {
            Ok(hash) => Ok(Some(hash)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow!("Database error: {}", e)),
        }
    }

    /// Get symbol count for a file (for detecting files that need FILE_CONTENT symbols)
    pub fn get_file_symbol_count(&self, file_path: &str) -> Result<i32> {
        let mut stmt = self
            .conn
            .prepare("SELECT symbol_count FROM files WHERE path = ?1")?;

        let result = stmt.query_row(params![file_path], |row| row.get::<_, i32>(0));

        match result {
            Ok(count) => Ok(count),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
            Err(e) => Err(anyhow!("Database error: {}", e)),
        }
    }

    /// Update file hash for incremental change detection
    pub fn update_file_hash(&self, file_path: &str, new_hash: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        self.conn.execute(
            "UPDATE files SET hash = ?1, last_indexed = ?2 WHERE path = ?3",
            params![new_hash, now, file_path],
        )?;

        debug!("Updated hash for file: {}", file_path);
        Ok(())
    }

    /// Delete file record and associated symbols
    pub fn delete_file_record(&self, file_path: &str) -> Result<()> {
        // Symbols will be cascade-deleted due to foreign key constraint
        let count = self
            .conn
            .execute("DELETE FROM files WHERE path = ?1", params![file_path])?;

        debug!(
            "Deleted file record for: {} ({} rows affected)",
            file_path, count
        );
        Ok(())
    }

    /// Delete file record for a specific workspace (workspace-aware cleanup)
    pub fn delete_file_record_in_workspace(
        &self,
        file_path: &str,
        workspace_id: &str,
    ) -> Result<()> {
        let count = self.conn.execute(
            "DELETE FROM files WHERE path = ?1 AND workspace_id = ?2",
            params![file_path, workspace_id],
        )?;

        debug!(
            "Deleted file record for '{}' in workspace '{}' ({} rows affected)",
            file_path, workspace_id, count
        );
        Ok(())
    }

    /// Store symbols in a transaction (regular method for incremental updates)
    pub fn store_symbols(&self, symbols: &[Symbol], workspace_id: &str) -> Result<()> {
        if symbols.is_empty() {
            return Ok(());
        }

        debug!("Storing {} symbols", symbols.len());

        let tx = self.conn.unchecked_transaction()?;

        for symbol in symbols {
            let metadata_json = symbol
                .metadata
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;

            // Serialize visibility enum to string
            let visibility_str = symbol.visibility.as_ref().map(|v| match v {
                crate::extractors::base::Visibility::Public => "public",
                crate::extractors::base::Visibility::Private => "private",
                crate::extractors::base::Visibility::Protected => "protected",
            });

            tx.execute(
                "INSERT OR REPLACE INTO symbols
                 (id, name, kind, language, file_path, signature, start_line, start_col,
                  end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                  parent_id, metadata, semantic_group, confidence, workspace_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
                params![
                    symbol.id,
                    symbol.name,
                    symbol.kind.to_string(),
                    symbol.language,
                    symbol.file_path,
                    symbol.signature,
                    symbol.start_line,
                    symbol.start_column, // This matches start_col in table
                    symbol.end_line,
                    symbol.end_column, // This matches end_col in table
                    symbol.start_byte,
                    symbol.end_byte,
                    symbol.doc_comment,
                    visibility_str,
                    symbol.code_context,
                    symbol.parent_id,
                    metadata_json,
                    symbol.semantic_group,
                    symbol.confidence,
                    workspace_id
                ],
            )?;
        }

        tx.commit()?;
        // Symbols stored successfully - no need to log per call
        Ok(())
    }

    /// 🚀 BLAZING-FAST bulk symbol storage for initial indexing
    /// Optimized for speed over safety - drops indexes during insert!
    pub fn bulk_store_symbols(&mut self, symbols: &[Symbol], workspace_id: &str) -> Result<()> {
        if symbols.is_empty() {
            return Ok(());
        }

        let start_time = std::time::Instant::now();
        info!(
            "🚀 Starting blazing-fast bulk insert of {} symbols with workspace_id: {}",
            symbols.len(),
            workspace_id
        );

        // STEP 1: Drop all indexes for maximum insert speed
        debug!("🗑️ Dropping indexes for bulk insert optimization");
        self.drop_symbol_indexes()?;

        // STEP 2: Optimize SQLite for bulk operations (DANGEROUS but FAST!)
        self.conn.execute("PRAGMA synchronous = OFF", [])?; // No disk sync - risky but fast
                                                            // NOTE: Don't change journal_mode here - database is already in WAL mode
                                                            // Changing from WAL to MEMORY requires exclusive access and causes "database is locked" errors
                                                            // self.conn.execute_batch("PRAGMA journal_mode = MEMORY")?; // REMOVED: Causes lock conflicts
        self.conn.execute("PRAGMA cache_size = 20000", [])?; // Large cache for bulk ops

        // STEP 3: Start transaction for atomic bulk insert
        // Use regular transaction (not unchecked) to ensure foreign key constraints are enforced
        let tx = self.conn.transaction()?;

        // STEP 3.5: Insert file records first to satisfy foreign key constraints
        // Extract unique file paths with their languages from symbols
        let mut unique_files: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for symbol in symbols {
            unique_files
                .entry(symbol.file_path.clone())
                .or_insert_with(|| symbol.language.clone());
        }

        debug!("📁 Inserting {} unique file records", unique_files.len());
        let mut file_stmt = tx.prepare(
            "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified, last_indexed, workspace_id)
             VALUES (?1, ?2, '', 0, 0, ?3, ?4)"
        )?;

        let timestamp = chrono::Utc::now().timestamp();
        for (file_path, language) in unique_files {
            file_stmt.execute(rusqlite::params![
                file_path,
                language,
                timestamp,
                workspace_id
            ])?;
        }
        drop(file_stmt);

        // STEP 4: Prepare statement once, use many times
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO symbols
             (id, name, kind, language, file_path, signature, start_line, start_col,
              end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
              parent_id, metadata, semantic_group, confidence, workspace_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
        )?;

        // STEP 5: Sort symbols in parent-first order to avoid foreign key violations
        // Symbols with no parent go first, then their children, etc.
        let all_symbol_ids: std::collections::HashSet<_> =
            symbols.iter().map(|s| s.id.clone()).collect();

        let mut sorted_symbols = Vec::new();
        let mut remaining_symbols: Vec<_> = symbols.to_vec();
        let mut inserted_ids = std::collections::HashSet::new();

        // First pass: Insert all symbols with no parent
        let (no_parent, with_parent): (Vec<_>, Vec<_>) = remaining_symbols
            .into_iter()
            .partition(|s| s.parent_id.is_none());

        for symbol in no_parent {
            inserted_ids.insert(symbol.id.clone());
            sorted_symbols.push(symbol);
        }

        remaining_symbols = with_parent;

        // Subsequent passes: Insert symbols whose parents have been inserted
        while !remaining_symbols.is_empty() {
            let initial_count = remaining_symbols.len();
            let (can_insert, still_waiting): (Vec<_>, Vec<_>) =
                remaining_symbols.into_iter().partition(|s| {
                    s.parent_id
                        .as_ref()
                        .map(|pid| inserted_ids.contains(pid))
                        .unwrap_or(false)
                });

            for symbol in can_insert {
                inserted_ids.insert(symbol.id.clone());
                sorted_symbols.push(symbol);
            }

            remaining_symbols = still_waiting;

            // Break if we made no progress (circular dependency or orphaned symbols)
            if remaining_symbols.len() == initial_count {
                warn!(
                    "⚠️ Skipping {} symbols with unresolvable parent references",
                    remaining_symbols.len()
                );
                for mut symbol in remaining_symbols {
                    if let Some(parent_id) = &symbol.parent_id {
                        if !all_symbol_ids.contains(parent_id) {
                            debug!(
                                "Orphan symbol {} ({}) has missing parent {} - clearing relationship",
                                symbol.name,
                                symbol.id,
                                parent_id
                            );
                            symbol.parent_id = None;
                        }
                    }
                    sorted_symbols.push(symbol);
                }
                break;
            }
        }

        // Final pass: ensure no symbol references a missing parent (enforce FK safety)
        for symbol in &mut sorted_symbols {
            if let Some(parent_id) = &symbol.parent_id {
                if !all_symbol_ids.contains(parent_id) {
                    debug!(
                        "Clearing missing parent {} for symbol {} ({}) before insert",
                        parent_id, symbol.name, symbol.id
                    );
                    symbol.parent_id = None;
                }
            }
        }

        // STEP 6: Batch insert for optimal performance
        const BATCH_SIZE: usize = 1000;
        let mut processed = 0;

        // Log the first symbol for debugging
        if let Some(first_symbol) = sorted_symbols.first() {
            info!(
                "🔍 First symbol to insert: name={}, file_path={}, parent_id={:?}, id={}",
                first_symbol.name, first_symbol.file_path, first_symbol.parent_id, first_symbol.id
            );
        }

        for chunk in sorted_symbols.chunks(BATCH_SIZE) {
            for symbol in chunk {
                let metadata_json = symbol
                    .metadata
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?;

                // Serialize visibility enum to string
                let visibility_str = symbol.visibility.as_ref().map(|v| match v {
                    crate::extractors::base::Visibility::Public => "public",
                    crate::extractors::base::Visibility::Private => "private",
                    crate::extractors::base::Visibility::Protected => "protected",
                });

                match stmt.execute(params![
                    symbol.id,
                    symbol.name,
                    symbol.kind.to_string(),
                    symbol.language,
                    symbol.file_path,
                    symbol.signature,
                    symbol.start_line,
                    symbol.start_column,
                    symbol.end_line,
                    symbol.end_column,
                    symbol.start_byte,
                    symbol.end_byte,
                    symbol.doc_comment,
                    visibility_str,
                    symbol.code_context,
                    symbol.parent_id,
                    metadata_json,
                    symbol.semantic_group,
                    symbol.confidence,
                    workspace_id
                ]) {
                    Ok(_) => {}
                    Err(e) => {
                        // Log the first few failures to understand what's wrong
                        if processed < 5 {
                            warn!("Failed to insert symbol: {} from file: {} with parent: {:?}. Error: {}",
                                  symbol.name, symbol.file_path, symbol.parent_id, e);
                        }
                        return Err(anyhow::anyhow!("Symbol insertion failed: {}", e));
                    }
                }

                processed += 1;
            }

            // Progress logging for large bulk operations
            if processed % 5000 == 0 {
                debug!(
                    "📊 Bulk insert progress: {}/{} symbols",
                    processed,
                    symbols.len()
                );
            }
        }

        // STEP 6: Drop statement and commit transaction
        drop(stmt);
        tx.commit()?;

        // STEP 7: Restore safe SQLite settings
        self.conn.execute("PRAGMA synchronous = NORMAL", [])?;
        // journal_mode returns a result, so we need to use query_row or execute_batch
        self.conn.execute_batch("PRAGMA journal_mode = WAL")?;

        // STEP 8: Rebuild all indexes (still faster than incremental with indexes!)
        debug!("🏗️ Rebuilding indexes after bulk insert");
        self.create_symbol_indexes()?;

        let duration = start_time.elapsed();
        info!(
            "✅ Blazing-fast bulk insert complete! {} symbols in {:.2}ms ({:.0} symbols/sec)",
            symbols.len(),
            duration.as_millis(),
            symbols.len() as f64 / duration.as_secs_f64()
        );

        Ok(())
    }

    /// Drop all symbol table indexes for bulk operations
    fn drop_symbol_indexes(&self) -> Result<()> {
        let indexes = [
            "idx_symbols_name",
            "idx_symbols_kind",
            "idx_symbols_language",
            "idx_symbols_file",
            "idx_symbols_semantic",
            "idx_symbols_parent",
            "idx_symbols_workspace",
        ];

        for index in &indexes {
            if let Err(e) = self
                .conn
                .execute(&format!("DROP INDEX IF EXISTS {}", index), [])
            {
                // Don't fail if index doesn't exist
                debug!("Note: Could not drop index {}: {}", index, e);
            }
        }

        Ok(())
    }

    /// Recreate all symbol table indexes after bulk operations
    fn create_symbol_indexes(&self) -> Result<()> {
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols(kind)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_language ON symbols(language)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_path)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_semantic ON symbols(semantic_group)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_parent ON symbols(parent_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_workspace ON symbols(workspace_id)",
            [],
        )?;

        Ok(())
    }

    /// Bulk store identifiers (references/usages) for LSP-quality reference tracking
    ///
    /// NEW: This is the write path for identifier extraction. Identifiers are stored
    /// unresolved (target_symbol_id = NULL) and resolved on-demand during queries.
    ///
    /// Performance optimized like bulk_store_symbols: drop indexes, batch insert, rebuild indexes.
    pub fn bulk_store_identifiers(
        &mut self,
        identifiers: &[crate::extractors::Identifier],
        workspace_id: &str,
    ) -> Result<()> {
        if identifiers.is_empty() {
            return Ok(());
        }

        let start_time = std::time::Instant::now();
        info!(
            "🚀 Starting bulk insert of {} identifiers with workspace_id: {}",
            identifiers.len(),
            workspace_id
        );

        // STEP 1: Drop all indexes for maximum insert speed
        debug!("🗑️ Dropping identifier indexes for bulk insert optimization");
        self.drop_identifier_indexes()?;

        // STEP 2: Optimize SQLite for bulk operations
        self.conn.execute("PRAGMA synchronous = OFF", [])?;
        self.conn.execute_batch("PRAGMA journal_mode = MEMORY")?;
        self.conn.execute("PRAGMA cache_size = 20000", [])?;

        // STEP 3: Start transaction for atomic bulk insert
        let tx = self.conn.transaction()?;

        // STEP 4: Prepare statement once, use many times
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO identifiers
             (id, name, kind, language, file_path, start_line, start_col,
              end_line, end_col, start_byte, end_byte, containing_symbol_id,
              target_symbol_id, confidence, code_context, workspace_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        )?;

        // STEP 5: Batch insert for optimal performance
        const BATCH_SIZE: usize = 1000;
        let mut processed = 0;

        for chunk in identifiers.chunks(BATCH_SIZE) {
            for identifier in chunk {
                stmt.execute(params![
                    identifier.id,
                    identifier.name,
                    identifier.kind.to_string(),
                    identifier.language,
                    identifier.file_path,
                    identifier.start_line,
                    identifier.start_column,
                    identifier.end_line,
                    identifier.end_column,
                    identifier.start_byte,
                    identifier.end_byte,
                    identifier.containing_symbol_id,
                    identifier.target_symbol_id, // NULL until resolved on-demand
                    identifier.confidence,
                    identifier.code_context,
                    workspace_id
                ])?;

                processed += 1;
            }

            // Progress logging for large bulk operations
            if processed % 5000 == 0 {
                debug!(
                    "📊 Bulk insert progress: {}/{} identifiers",
                    processed,
                    identifiers.len()
                );
            }
        }

        // STEP 6: Drop statement and commit transaction
        drop(stmt);
        tx.commit()?;

        // STEP 7: Restore safe SQLite settings
        self.conn.execute("PRAGMA synchronous = NORMAL", [])?;
        self.conn.execute_batch("PRAGMA journal_mode = WAL")?;

        // STEP 8: Rebuild all indexes
        debug!("🏗️ Rebuilding identifier indexes after bulk insert");
        self.create_identifier_indexes()?;

        let duration = start_time.elapsed();
        info!(
            "✅ Bulk identifier insert complete! {} identifiers in {:.2}ms ({:.0} identifiers/sec)",
            identifiers.len(),
            duration.as_millis(),
            identifiers.len() as f64 / duration.as_secs_f64()
        );

        Ok(())
    }

    /// Drop all identifier table indexes for bulk operations
    fn drop_identifier_indexes(&self) -> Result<()> {
        let indexes = [
            "idx_identifiers_name",
            "idx_identifiers_file",
            "idx_identifiers_containing",
            "idx_identifiers_target",
            "idx_identifiers_kind",
            "idx_identifiers_workspace",
        ];

        for index in &indexes {
            if let Err(e) = self
                .conn
                .execute(&format!("DROP INDEX IF EXISTS {}", index), [])
            {
                debug!("Note: Could not drop index {}: {}", index, e);
            }
        }

        Ok(())
    }

    /// Create all identifier table indexes after bulk operations
    fn create_identifier_indexes(&self) -> Result<()> {
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_identifiers_name ON identifiers(name)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_identifiers_file ON identifiers(file_path)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_identifiers_containing ON identifiers(containing_symbol_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_identifiers_target ON identifiers(target_symbol_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_identifiers_kind ON identifiers(kind)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_identifiers_workspace ON identifiers(workspace_id)",
            [],
        )?;

        Ok(())
    }

    /// Store relationships in a transaction (regular method for incremental updates)
    pub fn store_relationships(
        &self,
        relationships: &[Relationship],
        workspace_id: &str,
    ) -> Result<()> {
        if relationships.is_empty() {
            return Ok(());
        }

        debug!("Storing {} relationships", relationships.len());

        let tx = self.conn.unchecked_transaction()?;

        for rel in relationships {
            let metadata_json = rel
                .metadata
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;

            tx.execute(
                "INSERT OR REPLACE INTO relationships
                 (id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata, workspace_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    rel.id,
                    rel.from_symbol_id,
                    rel.to_symbol_id,
                    rel.kind.to_string(),
                    rel.file_path,
                    rel.line_number,
                    rel.confidence,
                    metadata_json,
                    workspace_id
                ],
            )?;
        }

        tx.commit()?;
        info!("Successfully stored {} relationships", relationships.len());
        Ok(())
    }

    /// 🚀 BLAZING-FAST bulk relationship storage for initial indexing
    pub fn bulk_store_relationships(
        &mut self,
        relationships: &[Relationship],
        workspace_id: &str,
    ) -> Result<()> {
        if relationships.is_empty() {
            return Ok(());
        }

        let start_time = std::time::Instant::now();
        info!(
            "🚀 Starting blazing-fast bulk insert of {} relationships",
            relationships.len()
        );

        // Drop relationship indexes
        self.drop_relationship_indexes()?;

        // Use regular transaction to ensure foreign key constraints are enforced
        let tx = self.conn.transaction()?;
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO relationships
             (id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata, workspace_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;

        let mut inserted_count = 0;
        let mut skipped_count = 0;

        for rel in relationships {
            let metadata_json = rel
                .metadata
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;

            // Try to insert, skip if foreign key constraint fails (external/missing symbols)
            match stmt.execute(params![
                rel.id,
                rel.from_symbol_id,
                rel.to_symbol_id,
                rel.kind.to_string(),
                rel.file_path,
                rel.line_number,
                rel.confidence,
                metadata_json,
                workspace_id
            ]) {
                Ok(_) => inserted_count += 1,
                Err(rusqlite::Error::SqliteFailure(err, _))
                    if err.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    // Skip relationships with missing symbol references
                    skipped_count += 1;
                    debug!(
                        "Skipping relationship {} -> {} (missing symbol reference)",
                        rel.from_symbol_id, rel.to_symbol_id
                    );
                }
                Err(e) => return Err(e.into()),
            }
        }

        // Drop statement before committing transaction
        drop(stmt);
        tx.commit()?;

        // Rebuild relationship indexes
        self.create_relationship_indexes()?;

        let duration = start_time.elapsed();
        if skipped_count > 0 {
            info!(
                "✅ Bulk relationship insert complete! {} inserted, {} skipped (external symbols) in {:.2}ms",
                inserted_count,
                skipped_count,
                duration.as_millis()
            );
        } else {
            info!(
                "✅ Bulk relationship insert complete! {} relationships in {:.2}ms",
                inserted_count,
                duration.as_millis()
            );
        }

        Ok(())
    }

    /// Drop all relationship table indexes for bulk operations
    fn drop_relationship_indexes(&self) -> Result<()> {
        let indexes = [
            "idx_rel_from",
            "idx_rel_to",
            "idx_rel_kind",
            "idx_rel_workspace",
        ];

        for index in &indexes {
            if let Err(e) = self
                .conn
                .execute(&format!("DROP INDEX IF EXISTS {}", index), [])
            {
                debug!("Note: Could not drop index {}: {}", index, e);
            }
        }

        Ok(())
    }

    /// Recreate all relationship table indexes after bulk operations
    fn create_relationship_indexes(&self) -> Result<()> {
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_rel_from ON relationships(from_symbol_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_rel_to ON relationships(to_symbol_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_rel_kind ON relationships(kind)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_rel_workspace ON relationships(workspace_id)",
            [],
        )?;

        Ok(())
    }

    /// Get symbol by ID
    pub fn get_symbol_by_id(&self, id: &str) -> Result<Option<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence
             FROM symbols WHERE id = ?1",
        )?;

        let result = stmt.query_row(params![id], |row| self.row_to_symbol(row));

        match result {
            Ok(symbol) => Ok(Some(symbol)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow!("Database error: {}", e)),
        }
    }

    /// Get multiple symbols by their IDs in one batched query (for semantic search results)
    pub fn get_symbols_by_ids(&self, ids: &[String]) -> Result<Vec<Symbol>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // Build parameterized query with IN clause for batch fetch
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{}", i)).collect();
        let query = format!(
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence
             FROM symbols WHERE id IN ({})",
            placeholders.join(", ")
        );

        let mut stmt = self.conn.prepare(&query)?;

        // Convert Vec<String> to Vec<&dyn ToSql> for params!
        let params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();

        let symbol_iter = stmt.query_map(&params[..], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for symbol_result in symbol_iter {
            symbols.push(symbol_result?);
        }

        Ok(symbols)
    }

    /// Find symbols by name with optional language filter
    pub fn find_symbols_by_name(&self, name: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence
             FROM symbols
             WHERE name = ?1
             ORDER BY language, file_path",
        )?;

        let symbol_iter = stmt.query_map(params![name], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for symbol_result in symbol_iter {
            symbols.push(symbol_result?);
        }

        debug!("Found {} symbols named '{}'", symbols.len(), name);
        Ok(symbols)
    }

    /// 🔒 FTS5 Query Sanitization - Escape special characters that cause syntax errors
    ///
    /// FTS5 has several special characters that trigger specific behaviors:
    /// - `#` - Column specifier (e.g., `name:#term`)
    /// - `@` - Auxiliary function calls
    /// - `^` - Initial token match
    /// - `:` - Can be interpreted as column separator
    /// - `[` `]` - Special meaning in some contexts
    ///
    /// Strategy:
    /// 1. If query is already quoted → pass through as-is (user knows what they want)
    /// 2. If query contains intentional operators (AND, OR, NOT, *, ") → pass through
    /// 3. If query contains special characters → quote the entire query as a phrase
    /// 4. Otherwise → pass through as-is (simple term search)
    fn sanitize_fts5_query(query: &str) -> String {
        let trimmed = query.trim();

        // Empty queries pass through (will return no results anyway)
        if trimmed.is_empty() {
            return trimmed.to_string();
        }

        // Already quoted - user explicitly wants phrase search
        if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        {
            return trimmed.to_string();
        }

        // Contains explicit FTS5 operators - pass through (user knows FTS5 syntax)
        if trimmed.contains(" AND ") || trimmed.contains(" OR ") || trimmed.contains(" NOT ") {
            return trimmed.to_string();
        }

        // Contains intentional wildcards - pass through
        if trimmed.contains('*') {
            return trimmed.to_string();
        }

        // FTS5 special characters that need escaping
        // Note: + is not officially documented as special, but causes "syntax error near +" in practice
        const SPECIAL_CHARS: &[char] = &['#', '@', '^', '[', ']', ':', '+', '/', '\\'];

        // Check if query contains any special characters
        let has_special = trimmed.chars().any(|c| SPECIAL_CHARS.contains(&c));

        if has_special {
            // Quote the entire query to treat it as a literal phrase
            // Use double quotes and escape any internal double quotes
            let escaped = trimmed.replace('"', "\"\""); // FTS5 uses doubled quotes for escaping
            format!("\"{}\"", escaped)
        } else {
            // Simple term - no special characters, pass through
            trimmed.to_string()
        }
    }

    /// 🔥 CASCADE FTS5: Find symbols using full-text search with BM25 ranking
    /// Replaces slow LIKE queries with fast FTS5 MATCH queries
    /// Column weights: name (10x), signature (5x), doc_comment (2x), code_context (1x)
    pub fn find_symbols_by_pattern(
        &self,
        pattern: &str,
        workspace_ids: Option<Vec<String>>,
    ) -> Result<Vec<Symbol>> {
        // 🔒 CRITICAL FIX: Sanitize query to prevent FTS5 syntax errors from special characters
        let sanitized_pattern = Self::sanitize_fts5_query(pattern);
        debug!(
            "🔍 FTS5 query sanitization: '{}' -> '{}'",
            pattern, sanitized_pattern
        );
        let (query, params) = if let Some(ws_ids) = workspace_ids {
            if ws_ids.is_empty() {
                return Ok(Vec::new());
            }

            let placeholders = ws_ids
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 2))
                .collect::<Vec<_>>()
                .join(",");

            // 🔥 FTS5 MATCH with BM25 ranking and workspace filtering
            // Prioritize exact name matches with 10x weight, then signature (5x), doc_comment (2x), code_context (1x)
            let query = format!(
                "SELECT s.id, s.name, s.kind, s.language, s.file_path, s.signature, s.start_line, s.start_col,
                        s.end_line, s.end_col, s.start_byte, s.end_byte, s.doc_comment, s.visibility, s.code_context,
                        s.parent_id, s.metadata, s.semantic_group, s.confidence, s.workspace_id
                 FROM symbols s
                 INNER JOIN symbols_fts fts ON s.rowid = fts.rowid
                 WHERE symbols_fts MATCH ?1 AND s.workspace_id IN ({})
                 ORDER BY bm25(symbols_fts, 10.0, 5.0, 2.0, 1.0)",
                placeholders
            );

            let mut params = vec![sanitized_pattern.clone()];
            params.extend(ws_ids);
            (query, params)
        } else {
            // 🔥 FTS5 MATCH with BM25 ranking - no workspace filter
            let query = "SELECT s.id, s.name, s.kind, s.language, s.file_path, s.signature, s.start_line, s.start_col,
                               s.end_line, s.end_col, s.start_byte, s.end_byte, s.doc_comment, s.visibility, s.code_context,
                               s.parent_id, s.metadata, s.semantic_group, s.confidence, s.workspace_id
                         FROM symbols s
                         INNER JOIN symbols_fts fts ON s.rowid = fts.rowid
                         WHERE symbols_fts MATCH ?1
                         ORDER BY bm25(symbols_fts, 10.0, 5.0, 2.0, 1.0)".to_string();
            (query, vec![sanitized_pattern])
        };

        let mut stmt = self.conn.prepare(&query)?;

        let symbol_iter = stmt.query_map(
            params
                .iter()
                .map(|p| p as &dyn rusqlite::ToSql)
                .collect::<Vec<_>>()
                .as_slice(),
            |row| self.row_to_symbol(row),
        )?;

        let mut symbols = Vec::new();
        for symbol_result in symbol_iter {
            symbols.push(symbol_result?);
        }

        debug!(
            "🔍 FTS5: Found {} symbols matching '{}' (BM25 ranked)",
            symbols.len(),
            pattern
        );
        Ok(symbols)
    }

    /// Get symbols for a specific file
    pub fn get_symbols_for_file(&self, file_path: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence
             FROM symbols
             WHERE file_path = ?1
             ORDER BY start_line, start_col",
        )?;

        let symbol_iter = stmt.query_map(params![file_path], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for symbol_result in symbol_iter {
            symbols.push(symbol_result?);
        }

        debug!("Found {} symbols in file '{}'", symbols.len(), file_path);
        Ok(symbols)
    }

    /// Delete symbols for a specific file (for incremental updates)
    pub fn delete_symbols_for_file(&self, file_path: &str) -> Result<()> {
        let count = self.conn.execute(
            "DELETE FROM symbols WHERE file_path = ?1",
            params![file_path],
        )?;

        debug!("Deleted {} symbols from file '{}'", count, file_path);
        Ok(())
    }

    /// Delete symbols for a specific file within a workspace (workspace-aware incremental updates)
    pub fn delete_symbols_for_file_in_workspace(
        &self,
        file_path: &str,
        workspace_id: &str,
    ) -> Result<()> {
        let count = self.conn.execute(
            "DELETE FROM symbols WHERE file_path = ?1 AND workspace_id = ?2",
            params![file_path, workspace_id],
        )?;

        debug!(
            "Deleted {} symbols from file '{}' in workspace '{}'",
            count, file_path, workspace_id
        );
        Ok(())
    }

    /// Delete relationships for symbols from a specific file within a workspace
    pub fn delete_relationships_for_file(&self, file_path: &str, workspace_id: &str) -> Result<()> {
        // Delete relationships where either the from_symbol or to_symbol belongs to the file
        let count = self.conn.execute(
            "DELETE FROM relationships
             WHERE from_symbol_id IN (
                 SELECT id FROM symbols WHERE file_path = ?1 AND workspace_id = ?2
             )
             OR to_symbol_id IN (
                 SELECT id FROM symbols WHERE file_path = ?1 AND workspace_id = ?2
             )",
            params![file_path, workspace_id],
        )?;

        debug!(
            "Deleted {} relationships for file '{}' in workspace '{}'",
            count, file_path, workspace_id
        );
        Ok(())
    }

    /// Get outgoing relationships from a symbol
    pub fn get_outgoing_relationships(&self, symbol_id: &str) -> Result<Vec<Relationship>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata
             FROM relationships
             WHERE from_symbol_id = ?1",
        )?;

        let rel_iter = stmt.query_map(params![symbol_id], |row| self.row_to_relationship(row))?;

        let mut relationships = Vec::new();
        for rel_result in rel_iter {
            relationships.push(rel_result?);
        }

        debug!(
            "Found {} outgoing relationships from symbol '{}'",
            relationships.len(),
            symbol_id
        );
        Ok(relationships)
    }

    /// Begin a database transaction
    pub fn begin_transaction(&mut self) -> Result<()> {
        self.conn.execute("BEGIN TRANSACTION", [])?;
        Ok(())
    }

    /// Commit the current transaction
    pub fn commit_transaction(&self) -> Result<()> {
        self.conn.execute("COMMIT", [])?;
        Ok(())
    }

    /// Rollback the current transaction
    pub fn rollback_transaction(&self) -> Result<()> {
        self.conn.execute("ROLLBACK", [])?;
        Ok(())
    }

    /// Get database statistics
    pub fn get_stats(&self) -> Result<DatabaseStats> {
        let total_symbols: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;

        let total_relationships: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM relationships", [], |row| row.get(0))?;

        let total_files: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;

        let total_embeddings: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM embeddings", [], |row| row.get(0))?;

        // Get unique languages
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT language FROM files ORDER BY language")?;

        let language_iter = stmt.query_map([], |row| row.get::<_, String>(0))?;

        let mut languages = Vec::new();
        for lang_result in language_iter {
            languages.push(lang_result?);
        }

        // Get database file size
        let db_size_mb = if let Ok(metadata) = std::fs::metadata(&self.file_path) {
            metadata.len() as f64 / (1024.0 * 1024.0)
        } else {
            0.0
        };

        Ok(DatabaseStats {
            total_symbols,
            total_relationships,
            total_files,
            total_embeddings,
            languages,
            db_size_mb,
        })
    }

    /// Helper to convert database row to Symbol
    fn row_to_symbol(&self, row: &Row) -> rusqlite::Result<Symbol> {
        let kind_str: String = row.get("kind")?;
        let kind = SymbolKind::from_string(&kind_str);

        let metadata_json: Option<String> = row.get("metadata")?;
        let metadata = metadata_json.and_then(|json| serde_json::from_str(&json).ok());

        // Deserialize visibility string to enum
        let visibility_str: Option<String> = row.get("visibility")?;
        let visibility = visibility_str.and_then(|v| match v.as_str() {
            "public" => Some(crate::extractors::base::Visibility::Public),
            "private" => Some(crate::extractors::base::Visibility::Private),
            "protected" => Some(crate::extractors::base::Visibility::Protected),
            _ => None,
        });

        Ok(Symbol {
            id: row.get("id")?,
            name: row.get("name")?,
            kind,
            language: row.get("language")?,
            file_path: row.get("file_path")?,
            signature: row.get("signature")?,
            start_line: row.get("start_line")?,
            start_column: row.get("start_col")?,
            end_line: row.get("end_line")?,
            end_column: row.get("end_col")?,
            start_byte: row.get("start_byte")?,
            end_byte: row.get("end_byte")?,
            doc_comment: row.get("doc_comment")?,
            visibility,
            parent_id: row.get("parent_id")?,
            metadata,
            semantic_group: row.get("semantic_group")?,
            confidence: row.get("confidence")?,
            code_context: row.get("code_context")?,
        })
    }

    /// Helper to convert database row to Relationship
    fn row_to_relationship(&self, row: &Row) -> rusqlite::Result<Relationship> {
        let kind_str: String = row.get("kind")?;
        let kind = RelationshipKind::from_string(&kind_str);

        let metadata_json: Option<String> = row.get("metadata")?;
        let metadata = metadata_json.and_then(|json| serde_json::from_str(&json).ok());

        Ok(Relationship {
            id: row.get("id")?,
            from_symbol_id: row.get("from_symbol_id")?,
            to_symbol_id: row.get("to_symbol_id")?,
            kind,
            file_path: row.get("file_path").unwrap_or_else(|_| String::new()), // Support old DBs without migration
            line_number: row.get("line_number").unwrap_or(0), // Support old DBs without migration
            confidence: row.get("confidence").unwrap_or(1.0),
            metadata,
        })
    }

    /// Get relationships where the specified symbol is the source (from_symbol_id)
    pub fn get_relationships_for_symbol(&self, symbol_id: &str) -> Result<Vec<Relationship>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata
            FROM relationships
            WHERE from_symbol_id = ?1
        ",
        )?;

        let rows = stmt.query_map([symbol_id], |row| self.row_to_relationship(row))?;

        let mut relationships = Vec::new();
        for row_result in rows {
            relationships.push(row_result?);
        }

        Ok(relationships)
    }

    /// Get relationships TO a symbol (where symbol is the target/referenced)
    /// Uses indexed query on to_symbol_id for O(log n) performance
    /// Complements get_relationships_for_symbol() which finds relationships FROM a symbol
    pub fn get_relationships_to_symbol(&self, symbol_id: &str) -> Result<Vec<Relationship>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata
            FROM relationships
            WHERE to_symbol_id = ?1
        ",
        )?;

        let rows = stmt.query_map([symbol_id], |row| self.row_to_relationship(row))?;

        let mut relationships = Vec::new();
        for row_result in rows {
            relationships.push(row_result?);
        }

        Ok(relationships)
    }

    /// Get relationships TO multiple symbols in a single batch query
    /// PERFORMANCE FIX: Replaces N+1 query pattern with single batch query using SQL IN clause
    pub fn get_relationships_to_symbols(&self, symbol_ids: &[String]) -> Result<Vec<Relationship>> {
        if symbol_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Build parameterized query with IN clause for batch fetch
        let placeholders: Vec<String> = (1..=symbol_ids.len()).map(|i| format!("?{}", i)).collect();
        let query = format!(
            "SELECT id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata
             FROM relationships
             WHERE to_symbol_id IN ({})",
            placeholders.join(", ")
        );

        let mut stmt = self.conn.prepare(&query)?;

        // Convert Vec<String> to Vec<&dyn ToSql> for params
        let params: Vec<&dyn rusqlite::ToSql> = symbol_ids
            .iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();

        let relationship_iter = stmt.query_map(&params[..], |row| self.row_to_relationship(row))?;

        let mut relationships = Vec::new();
        for relationship_result in relationship_iter {
            relationships.push(relationship_result?);
        }

        Ok(relationships)
    }

    /// Get symbols grouped by semantic_group field
    pub fn get_symbols_by_semantic_group(&self, semantic_group: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, name, kind, language, file_path, signature,
                   start_line, start_col, end_line, end_col, start_byte, end_byte,
                   doc_comment, visibility, code_context, parent_id,
                   metadata, semantic_group, confidence
            FROM symbols
            WHERE semantic_group = ?1
        ",
        )?;

        let rows = stmt.query_map([semantic_group], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row_result in rows {
            symbols.push(row_result?);
        }

        Ok(symbols)
    }

    /// Get all symbols from all workspaces (for SearchEngine population)
    pub fn get_all_symbols(&self) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, name, kind, language, file_path, signature,
                   start_line, start_col, end_line, end_col, start_byte, end_byte,
                   doc_comment, visibility, code_context, parent_id,
                   metadata, semantic_group, confidence
            FROM symbols
            ORDER BY workspace_id, file_path, start_line
        ",
        )?;

        let rows = stmt.query_map([], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row_result in rows {
            symbols.push(row_result?);
        }

        debug!(
            "Retrieved {} symbols from database for SearchEngine",
            symbols.len()
        );
        Ok(symbols)
    }

    /// Get all symbols matching an exact name (indexed lookup)
    /// Used to replace in-memory Vec<Symbol> fallbacks with persistent SQLite queries
    pub fn get_symbols_by_name(&self, name: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, name, kind, language, file_path, signature,
                   start_line, start_col, end_line, end_col, start_byte, end_byte,
                   doc_comment, visibility, code_context, parent_id,
                   metadata, semantic_group, confidence
            FROM symbols
            WHERE name = ?1
            ORDER BY file_path, start_line
        ",
        )?;

        let rows = stmt.query_map([name], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row_result in rows {
            symbols.push(row_result?);
        }

        debug!(
            "Retrieved {} symbols with name '{}' from database",
            symbols.len(),
            name
        );
        Ok(symbols)
    }

    /// Get symbols by exact name match with workspace filtering
    /// PERFORMANCE: Uses indexed WHERE name = ?1 instead of LIKE for O(log n) lookup
    pub fn get_symbols_by_name_and_workspace(
        &self,
        name: &str,
        workspace_ids: Vec<String>,
    ) -> Result<Vec<Symbol>> {
        if workspace_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Build parameterized query with IN clause for workspace filtering
        let placeholders = workspace_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 2))
            .collect::<Vec<_>>()
            .join(",");

        let query = format!(
            "SELECT id, name, kind, language, file_path, signature,
                    start_line, start_col, end_line, end_col, start_byte, end_byte,
                    doc_comment, visibility, code_context, parent_id,
                    metadata, semantic_group, confidence
             FROM symbols
             WHERE name = ?1 AND workspace_id IN ({})
             ORDER BY workspace_id, file_path, start_line",
            placeholders
        );

        let mut stmt = self.conn.prepare(&query)?;

        // Build parameters: name first, then workspace IDs
        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&name as &dyn rusqlite::ToSql];
        let ws_params: Vec<&dyn rusqlite::ToSql> = workspace_ids
            .iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();
        params.extend(ws_params);

        let rows = stmt.query_map(&params[..], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row_result in rows {
            symbols.push(row_result?);
        }

        debug!(
            "Retrieved {} symbols with exact name '{}' from {} workspace(s)",
            symbols.len(),
            name,
            workspace_ids.len()
        );
        Ok(symbols)
    }

    /// Get all relationships from the database
    /// Used to replace in-memory Vec<Relationship> fallbacks with persistent SQLite queries
    pub fn get_all_relationships(&self) -> Result<Vec<Relationship>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata
            FROM relationships
            ORDER BY from_symbol_id
        ",
        )?;

        let rows = stmt.query_map([], |row| self.row_to_relationship(row))?;

        let mut relationships = Vec::new();
        for row_result in rows {
            relationships.push(row_result?);
        }

        debug!(
            "Retrieved {} relationships from database",
            relationships.len()
        );
        Ok(relationships)
    }

    // ==================== EMBEDDING PERSISTENCE METHODS ====================

    /// Store embedding vector data as BLOB
    pub fn store_embedding_vector(
        &self,
        vector_id: &str,
        vector_data: &[f32],
        dimensions: usize,
        model_name: &str,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Serialize f32 vector to bytes using native endianness
        let bytes: Vec<u8> = vector_data.iter().flat_map(|f| f.to_le_bytes()).collect();

        self.conn.execute(
            "INSERT OR REPLACE INTO embedding_vectors
             (vector_id, dimensions, vector_data, model_name, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![vector_id, dimensions as i64, bytes, model_name, now],
        )?;

        debug!(
            "Stored embedding vector: {} ({}D, {} bytes)",
            vector_id,
            dimensions,
            bytes.len()
        );
        Ok(())
    }

    /// Retrieve embedding vector data from BLOB
    pub fn get_embedding_vector(&self, vector_id: &str) -> Result<Option<Vec<f32>>> {
        let result = self.conn.query_row(
            "SELECT vector_data, dimensions FROM embedding_vectors WHERE vector_id = ?1",
            params![vector_id],
            |row| {
                let bytes: Vec<u8> = row.get(0)?;
                let dimensions: i64 = row.get(1)?;
                Ok((bytes, dimensions))
            },
        );

        match result {
            Ok((bytes, dimensions)) => {
                // Deserialize bytes back to f32 vector
                if bytes.len() != (dimensions as usize * 4) {
                    return Err(anyhow!(
                        "Invalid vector data size: expected {} bytes, got {}",
                        dimensions * 4,
                        bytes.len()
                    ));
                }

                let vector: Vec<f32> = bytes
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();

                Ok(Some(vector))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow!("Failed to retrieve embedding vector: {}", e)),
        }
    }

    /// Store embedding metadata linking symbol to vector
    pub fn store_embedding_metadata(
        &self,
        symbol_id: &str,
        vector_id: &str,
        model_name: &str,
        embedding_hash: Option<&str>,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        self.conn.execute(
            "INSERT OR REPLACE INTO embeddings
             (symbol_id, vector_id, model_name, embedding_hash, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![symbol_id, vector_id, model_name, embedding_hash, now],
        )?;

        debug!(
            "Stored embedding metadata: symbol={}, vector={}, model={}",
            symbol_id, vector_id, model_name
        );
        Ok(())
    }

    /// 🚀 BLAZING-FAST bulk embedding storage for batch processing
    /// Inserts both vectors and metadata in a single transaction
    pub fn bulk_store_embeddings(
        &mut self,
        embeddings: &[(String, Vec<f32>)], // (symbol_id, vector)
        dimensions: usize,
        model_name: &str,
    ) -> Result<()> {
        if embeddings.is_empty() {
            return Ok(());
        }

        let start_time = std::time::Instant::now();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Use transaction for atomic bulk insert
        let tx = self.conn.transaction()?;

        // Prepare statements for batch insert
        let mut vector_stmt = tx.prepare(
            "INSERT OR REPLACE INTO embedding_vectors
             (vector_id, dimensions, vector_data, model_name, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        let mut metadata_stmt = tx.prepare(
            "INSERT OR REPLACE INTO embeddings
             (symbol_id, vector_id, model_name, embedding_hash, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        for (symbol_id, vector_data) in embeddings {
            // Serialize vector to bytes
            let bytes: Vec<u8> = vector_data.iter().flat_map(|f| f.to_le_bytes()).collect();

            // Insert vector data (using symbol_id as vector_id for simplicity)
            vector_stmt.execute(params![
                symbol_id,
                dimensions as i64,
                bytes,
                model_name,
                now
            ])?;

            // Insert metadata linking symbol to vector
            metadata_stmt.execute(params![
                symbol_id,
                symbol_id, // vector_id = symbol_id
                model_name,
                None::<String>, // embedding_hash
                now
            ])?;
        }

        // Drop statements before committing
        drop(vector_stmt);
        drop(metadata_stmt);
        tx.commit()?;

        let duration = start_time.elapsed();
        info!(
            "✅ Bulk embedding storage complete! {} embeddings in {:.2}ms ({:.0} embeddings/sec)",
            embeddings.len(),
            duration.as_millis(),
            embeddings.len() as f64 / duration.as_secs_f64()
        );

        Ok(())
    }

    /// Get embedding vector for a specific symbol
    pub fn get_embedding_for_symbol(
        &self,
        symbol_id: &str,
        model_name: &str,
    ) -> Result<Option<Vec<f32>>> {
        // First get the vector_id from embeddings metadata table
        let vector_id_result = self.conn.query_row(
            "SELECT vector_id FROM embeddings WHERE symbol_id = ?1 AND model_name = ?2",
            params![symbol_id, model_name],
            |row| row.get::<_, String>(0),
        );

        match vector_id_result {
            Ok(vector_id) => {
                // Then fetch the actual vector data
                self.get_embedding_vector(&vector_id)
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow!("Failed to get embedding metadata: {}", e)),
        }
    }

    /// Delete embedding vector and metadata
    pub fn delete_embedding(&self, vector_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM embedding_vectors WHERE vector_id = ?1",
            params![vector_id],
        )?;

        // Metadata will cascade delete automatically due to FK constraint
        debug!("Deleted embedding vector: {}", vector_id);
        Ok(())
    }

    /// Delete embeddings for a specific symbol
    pub fn delete_embeddings_for_symbol(&self, symbol_id: &str) -> Result<()> {
        // Get all vector_ids before deleting metadata
        let mut stmt = self
            .conn
            .prepare("SELECT vector_id FROM embeddings WHERE symbol_id = ?1")?;
        let vector_ids: Vec<String> = stmt
            .query_map(params![symbol_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        // Delete metadata (cascades due to FK)
        self.conn.execute(
            "DELETE FROM embeddings WHERE symbol_id = ?1",
            params![symbol_id],
        )?;

        // Delete vector data
        for vector_id in vector_ids {
            self.conn.execute(
                "DELETE FROM embedding_vectors WHERE vector_id = ?1",
                params![vector_id],
            )?;
        }

        debug!("Deleted embeddings for symbol: {}", symbol_id);
        Ok(())
    }

    /// Load all embeddings for a specific model from disk into memory
    pub fn load_all_embeddings(&self, model_name: &str) -> Result<HashMap<String, Vec<f32>>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.symbol_id, ev.vector_data, ev.dimensions
             FROM embeddings e
             JOIN embedding_vectors ev ON e.vector_id = ev.vector_id
             WHERE e.model_name = ?1",
        )?;

        let rows = stmt.query_map(params![model_name], |row| {
            let symbol_id: String = row.get(0)?;
            let bytes: Vec<u8> = row.get(1)?;
            let dimensions: i64 = row.get(2)?;
            Ok((symbol_id, bytes, dimensions))
        })?;

        let mut embeddings = HashMap::new();
        let mut loaded_count = 0;

        for row_result in rows {
            let (symbol_id, bytes, dimensions) = row_result?;

            // Deserialize bytes to f32 vector
            if bytes.len() != (dimensions as usize * 4) {
                warn!(
                    "Skipping corrupted embedding for symbol {}: invalid size",
                    symbol_id
                );
                continue;
            }

            let vector: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();

            embeddings.insert(symbol_id, vector);
            loaded_count += 1;
        }

        info!(
            "Loaded {} embeddings for model '{}' from disk",
            loaded_count, model_name
        );
        Ok(embeddings)
    }

    /// Count total embeddings for a workspace (for registry status reconciliation)
    pub fn count_embeddings(&self, workspace_id: &str) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM embeddings e
             JOIN symbols s ON e.symbol_id = s.id
             WHERE s.workspace_id = ?1",
            params![workspace_id],
            |row| row.get(0),
        )?;

        Ok(count)
    }

    /// Get symbols for a specific workspace (optimized for background tasks)
    pub fn get_symbols_for_workspace(&self, workspace_id: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, name, kind, language, file_path, signature,
                   start_line, start_col, end_line, end_col, start_byte, end_byte,
                   doc_comment, visibility, code_context, parent_id,
                   metadata, semantic_group, confidence
            FROM symbols
            WHERE workspace_id = ?1
            ORDER BY file_path, start_line
        ",
        )?;

        let rows = stmt.query_map([workspace_id], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row_result in rows {
            symbols.push(row_result?);
        }

        debug!(
            "Retrieved {} symbols for workspace '{}' from database",
            symbols.len(),
            workspace_id
        );
        Ok(symbols)
    }

    /// Get file hashes for a specific workspace for incremental update detection
    pub fn get_file_hashes_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<std::collections::HashMap<String, String>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT path, hash
            FROM files
            WHERE workspace_id = ?1
            ORDER BY path
        ",
        )?;

        let rows = stmt.query_map([workspace_id], |row| {
            Ok((
                row.get::<_, String>(0)?, // path
                row.get::<_, String>(1)?, // hash
            ))
        })?;

        let mut file_hashes = std::collections::HashMap::new();
        for row_result in rows {
            let (path, hash) = row_result?;
            file_hashes.insert(path, hash);
        }

        debug!(
            "Retrieved {} file hashes for workspace '{}' from database",
            file_hashes.len(),
            workspace_id
        );
        Ok(file_hashes)
    }

    /// Get symbols in batches for memory-efficient processing (for large codebases)
    pub fn get_symbols_batch(
        &self,
        workspace_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, name, kind, language, file_path, signature,
                   start_line, start_col, end_line, end_col, start_byte, end_byte,
                   doc_comment, visibility, code_context, parent_id,
                   metadata, semantic_group, confidence
            FROM symbols
            WHERE workspace_id = ?1
            ORDER BY file_path, start_line
            LIMIT ?2 OFFSET ?3
        ",
        )?;

        let rows = stmt.query_map(
            [workspace_id, &limit.to_string(), &offset.to_string()],
            |row| self.row_to_symbol(row),
        )?;

        let mut symbols = Vec::new();
        for row_result in rows {
            symbols.push(row_result?);
        }

        debug!(
            "Retrieved batch of {} symbols (offset: {}, limit: {}) for workspace '{}'",
            symbols.len(),
            offset,
            limit,
            workspace_id
        );
        Ok(symbols)
    }

    /// Get total symbol count for a workspace (for progress tracking)
    pub fn get_symbol_count_for_workspace(&self, workspace_id: &str) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM symbols WHERE workspace_id = ?1",
            params![workspace_id],
            |row| row.get(0),
        )?;

        Ok(count)
    }

    /// Get total file count for a workspace (for registry statistics)
    pub fn get_file_count_for_workspace(&self, workspace_id: &str) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM files WHERE workspace_id = ?1",
            params![workspace_id],
            |row| row.get(0),
        )?;

        Ok(count)
    }

    /// Get all indexed file paths for a workspace (for staleness detection)
    ///
    /// Returns a vector of relative file paths that are currently indexed in the database
    pub fn get_all_indexed_files(&self, workspace_id: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path FROM files WHERE workspace_id = ?1")?;

        let file_paths: Vec<String> = stmt
            .query_map(params![workspace_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;

        Ok(file_paths)
    }

    /// Check if workspace has any symbols (quick health check)
    pub fn has_symbols_for_workspace(&self, workspace_id: &str) -> Result<bool> {
        let exists: i64 = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM symbols WHERE workspace_id = ?1 LIMIT 1)",
            params![workspace_id],
            |row| row.get(0),
        )?;

        Ok(exists > 0)
    }

    /// Count total symbols for a workspace (for statistics)
    pub fn count_symbols_for_workspace(&self, workspace_id: &str) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM symbols WHERE workspace_id = ?1",
            params![workspace_id],
            |row| row.get(0),
        )?;

        Ok(count as usize)
    }

    /// Query symbols by name pattern (LIKE search) with optional filters
    /// Uses idx_symbols_name, idx_symbols_language, idx_symbols_workspace for fast lookup
    pub fn query_symbols_by_name_pattern(
        &self,
        pattern: &str,
        language: Option<&str>,
        workspace_ids: &[String],
    ) -> Result<Vec<Symbol>> {
        let pattern_like = format!("%{}%", pattern);

        let query = if !workspace_ids.is_empty() {
            let workspace_placeholders = workspace_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            if let Some(_lang) = language {
                format!(
                    "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                            end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                            parent_id, metadata, semantic_group, confidence
                     FROM symbols
                     WHERE (name LIKE ?1 OR code_context LIKE ?1) AND language = ?2 AND workspace_id IN ({})
                     ORDER BY name, file_path
                     LIMIT 1000",
                    workspace_placeholders
                )
            } else {
                format!(
                    "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                            end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                            parent_id, metadata, semantic_group, confidence
                     FROM symbols
                     WHERE (name LIKE ?1 OR code_context LIKE ?1) AND workspace_id IN ({})
                     ORDER BY name, file_path
                     LIMIT 1000",
                    workspace_placeholders
                )
            }
        } else if language.is_some() {
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence
             FROM symbols
             WHERE (name LIKE ?1 OR code_context LIKE ?1) AND language = ?2
             ORDER BY name, file_path
             LIMIT 1000"
                .to_string()
        } else {
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence
             FROM symbols
             WHERE (name LIKE ?1 OR code_context LIKE ?1)
             ORDER BY name, file_path
             LIMIT 1000"
                .to_string()
        };

        let mut stmt = self.conn.prepare(&query)?;

        // Build params dynamically
        let symbols = if let Some(lang) = language {
            let mut params_vec: Vec<&dyn rusqlite::ToSql> = vec![&pattern_like, &lang];
            for ws_id in workspace_ids {
                params_vec.push(ws_id);
            }
            let rows = stmt.query_map(params_vec.as_slice(), |row| self.row_to_symbol(row))?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            result
        } else {
            let mut params_vec: Vec<&dyn rusqlite::ToSql> = vec![&pattern_like];
            for ws_id in workspace_ids {
                params_vec.push(ws_id);
            }
            let rows = stmt.query_map(params_vec.as_slice(), |row| self.row_to_symbol(row))?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            result
        };

        Ok(symbols)
    }

    /// Query symbols by kind with workspace filtering
    /// Uses idx_symbols_kind, idx_symbols_workspace for fast lookup
    pub fn query_symbols_by_kind(
        &self,
        kind: &SymbolKind,
        workspace_ids: &[String],
    ) -> Result<Vec<Symbol>> {
        let kind_str = match kind {
            SymbolKind::Function => "function",
            SymbolKind::Method => "method",
            SymbolKind::Class => "class",
            SymbolKind::Interface => "interface",
            SymbolKind::Enum => "enum",
            SymbolKind::Struct => "struct",
            SymbolKind::Variable => "variable",
            SymbolKind::Constant => "constant",
            SymbolKind::Property => "property",
            SymbolKind::Module => "module",
            SymbolKind::Namespace => "namespace",
            SymbolKind::Type => "type",
            SymbolKind::Trait => "trait",
            SymbolKind::Union => "union",
            SymbolKind::Field => "field",
            SymbolKind::Constructor => "constructor",
            SymbolKind::Destructor => "destructor",
            SymbolKind::Operator => "operator",
            SymbolKind::Import => "import",
            SymbolKind::Export => "export",
            SymbolKind::Event => "event",
            SymbolKind::Delegate => "delegate",
            SymbolKind::EnumMember => "enum_member",
        };

        let query = if !workspace_ids.is_empty() {
            let workspace_placeholders = workspace_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                        end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                        parent_id, metadata, semantic_group, confidence
                 FROM symbols
                 WHERE kind = ?1 AND workspace_id IN ({})
                 ORDER BY file_path, start_line",
                workspace_placeholders
            )
        } else {
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence
             FROM symbols
             WHERE kind = ?1
             ORDER BY file_path, start_line"
                .to_string()
        };

        let mut stmt = self.conn.prepare(&query)?;

        let mut params_vec: Vec<&dyn rusqlite::ToSql> = vec![&kind_str];
        for ws_id in workspace_ids {
            params_vec.push(ws_id);
        }

        let rows = stmt.query_map(params_vec.as_slice(), |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row in rows {
            symbols.push(row?);
        }

        Ok(symbols)
    }

    /// Query symbols by language with workspace filtering
    /// Uses idx_symbols_language, idx_symbols_workspace for fast lookup
    pub fn query_symbols_by_language(
        &self,
        language: &str,
        workspace_ids: &[String],
    ) -> Result<Vec<Symbol>> {
        let query = if !workspace_ids.is_empty() {
            let workspace_placeholders = workspace_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                        end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                        parent_id, metadata, semantic_group, confidence
                 FROM symbols
                 WHERE language = ?1 AND workspace_id IN ({})
                 ORDER BY file_path, start_line",
                workspace_placeholders
            )
        } else {
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence
             FROM symbols
             WHERE language = ?1
             ORDER BY file_path, start_line"
                .to_string()
        };

        let mut stmt = self.conn.prepare(&query)?;

        let mut params_vec: Vec<&dyn rusqlite::ToSql> = vec![&language];
        for ws_id in workspace_ids {
            params_vec.push(ws_id);
        }

        let rows = stmt.query_map(params_vec.as_slice(), |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row in rows {
            symbols.push(row?);
        }

        Ok(symbols)
    }

    /// Get aggregate symbol statistics (fast COUNT queries with GROUP BY)
    /// Returns counts by kind and by language
    pub fn get_symbol_statistics(
        &self,
        workspace_ids: &[String],
    ) -> Result<(
        std::collections::HashMap<String, usize>,
        std::collections::HashMap<String, usize>,
    )> {
        use std::collections::HashMap;

        let mut by_kind = HashMap::new();
        let mut by_language = HashMap::new();

        // Count by kind
        if !workspace_ids.is_empty() {
            let workspace_placeholders = workspace_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let kind_query = format!(
                "SELECT kind, COUNT(*) as count FROM symbols WHERE workspace_id IN ({}) GROUP BY kind",
                workspace_placeholders
            );

            let mut stmt = self.conn.prepare(&kind_query)?;
            let params: Vec<&dyn rusqlite::ToSql> = workspace_ids
                .iter()
                .map(|id| id as &dyn rusqlite::ToSql)
                .collect();
            let rows = stmt.query_map(params.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?;

            for row in rows {
                let (kind, count) = row?;
                by_kind.insert(kind, count);
            }
        } else {
            let kind_query = "SELECT kind, COUNT(*) as count FROM symbols GROUP BY kind";
            let mut stmt = self.conn.prepare(kind_query)?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?;

            for row in rows {
                let (kind, count) = row?;
                by_kind.insert(kind, count);
            }
        }

        // Count by language
        if !workspace_ids.is_empty() {
            let workspace_placeholders = workspace_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let lang_query = format!(
                "SELECT language, COUNT(*) as count FROM symbols WHERE workspace_id IN ({}) GROUP BY language",
                workspace_placeholders
            );

            let mut stmt = self.conn.prepare(&lang_query)?;
            let params: Vec<&dyn rusqlite::ToSql> = workspace_ids
                .iter()
                .map(|id| id as &dyn rusqlite::ToSql)
                .collect();
            let rows = stmt.query_map(params.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?;

            for row in rows {
                let (language, count) = row?;
                by_language.insert(language, count);
            }
        } else {
            let lang_query = "SELECT language, COUNT(*) as count FROM symbols GROUP BY language";
            let mut stmt = self.conn.prepare(lang_query)?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?;

            for row in rows {
                let (language, count) = row?;
                by_language.insert(language, count);
            }
        }

        Ok((by_kind, by_language))
    }

    /// Get file-level statistics using SQL GROUP BY (O(log n) instead of O(n))
    ///
    /// Returns: HashMap<file_path, symbol_count>
    pub fn get_file_statistics(
        &self,
        workspace_ids: &[String],
    ) -> Result<std::collections::HashMap<String, usize>> {
        use std::collections::HashMap;

        let mut by_file = HashMap::new();

        // Count symbols per file using SQL GROUP BY
        if !workspace_ids.is_empty() {
            let workspace_placeholders = workspace_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let file_query = format!(
                "SELECT file_path, COUNT(*) as count FROM symbols WHERE workspace_id IN ({}) GROUP BY file_path",
                workspace_placeholders
            );

            let mut stmt = self.conn.prepare(&file_query)?;
            let params: Vec<&dyn rusqlite::ToSql> = workspace_ids
                .iter()
                .map(|id| id as &dyn rusqlite::ToSql)
                .collect();
            let rows = stmt.query_map(params.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?;

            for row in rows {
                let (file_path, count) = row?;
                by_file.insert(file_path, count);
            }
        } else {
            let file_query = "SELECT file_path, COUNT(*) as count FROM symbols GROUP BY file_path";
            let mut stmt = self.conn.prepare(file_query)?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?;

            for row in rows {
                let (file_path, count) = row?;
                by_file.insert(file_path, count);
            }
        }

        Ok(by_file)
    }

    /// Get total symbol count using SQL COUNT (O(1) database operation)
    pub fn get_total_symbol_count(&self, workspace_ids: &[String]) -> Result<usize> {
        let count: i64 = if !workspace_ids.is_empty() {
            let workspace_placeholders = workspace_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let count_query = format!(
                "SELECT COUNT(*) FROM symbols WHERE workspace_id IN ({})",
                workspace_placeholders
            );

            let mut stmt = self.conn.prepare(&count_query)?;
            let params: Vec<&dyn rusqlite::ToSql> = workspace_ids
                .iter()
                .map(|id| id as &dyn rusqlite::ToSql)
                .collect();
            stmt.query_row(params.as_slice(), |row| row.get(0))?
        } else {
            let count_query = "SELECT COUNT(*) FROM symbols";
            let mut stmt = self.conn.prepare(count_query)?;
            stmt.query_row([], |row| row.get(0))?
        };

        Ok(count as usize)
    }

    /// Get file-level relationship statistics using SQL (for hotspot analysis)
    ///
    /// Returns: HashMap<file_path, relationship_count> counting relationships where symbols from this file participate
    pub fn get_file_relationship_statistics(
        &self,
        workspace_ids: &[String],
    ) -> Result<std::collections::HashMap<String, usize>> {
        use std::collections::HashMap;

        let mut by_file = HashMap::new();

        // This is a more complex query: count relationships per file
        // We need to join symbols with relationships to count how many relationships involve symbols from each file
        if !workspace_ids.is_empty() {
            let workspace_placeholders = workspace_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let rel_query = format!(
                "SELECT s.file_path, COUNT(DISTINCT r.id) as count \
                 FROM symbols s \
                 LEFT JOIN relationships r ON (r.from_symbol_id = s.id OR r.to_symbol_id = s.id) \
                 WHERE s.workspace_id IN ({}) \
                 GROUP BY s.file_path",
                workspace_placeholders
            );

            let mut stmt = self.conn.prepare(&rel_query)?;
            let params: Vec<&dyn rusqlite::ToSql> = workspace_ids
                .iter()
                .map(|id| id as &dyn rusqlite::ToSql)
                .collect();
            let rows = stmt.query_map(params.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?;

            for row in rows {
                let (file_path, count) = row?;
                by_file.insert(file_path, count);
            }
        } else {
            let rel_query = "SELECT s.file_path, COUNT(DISTINCT r.id) as count \
                             FROM symbols s \
                             LEFT JOIN relationships r ON (r.from_symbol_id = s.id OR r.to_symbol_id = s.id) \
                             GROUP BY s.file_path";

            let mut stmt = self.conn.prepare(rel_query)?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?;

            for row in rows {
                let (file_path, count) = row?;
                by_file.insert(file_path, count);
            }
        }

        Ok(by_file)
    }

    /// Get relationship type statistics using SQL aggregation (avoids loading all relationships into memory)
    /// Returns HashMap<relationship_kind, count> grouped by relationship type
    /// Used by FastExploreTool's intelligent_dependencies mode
    pub fn get_relationship_type_statistics(
        &self,
        workspace_ids: &[String],
    ) -> Result<HashMap<String, i64>> {
        let mut by_kind = HashMap::new();

        // SQL GROUP BY aggregation - counts relationships by kind without loading data into memory
        if !workspace_ids.is_empty() {
            let workspace_placeholders = workspace_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let query = format!(
                "SELECT kind, COUNT(*) as count \
                 FROM relationships \
                 WHERE workspace_id IN ({}) \
                 GROUP BY kind",
                workspace_placeholders
            );

            let mut stmt = self.conn.prepare(&query)?;
            let params: Vec<&dyn rusqlite::ToSql> = workspace_ids
                .iter()
                .map(|id| id as &dyn rusqlite::ToSql)
                .collect();
            let rows = stmt.query_map(params.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;

            for row in rows {
                let (kind, count) = row?;
                by_kind.insert(kind, count);
            }
        } else {
            // No workspace filter - count all relationships
            let query = "SELECT kind, COUNT(*) as count \
                         FROM relationships \
                         GROUP BY kind";

            let mut stmt = self.conn.prepare(query)?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;

            for row in rows {
                let (kind, count) = row?;
                by_kind.insert(kind, count);
            }
        }

        Ok(by_kind)
    }

    /// Get most referenced symbols using SQL aggregation (avoids loading all relationships into memory)
    /// Returns Vec<(symbol_id, reference_count)> sorted by count descending
    /// Used by FastExploreTool's intelligent_dependencies mode to find heavily referenced symbols
    pub fn get_most_referenced_symbols(
        &self,
        workspace_ids: &[String],
        limit: usize,
    ) -> Result<Vec<(String, usize)>> {
        let mut results = Vec::new();

        // SQL GROUP BY aggregation - counts incoming references per symbol
        if !workspace_ids.is_empty() {
            let workspace_placeholders = workspace_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let query = format!(
                "SELECT to_symbol_id, COUNT(*) as ref_count \
                 FROM relationships \
                 WHERE workspace_id IN ({}) \
                 GROUP BY to_symbol_id \
                 ORDER BY ref_count DESC \
                 LIMIT ?",
                workspace_placeholders
            );

            let mut stmt = self.conn.prepare(&query)?;
            let mut params: Vec<&dyn rusqlite::ToSql> = workspace_ids
                .iter()
                .map(|id| id as &dyn rusqlite::ToSql)
                .collect();
            params.push(&limit);

            let rows = stmt.query_map(params.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?;

            for row in rows {
                let (symbol_id, count) = row?;
                results.push((symbol_id, count));
            }
        } else {
            // No workspace filter - count all references
            let query = "SELECT to_symbol_id, COUNT(*) as ref_count \
                         FROM relationships \
                         GROUP BY to_symbol_id \
                         ORDER BY ref_count DESC \
                         LIMIT ?";

            let mut stmt = self.conn.prepare(query)?;
            let rows = stmt.query_map([limit], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?;

            for row in rows {
                let (symbol_id, count) = row?;
                results.push((symbol_id, count));
            }
        }

        Ok(results)
    }

    /// Delete all data for a specific workspace (for workspace cleanup)
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

/// Statistics returned after workspace cleanup
#[derive(Debug, Clone)]
pub struct WorkspaceCleanupStats {
    pub symbols_deleted: i64,
    pub relationships_deleted: i64,
    pub files_deleted: i64,
}

/// Usage statistics for a workspace (for LRU eviction)
#[derive(Debug, Clone)]
pub struct WorkspaceUsageStats {
    pub workspace_id: String,
    pub symbol_count: i64,
    pub file_count: i64,
    pub total_size_bytes: i64,
}

/// Utility function to calculate Blake3 hash of file content
pub fn calculate_file_hash<P: AsRef<Path>>(file_path: P) -> Result<String> {
    let content = std::fs::read(file_path)?;
    let hash = blake3::hash(&content);
    Ok(hash.to_hex().to_string())
}

/// Create FileInfo from a file path
/// CASCADE: Now reads and includes file content for FTS5 search
pub fn create_file_info<P: AsRef<Path>>(file_path: P, language: &str) -> Result<FileInfo> {
    let path = file_path.as_ref();
    let metadata = std::fs::metadata(path)?;
    let hash = calculate_file_hash(path)?;

    // CASCADE: Read file content for FTS5 search
    let content = std::fs::read_to_string(path).ok(); // Binary files or read errors - skip content

    let last_modified = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;

    // CRITICAL FIX: Canonicalize path to resolve symlinks (macOS /var vs /private/var)
    // This ensures files table and symbols table use same canonical paths
    // Without this: files table has /var/..., symbols have /private/var/... → FOREIGN KEY fail
    let canonical_path = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string();

    Ok(FileInfo {
        path: canonical_path, // Use canonical path, not original
        language: language.to_string(),
        hash,
        size: metadata.len() as i64,
        last_modified,
        last_indexed: 0, // Will be set by database
        symbol_count: 0, // Will be updated after extraction
        content,         // CASCADE: File content for FTS5
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractors::SymbolKind;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use tree_sitter::Parser;

    #[tokio::test]
    async fn test_database_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = SymbolDatabase::new(&db_path).unwrap();
        let stats = db.get_stats().unwrap();

        assert_eq!(stats.total_symbols, 0);
        assert_eq!(stats.total_relationships, 0);
        assert_eq!(stats.total_files, 0);
    }

    #[test]
    fn test_minimal_database_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("minimal.db");

        // Test just the SQLite connection
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Test a simple table creation
        let result = conn.execute("CREATE TABLE test (id TEXT PRIMARY KEY, name TEXT)", []);

        // This should work without "Execute returned results" error
        assert!(result.is_ok());

        // Test a simple insert
        let insert_result = conn.execute("INSERT INTO test VALUES ('1', 'test')", []);
        assert!(insert_result.is_ok());
    }

    #[tokio::test]
    async fn test_debug_foreign_key_constraint() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("debug.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Create a temporary file
        let test_file = temp_dir.path().join("test.ts");
        std::fs::write(&test_file, "// test content").unwrap();

        // Store file info
        let file_info = crate::database::create_file_info(&test_file, "typescript").unwrap();
        println!("File path in file_info: {}", file_info.path);
        db.store_file_info(&file_info, "test").unwrap();

        // Create a symbol with the same file path (canonicalized to match file_info)
        let file_path = test_file
            .canonicalize()
            .unwrap_or_else(|_| test_file.clone())
            .to_string_lossy()
            .to_string();
        println!("File path in symbol: {}", file_path);

        let symbol = Symbol {
            id: "test-symbol".to_string(),
            name: "testFunction".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: file_path,
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 10,
            start_byte: 0,
            end_byte: 10,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        };

        // This should work without foreign key constraint error
        let result = db.store_symbols(&[symbol], "test");
        assert!(
            result.is_ok(),
            "Foreign key constraint failed: {:?}",
            result
        );
    }

    #[test]
    fn test_individual_table_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("individual.db");

        // Create a SymbolDatabase instance manually to test each table individually
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let db = SymbolDatabase {
            conn,
            file_path: db_path,
        };

        // Test files table creation
        let files_result = db.create_files_table();
        assert!(
            files_result.is_ok(),
            "Files table creation failed: {:?}",
            files_result
        );

        // Test symbols table creation
        let symbols_result = db.create_symbols_table();
        assert!(
            symbols_result.is_ok(),
            "Symbols table creation failed: {:?}",
            symbols_result
        );

        // Test relationships table creation
        let relationships_result = db.create_relationships_table();
        assert!(
            relationships_result.is_ok(),
            "Relationships table creation failed: {:?}",
            relationships_result
        );

        // Test embeddings table creation
        let embeddings_result = db.create_embeddings_table();
        assert!(
            embeddings_result.is_ok(),
            "Embeddings table creation failed: {:?}",
            embeddings_result
        );
    }

    #[tokio::test]
    async fn test_file_info_storage() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        let file_info = FileInfo {
            path: "test.rs".to_string(),
            language: "rust".to_string(),
            hash: "abcd1234".to_string(),
            size: 1024,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 5,
            content: None,
        };

        db.store_file_info(&file_info, "test").unwrap();

        let hash = db.get_file_hash("test.rs").unwrap();
        assert_eq!(hash, Some("abcd1234".to_string()));
    }

    #[tokio::test]
    async fn test_symbol_storage_and_retrieval() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        let symbol = Symbol {
            id: "test-symbol-1".to_string(),
            name: "test_function".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "test.rs".to_string(),
            start_line: 10,
            start_column: 0,
            end_line: 15,
            end_column: 1,
            start_byte: 0,
            end_byte: 0,
            signature: Some("fn test_function()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        };

        // Following foreign key contract: store file record first
        let file_info = FileInfo {
            path: "test.rs".to_string(),
            language: "rust".to_string(),
            hash: "test-hash".to_string(),
            size: 100,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 1,
            content: None,
        };
        db.store_file_info(&file_info, "test").unwrap();

        db.store_symbols(&[symbol.clone()], "test").unwrap();

        let retrieved = db.get_symbol_by_id("test-symbol-1").unwrap();
        assert!(retrieved.is_some());

        let retrieved_symbol = retrieved.unwrap();
        assert_eq!(retrieved_symbol.name, "test_function");
        assert_eq!(retrieved_symbol.language, "rust");
    }

    #[test]
    fn test_bulk_store_symbols_for_existing_file_paths() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("bulk.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        // Use a real Go fixture to mirror the production failure scenario
        let fixture_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/real-world/go/main.go");
        let fixture_content = std::fs::read_to_string(&fixture_path).unwrap();

        let file_info = crate::database::create_file_info(&fixture_path, "go").unwrap();
        db.bulk_store_files(&[file_info], "test_workspace").unwrap();

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(&fixture_content, None).unwrap();

        let mut extractor = crate::extractors::go::GoExtractor::new(
            "go".to_string(),
            fixture_path.to_string_lossy().to_string(),
            fixture_content,
        );
        let symbols = extractor.extract_symbols(&tree);

        assert!(!symbols.is_empty(), "Expected fixture to produce symbols");

        let result = db.bulk_store_symbols(&symbols, "test_workspace");
        assert!(
            result.is_ok(),
            "Bulk store should succeed without foreign key violations: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_symbol_with_metadata_and_semantic_fields() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Create a temporary file for the test
        let test_file = temp_dir.path().join("user.ts");
        std::fs::write(&test_file, "// test file content").unwrap();

        // Create symbol with all new fields populated
        let mut metadata = HashMap::new();
        metadata.insert("isAsync".to_string(), serde_json::Value::Bool(true));
        metadata.insert(
            "returnType".to_string(),
            serde_json::Value::String("Promise<User>".to_string()),
        );

        let symbol = Symbol {
            id: "test-symbol-complex".to_string(),
            name: "getUserAsync".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: test_file
                .canonicalize()
                .unwrap_or_else(|_| test_file.clone())
                .to_string_lossy()
                .to_string(),
            start_line: 20,
            start_column: 4,
            end_line: 30,
            end_column: 1,
            start_byte: 500,
            end_byte: 800,
            signature: Some("async getUserAsync(id: string): Promise<User>".to_string()),
            doc_comment: Some("Fetches user data asynchronously".to_string()),
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: None, // No parent for this test
            metadata: Some(metadata.clone()),
            semantic_group: Some("user-data-access".to_string()),
            confidence: Some(0.95),
            code_context: None,
        };

        // First, store the file record (required due to foreign key constraint)
        let file_info = crate::database::create_file_info(&test_file, "typescript").unwrap();
        println!("DEBUG: File path in file_info: {}", file_info.path);
        println!("DEBUG: Symbol file path: {}", symbol.file_path);
        db.store_file_info(&file_info, "test").unwrap();

        // Store the symbol
        db.store_symbols(&[symbol.clone()], "test").unwrap();

        // Retrieve and verify all fields are preserved
        let retrieved = db.get_symbol_by_id("test-symbol-complex").unwrap().unwrap();

        assert_eq!(retrieved.name, "getUserAsync");
        assert_eq!(
            retrieved.semantic_group,
            Some("user-data-access".to_string())
        );
        assert_eq!(retrieved.confidence, Some(0.95));

        // Verify metadata is properly stored and retrieved
        let retrieved_metadata = retrieved.metadata.unwrap();
        assert_eq!(
            retrieved_metadata
                .get("isAsync")
                .unwrap()
                .as_bool()
                .unwrap(),
            true
        );
        assert_eq!(
            retrieved_metadata
                .get("returnType")
                .unwrap()
                .as_str()
                .unwrap(),
            "Promise<User>"
        );
    }

    #[tokio::test]
    async fn test_relationship_with_id_field() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Following foreign key contract: create file and symbols first
        let file_info = FileInfo {
            path: "main.rs".to_string(),
            language: "rust".to_string(),
            hash: "main-hash".to_string(),
            size: 500,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 2,
            content: None,
        };
        db.store_file_info(&file_info, "test").unwrap();

        let caller_symbol = Symbol {
            id: "caller_func".to_string(),
            name: "caller_func".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "main.rs".to_string(),
            start_line: 10,
            start_column: 0,
            end_line: 15,
            end_column: 1,
            start_byte: 0,
            end_byte: 0,
            signature: Some("fn caller_func()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        };

        let called_symbol = Symbol {
            id: "called_func".to_string(),
            name: "called_func".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "main.rs".to_string(),
            start_line: 20,
            start_column: 0,
            end_line: 25,
            end_column: 1,
            start_byte: 0,
            end_byte: 0,
            signature: Some("fn called_func()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        };

        db.store_symbols(&[caller_symbol, called_symbol], "test")
            .unwrap();

        // Create relationship with generated id
        let relationship = crate::extractors::base::Relationship {
            id: "caller_func_called_func_Calls_42".to_string(),
            from_symbol_id: "caller_func".to_string(),
            to_symbol_id: "called_func".to_string(),
            kind: crate::extractors::base::RelationshipKind::Calls,
            file_path: "main.rs".to_string(),
            line_number: 42,
            confidence: 0.9,
            metadata: None,
        };

        // Store the relationship
        db.store_relationships(&[relationship.clone()], "test")
            .unwrap();

        // Retrieve relationships for the from_symbol
        let relationships = db.get_relationships_for_symbol("caller_func").unwrap();
        assert_eq!(relationships.len(), 1);

        let retrieved = &relationships[0];
        assert_eq!(retrieved.id, "caller_func_called_func_Calls_42");
        assert_eq!(retrieved.from_symbol_id, "caller_func");
        assert_eq!(retrieved.to_symbol_id, "called_func");
        assert_eq!(retrieved.confidence, 0.9);
    }

    #[tokio::test]
    async fn test_cross_language_semantic_grouping() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Create symbols from different languages but same semantic group
        let ts_interface = Symbol {
            id: "ts-user-interface".to_string(),
            name: "User".to_string(),
            kind: SymbolKind::Interface,
            language: "typescript".to_string(),
            file_path: "user.ts".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 10,
            end_column: 1,
            start_byte: 0,
            end_byte: 200,
            signature: Some("interface User".to_string()),
            doc_comment: None,
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: None,
            metadata: None,
            semantic_group: Some("user-entity".to_string()),
            confidence: Some(1.0),
            code_context: None,
        };

        let rust_struct = Symbol {
            id: "rust-user-struct".to_string(),
            name: "User".to_string(),
            kind: SymbolKind::Struct,
            language: "rust".to_string(),
            file_path: "user.rs".to_string(),
            start_line: 5,
            start_column: 0,
            end_line: 15,
            end_column: 1,
            start_byte: 100,
            end_byte: 400,
            signature: Some("struct User".to_string()),
            doc_comment: None,
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: None,
            metadata: None,
            semantic_group: Some("user-entity".to_string()),
            confidence: Some(0.98),
            code_context: None,
        };

        // Following foreign key contract: store file records first
        let ts_file_info = FileInfo {
            path: "user.ts".to_string(),
            language: "typescript".to_string(),
            hash: "ts-hash".to_string(),
            size: 200,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 1,
            content: None,
        };
        db.store_file_info(&ts_file_info, "test").unwrap();

        let rust_file_info = FileInfo {
            path: "user.rs".to_string(),
            language: "rust".to_string(),
            hash: "rust-hash".to_string(),
            size: 300,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 1,
            content: None,
        };
        db.store_file_info(&rust_file_info, "test").unwrap();

        // Store both symbols
        db.store_symbols(&[ts_interface, rust_struct], "test")
            .unwrap();

        // Query symbols by semantic group (this will fail initially - need to implement)
        let grouped_symbols = db.get_symbols_by_semantic_group("user-entity").unwrap();
        assert_eq!(grouped_symbols.len(), 2);

        // Verify we have both TypeScript and Rust symbols
        let languages: std::collections::HashSet<_> = grouped_symbols
            .iter()
            .map(|s| s.language.as_str())
            .collect();
        assert!(languages.contains("typescript"));
        assert!(languages.contains("rust"));
    }

    #[tokio::test]
    async fn test_extractor_database_integration() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Simulate what an extractor would create
        use crate::extractors::base::BaseExtractor;

        let source_code = r#"
        function getUserById(id: string): Promise<User> {
            return fetchUser(id);
        }
        "#;

        // This test will initially fail - we need to verify extractors can create symbols
        // with the new field structure that work with the database
        let base_extractor = BaseExtractor::new(
            "typescript".to_string(),
            "test.ts".to_string(),
            source_code.to_string(),
        );

        // Create a symbol like an extractor would
        let mut metadata = HashMap::new();
        metadata.insert("isAsync".to_string(), serde_json::Value::Bool(false));
        metadata.insert(
            "returnType".to_string(),
            serde_json::Value::String("Promise<User>".to_string()),
        );

        let symbol = Symbol {
            id: base_extractor.generate_id("getUserById", 2, 8),
            name: "getUserById".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "test.ts".to_string(),
            start_line: 2,
            start_column: 8,
            end_line: 4,
            end_column: 9,
            start_byte: 0,
            end_byte: 0,
            signature: Some("function getUserById(id: string): Promise<User>".to_string()),
            doc_comment: None,
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: None,
            metadata: Some(metadata),
            semantic_group: None, // Will be populated during cross-language analysis
            confidence: None,     // Will be calculated based on parsing context
            code_context: None,
        };

        // Following foreign key contract: store file record first
        let file_info = FileInfo {
            path: "test.ts".to_string(),
            language: "typescript".to_string(),
            hash: "test-ts-hash".to_string(),
            size: 150,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 1,
            content: None,
        };
        db.store_file_info(&file_info, "test").unwrap();

        // Test that extractor-generated symbols work with database
        db.store_symbols(&[symbol.clone()], "test").unwrap();

        let retrieved = db.get_symbol_by_id(&symbol.id).unwrap().unwrap();
        assert_eq!(retrieved.name, "getUserById");
        assert!(retrieved.metadata.is_some());

        let metadata = retrieved.metadata.unwrap();
        assert_eq!(
            metadata.get("returnType").unwrap().as_str().unwrap(),
            "Promise<User>"
        );
    }

    /// 🔴 TDD TEST: This test SHOULD FAIL until schema is complete
    /// Tests that all missing database fields are properly persisted and retrieved
    #[tokio::test]
    async fn test_complete_symbol_field_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("complete_fields.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Create file record first (FK requirement)
        let file_info = FileInfo {
            path: "complete_test.rs".to_string(),
            language: "rust".to_string(),
            hash: "complete-hash".to_string(),
            size: 500,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 1,
            content: None,
        };
        db.store_file_info(&file_info, "test_workspace").unwrap();

        // Create symbol with ALL fields populated (including the missing ones)
        let symbol = Symbol {
            id: "complete-symbol-id".to_string(),
            name: "complete_function".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "complete_test.rs".to_string(),
            start_line: 10,
            start_column: 4,
            end_line: 20,
            end_column: 5,
            // 🔴 THESE FIELDS ARE CURRENTLY LOST (not in database schema):
            start_byte: 150,
            end_byte: 450,
            doc_comment: Some("/// This function does something important".to_string()),
            visibility: Some(crate::extractors::base::Visibility::Public),
            code_context: Some(
                "  // line before\n  fn complete_function() {\n  // line after".to_string(),
            ),
            // Regular fields that work:
            signature: Some("fn complete_function() -> Result<()>".to_string()),
            parent_id: None,
            metadata: None,
            semantic_group: Some("test-group".to_string()),
            confidence: Some(0.95),
        };

        // Store the symbol
        db.store_symbols(&[symbol.clone()], "test_workspace")
            .unwrap();

        // Retrieve and verify ALL fields are preserved
        let retrieved = db
            .get_symbol_by_id("complete-symbol-id")
            .unwrap()
            .expect("Symbol should exist in database");

        // Basic fields (these already work)
        assert_eq!(retrieved.name, "complete_function");
        assert_eq!(retrieved.start_line, 10);
        assert_eq!(retrieved.end_line, 20);

        // 🔴 CRITICAL MISSING FIELDS - These assertions will FAIL until schema is fixed:
        assert_eq!(retrieved.start_byte, 150, "start_byte should be persisted");
        assert_eq!(retrieved.end_byte, 450, "end_byte should be persisted");
        assert_eq!(
            retrieved.doc_comment,
            Some("/// This function does something important".to_string()),
            "doc_comment should be persisted"
        );
        assert_eq!(
            retrieved.visibility,
            Some(crate::extractors::base::Visibility::Public),
            "visibility should be persisted"
        );
        assert_eq!(
            retrieved.code_context,
            Some("  // line before\n  fn complete_function() {\n  // line after".to_string()),
            "code_context should be persisted"
        );

        println!("✅ ALL FIELDS PERSISTED CORRECTLY!");
    }

    // ========================================
    // CASCADE ARCHITECTURE: Phase 1 TDD Tests
    // ========================================

    #[test]
    fn test_store_file_with_content() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        db.store_file_with_content(
            "test.md",
            "markdown",
            "abc123",
            1024,
            1234567890,
            "# Test\nThis is test content",
            "test_workspace",
        )
        .unwrap();

        let content = db
            .get_file_content("test.md", Some("test_workspace"))
            .unwrap();
        assert_eq!(content, Some("# Test\nThis is test content".to_string()));
    }

    #[test]
    fn test_fts_search_file_content() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        db.store_file_with_content(
            "docs/architecture.md",
            "markdown",
            "abc123",
            2048,
            1234567890,
            "# Architecture\nSQLite is the single source of truth",
            "test_workspace",
        )
        .unwrap();

        // Search for "SQLite"
        let results = db
            .search_file_content_fts("SQLite", Some("test_workspace"), 10)
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "docs/architecture.md");
        assert!(results[0].snippet.contains("SQLite"));
    }

    #[test]
    fn test_fts_search_ranks_by_relevance() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // File 1: "cascade" appears once in longer document
        db.store_file_with_content(
            "README.md",
            "markdown",
            "abc1",
            1024,
            1234567890,
            "This document describes our cascade architecture pattern for data flow. \
             We use it to propagate changes through the system efficiently. \
             The design ensures consistency across all components.",
            "test_workspace",
        )
        .unwrap();

        // File 2: "cascade" appears five times in longer document
        db.store_file_with_content(
            "CASCADE.md",
            "markdown",
            "abc2",
            2048,
            1234567890,
            "The cascade cascade cascade cascade cascade model is powerful. \
             Our cascade system uses cascade patterns for cascade propagation. \
             Every cascade operation follows the cascade architecture design.",
            "test_workspace",
        )
        .unwrap();

        let results = db
            .search_file_content_fts("cascade", Some("test_workspace"), 10)
            .unwrap();

        // Verify both files are found
        assert_eq!(results.len(), 2);

        // Verify both files match
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.contains(&"README.md"));
        assert!(paths.contains(&"CASCADE.md"));

        // Verify ranks are differentiated (not equal)
        assert_ne!(results[0].rank, results[1].rank);
    }

    #[test]
    fn test_fts_respects_workspace_filter() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        db.store_file_with_content(
            "file1.md",
            "markdown",
            "abc1",
            1024,
            1234567890,
            "workspace A content",
            "workspace_a",
        )
        .unwrap();

        db.store_file_with_content(
            "file2.md",
            "markdown",
            "abc2",
            1024,
            1234567890,
            "workspace B content",
            "workspace_b",
        )
        .unwrap();

        // Search only workspace A
        let results = db
            .search_file_content_fts("content", Some("workspace_a"), 10)
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "file1.md");
    }

    // ============================================================
    // SCHEMA MIGRATION TESTS
    // ============================================================

    #[test]
    fn test_migration_fresh_database_at_latest_version() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = SymbolDatabase::new(&db_path).unwrap();

        // Fresh database should be at latest version
        let version = db.get_schema_version().unwrap();
        assert_eq!(version, LATEST_SCHEMA_VERSION);
    }

    #[test]
    fn test_migration_version_table_exists() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = SymbolDatabase::new(&db_path).unwrap();

        // Verify schema_version table exists
        let result: Result<i64, rusqlite::Error> =
            db.conn
                .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0));

        assert!(result.is_ok(), "schema_version table should exist");
    }

    #[test]
    fn test_migration_adds_content_column() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = SymbolDatabase::new(&db_path).unwrap();

        // Verify content column exists in files table
        let has_content = db.has_column("files", "content").unwrap();
        assert!(
            has_content,
            "files table should have content column after migration"
        );
    }

    #[test]
    fn test_migration_from_legacy_v1_database() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create a legacy V1 database (without content column)
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

            // Create old schema WITHOUT content column
            conn.execute(
                "CREATE TABLE files (
                    path TEXT PRIMARY KEY,
                    language TEXT NOT NULL,
                    hash TEXT NOT NULL,
                    size INTEGER NOT NULL,
                    last_modified INTEGER NOT NULL,
                    last_indexed INTEGER DEFAULT 0,
                    parse_cache BLOB,
                    symbol_count INTEGER DEFAULT 0,
                    workspace_id TEXT NOT NULL DEFAULT 'primary'
                )",
                [],
            )
            .unwrap();

            // Insert test data
            conn.execute(
                "INSERT INTO files (path, language, hash, size, last_modified)
                 VALUES ('test.rs', 'rust', 'abc123', 1024, 1234567890)",
                [],
            )
            .unwrap();
        }

        // Now open with new code - should trigger migration
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Verify migration occurred
        let version = db.get_schema_version().unwrap();
        assert_eq!(
            version, LATEST_SCHEMA_VERSION,
            "Database should be migrated to latest version"
        );

        // Verify content column exists
        let has_content = db.has_column("files", "content").unwrap();
        assert!(has_content, "Migration should have added content column");

        // Verify existing data is preserved
        let file_count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE path = 'test.rs'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            file_count, 1,
            "Existing data should be preserved after migration"
        );
    }

    #[test]
    fn test_migration_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create database (runs migrations)
        {
            let _db = SymbolDatabase::new(&db_path).unwrap();
        }

        // Open again (should handle already-migrated database)
        let db = SymbolDatabase::new(&db_path).unwrap();
        let version = db.get_schema_version().unwrap();
        assert_eq!(version, LATEST_SCHEMA_VERSION);

        // Should not error or change version
        let has_content = db.has_column("files", "content").unwrap();
        assert!(has_content);
    }

    #[test]
    fn test_fts_triggers_work_after_migration() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create legacy V1 database (with workspace_id but without content)
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "CREATE TABLE files (
                    path TEXT PRIMARY KEY,
                    language TEXT NOT NULL,
                    hash TEXT NOT NULL,
                    size INTEGER NOT NULL,
                    last_modified INTEGER NOT NULL,
                    last_indexed INTEGER DEFAULT 0,
                    parse_cache BLOB,
                    symbol_count INTEGER DEFAULT 0,
                    workspace_id TEXT NOT NULL DEFAULT 'primary'
                )",
                [],
            )
            .unwrap();
        }

        // Open and migrate
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Store file with content (should trigger FTS5 sync via triggers)
        db.store_file_with_content(
            "test.rs",
            "rust",
            "hash123",
            1024,
            1234567890,
            "fn main() { println!(\"hello\"); }",
            "primary",
        )
        .unwrap();

        // Verify FTS5 search works (triggers populated FTS table)
        let results = db.search_file_content_fts("main", None, 10).unwrap();
        assert_eq!(results.len(), 1, "FTS search should work after migration");
        assert_eq!(results[0].path, "test.rs");
    }
}
