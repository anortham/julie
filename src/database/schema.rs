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
        self.create_relationships_table()?;
        self.create_embeddings_table()?;

        // Create memory views (depends on files table)
        self.create_memories_view()?;
        self.create_plans_view()?; // Phase 1.5: Mutable plans

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

        // CASCADE: Create FTS5 table and triggers
        self.create_files_fts_table()?;
        self.create_files_fts_triggers()?;

        Ok(())
    }

    /// CASCADE: Create FTS5 virtual table for full-text search on file content
    pub(crate) fn create_files_fts_table(&self) -> Result<()> {
        self.conn.execute(
            r#"CREATE VIRTUAL TABLE IF NOT EXISTS files_fts USING fts5(
                path,
                content,
                tokenize = "unicode61 separators '_::->.'",
                content='files',
                content_rowid='rowid'
            )"#,
            [],
        )?;
        debug!("Created files_fts virtual table with unicode61 tokenizer (separators: _::->.)");
        Ok(())
    }

    /// CASCADE: Create triggers to keep FTS5 in sync with files table
    pub(crate) fn create_files_fts_triggers(&self) -> Result<()> {
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

    /// CASCADE: Disable FTS5 triggers for bulk operations
    /// This prevents row-by-row FTS updates during bulk inserts
    #[allow(dead_code)]
    pub(crate) fn disable_files_fts_triggers(&self) -> Result<()> {
        self.conn.execute("DROP TRIGGER IF EXISTS files_ai", [])?;
        self.conn.execute("DROP TRIGGER IF EXISTS files_ad", [])?;
        self.conn.execute("DROP TRIGGER IF EXISTS files_au", [])?;
        debug!("Disabled files FTS5 triggers for bulk operation");
        Ok(())
    }

    /// CASCADE: Re-enable FTS5 triggers after bulk operations
    #[allow(dead_code)]
    pub(crate) fn enable_files_fts_triggers(&self) -> Result<()> {
        self.create_files_fts_triggers()?;
        debug!("Re-enabled files FTS5 triggers");
        Ok(())
    }

    /// CASCADE: Rebuild files FTS5 index atomically
    /// Use after bulk operations with disabled triggers
    pub(crate) fn rebuild_files_fts(&self) -> Result<()> {
        debug!("Rebuilding files FTS5 index...");
        // First, delete all existing FTS content
        self.conn
            .execute("INSERT INTO files_fts(files_fts) VALUES('delete-all')", [])?;
        // Then rebuild from base table
        self.conn
            .execute("INSERT INTO files_fts(files_fts) VALUES('rebuild')", [])?;
        debug!("✅ Files FTS5 index rebuilt successfully");
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

        // CASCADE: Create FTS5 table and triggers for symbols
        self.create_symbols_fts_table()?;
        self.create_symbols_fts_triggers()?;

        Ok(())
    }

    /// CASCADE: Create FTS5 virtual table for full-text search on symbols
    /// Indexes name, signature, doc_comment, and code_context for fast relevance-ranked search
    pub(crate) fn create_symbols_fts_table(&self) -> Result<()> {
        self.conn.execute(
            r#"CREATE VIRTUAL TABLE IF NOT EXISTS symbols_fts USING fts5(
                name,
                signature,
                doc_comment,
                code_context,
                tokenize = "unicode61 separators '_::->.'",
                content='symbols',
                content_rowid='rowid'
            )"#,
            [],
        )?;
        debug!("Created symbols_fts virtual table with unicode61 tokenizer (separators: _::->.)");
        Ok(())
    }

    /// CASCADE: Create triggers to keep symbols_fts in sync with symbols table
    pub(crate) fn create_symbols_fts_triggers(&self) -> Result<()> {
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

    /// CASCADE: Disable symbols FTS5 triggers for bulk operations
    /// This prevents row-by-row FTS updates during bulk inserts
    #[allow(dead_code)]
    pub(crate) fn disable_symbols_fts_triggers(&self) -> Result<()> {
        self.conn.execute("DROP TRIGGER IF EXISTS symbols_ai", [])?;
        self.conn.execute("DROP TRIGGER IF EXISTS symbols_ad", [])?;
        self.conn.execute("DROP TRIGGER IF EXISTS symbols_au", [])?;
        debug!("Disabled symbols FTS5 triggers for bulk operation");
        Ok(())
    }

    /// CASCADE: Re-enable symbols FTS5 triggers after bulk operations
    #[allow(dead_code)]
    pub(crate) fn enable_symbols_fts_triggers(&self) -> Result<()> {
        self.create_symbols_fts_triggers()?;
        debug!("Re-enabled symbols FTS5 triggers");
        Ok(())
    }

    /// CASCADE: Rebuild symbols FTS5 index atomically
    /// Use after bulk operations with disabled triggers
    pub(crate) fn rebuild_symbols_fts(&self) -> Result<()> {
        debug!("Rebuilding symbols FTS5 index...");
        // First, delete all existing FTS content
        self.conn.execute(
            "INSERT INTO symbols_fts(symbols_fts) VALUES('delete-all')",
            [],
        )?;
        // Then rebuild from base table
        self.conn
            .execute("INSERT INTO symbols_fts(symbols_fts) VALUES('rebuild')", [])?;
        debug!("✅ Symbols FTS5 index rebuilt successfully");
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

    /// Create the embeddings table for vector mapping
    pub(crate) fn create_embeddings_table(&self) -> Result<()> {
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

    /// Create the memories view for querying memory files
    ///
    /// This view extracts memory data from JSON files stored in `.memories/`
    /// by querying the files table and using SQLite's json_extract() function.
    ///
    /// The view filters to only include JSON files in the `.memories/` directory,
    /// excluding mutable plans (which live in `.memories/plans/` and will be added in Phase 2).
    pub(crate) fn create_memories_view(&self) -> Result<()> {
        // Create view that extracts JSON fields from memory files
        self.conn.execute(
            r#"CREATE VIEW IF NOT EXISTS memories AS
            SELECT
                f.path,
                f.hash,
                f.last_modified,
                json_extract(f.content, '$.id') as id,
                json_extract(f.content, '$.timestamp') as timestamp,
                json_extract(f.content, '$.type') as type,
                json_extract(f.content, '$.description') as description,
                json_extract(f.content, '$.tags') as tags,
                json_extract(f.content, '$.git.branch') as git_branch,
                json_extract(f.content, '$.git.commit') as git_commit,
                json_extract(f.content, '$.git.dirty') as git_dirty
            FROM files f
            WHERE f.path LIKE '.memories/%'
              AND f.path LIKE '%.json'
              AND f.path NOT LIKE '.memories/plans/%'"#,
            [],
        )?;

        // Create index on timestamp for fast chronological queries
        self.conn.execute(
            r#"CREATE INDEX IF NOT EXISTS idx_memories_timestamp
            ON files(json_extract(content, '$.timestamp'))
            WHERE path LIKE '.memories/%'
              AND path LIKE '%.json'"#,
            [],
        )?;

        // Create index on type for fast filtering by memory type
        self.conn.execute(
            r#"CREATE INDEX IF NOT EXISTS idx_memories_type
            ON files(json_extract(content, '$.type'))
            WHERE path LIKE '.memories/%'
              AND path LIKE '%.json'"#,
            [],
        )?;

        debug!("Created memories view and indexes");
        Ok(())
    }

    /// Create the plans view for mutable development plans (Phase 1.5)
    ///
    /// This view provides structured access to plan JSON files stored in `.memories/plans/`.
    /// Plans are mutable (can be updated in-place) unlike immutable checkpoint memories.
    ///
    /// The view extracts plan-specific fields including status, title, and content,
    /// making plans queryable via SQL for recall() and plan management operations.
    pub(crate) fn create_plans_view(&self) -> Result<()> {
        // Create view that extracts JSON fields from plan files
        self.conn.execute(
            r#"CREATE VIEW IF NOT EXISTS plans AS
            SELECT
                f.path,
                f.hash,
                f.last_modified,
                json_extract(f.content, '$.id') as id,
                json_extract(f.content, '$.timestamp') as timestamp,
                json_extract(f.content, '$.type') as type,
                json_extract(f.content, '$.title') as title,
                json_extract(f.content, '$.status') as status,
                json_extract(f.content, '$.content') as content,
                json_extract(f.content, '$.git.branch') as git_branch,
                json_extract(f.content, '$.git.commit') as git_commit,
                json_extract(f.content, '$.git.dirty') as git_dirty
            FROM files f
            WHERE f.path LIKE '.memories/plans/%'
              AND f.path LIKE '%.json'"#,
            [],
        )?;

        // Create index on timestamp for fast chronological queries
        self.conn.execute(
            r#"CREATE INDEX IF NOT EXISTS idx_plans_timestamp
            ON files(json_extract(content, '$.timestamp'))
            WHERE path LIKE '.memories/plans/%'
              AND path LIKE '%.json'"#,
            [],
        )?;

        // Create index on status for fast filtering (active/completed/archived)
        self.conn.execute(
            r#"CREATE INDEX IF NOT EXISTS idx_plans_status
            ON files(json_extract(content, '$.status'))
            WHERE path LIKE '.memories/plans/%'
              AND path LIKE '%.json'"#,
            [],
        )?;

        debug!("Created plans view and indexes");
        Ok(())
    }
}
