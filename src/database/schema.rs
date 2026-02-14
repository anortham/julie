// Database schema initialization and table creation

use super::*;
use anyhow::Result;
use tracing::debug;

impl SymbolDatabase {
    /// Initialize the complete database schema
    pub(super) fn initialize_schema(&mut self) -> Result<()> {
        debug!("Creating database schema");

        // Enable foreign key constraints
        self.conn.execute("PRAGMA foreign_keys = ON", [])?;

        // NOTE: WAL mode is now set in SymbolDatabase::new() BEFORE migrations run
        // This ensures WAL is active for all operations including schema changes

        // Create tables in dependency order
        self.create_workspaces_table()?;
        self.create_files_table()?;
        self.create_symbols_table()?;
        self.create_identifiers_table()?; // Reference tracking
        self.create_types_table()?; // Type intelligence
        self.create_relationships_table()?;

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
    pub(crate) fn create_files_table(&self) -> Result<()> {
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
                content TEXT  -- CASCADE: Full file content for FTS
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

        debug!("Created files table and indexes");

        Ok(())
    }

    /// Create the symbols table with rich metadata
    pub(crate) fn create_symbols_table(&self) -> Result<()> {
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

                -- Content type to distinguish documentation from code
                -- NULL = code (default), 'documentation' = markdown docs
                content_type TEXT DEFAULT NULL
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

        debug!("Created symbols table and indexes");

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

        debug!("Created identifiers table and indexes");
        Ok(())
    }

    /// Create the types table for type intelligence
    fn create_types_table(&self) -> Result<()> {
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

        // Essential indexes for fast queries
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

        debug!("Created types table and indexes");
        Ok(())
    }

    /// Create the relationships table for tracing data flow
    pub(crate) fn create_relationships_table(&self) -> Result<()> {
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
                created_at INTEGER DEFAULT 0
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

        debug!("Created relationships table and indexes");
        Ok(())
    }

}
