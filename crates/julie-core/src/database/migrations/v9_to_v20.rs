use crate::database::SymbolDatabase;
use anyhow::{Result, anyhow};
use tracing::{debug, info, warn};

impl SymbolDatabase {
    /// Migration 009: Add reference_score column for graph centrality ranking.
    ///
    /// Stores pre-computed weighted incoming reference count per symbol.
    /// Used by search scoring to boost well-connected symbols.
    pub(super) fn migration_009_add_reference_score(&self) -> Result<()> {
        info!("Running migration 009: Add reference_score column");

        // Check if symbols table exists
        let table_exists: bool = self.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='symbols'",
            [],
            |row| row.get::<_, i32>(0).map(|c| c > 0),
        )?;

        if !table_exists {
            debug!("Symbols table doesn't exist yet (fresh database), skipping migration");
            return Ok(());
        }

        // Check if column already exists (idempotency)
        if self.has_column("symbols", "reference_score")? {
            warn!("reference_score column already exists in symbols table, skipping migration");
            return Ok(());
        }

        self.conn.execute(
            "ALTER TABLE symbols ADD COLUMN reference_score REAL NOT NULL DEFAULT 0.0",
            [],
        )?;
        info!("✅ Added reference_score column to symbols table");
        Ok(())
    }

    /// Migration 010: Add symbol_vectors virtual table for semantic embeddings.
    ///
    /// Uses sqlite-vec's `vec0` module for KNN vector search.
    /// Stores 384-dimensional float embeddings keyed by symbol_id.
    /// Requires sqlite-vec to be registered via `register_sqlite_vec()`.
    pub(super) fn migration_010_add_symbol_vectors(&self) -> Result<()> {
        info!("Running migration 010: Add symbol_vectors virtual table");

        // Check if table already exists (idempotency)
        let table_exists: bool = self.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='symbol_vectors'",
            [],
            |row| row.get::<_, i32>(0).map(|c| c > 0),
        )?;

        if table_exists {
            debug!("symbol_vectors table already exists, skipping migration 010");
            return Ok(());
        }

        self.conn.execute(
            "CREATE VIRTUAL TABLE symbol_vectors USING vec0(
                symbol_id TEXT PRIMARY KEY,
                embedding float[384]
            )",
            [],
        )?;

        info!("✅ symbol_vectors virtual table created (384-dim float vectors)");
        Ok(())
    }

    /// Migration 011: Add embedding_config table for dynamic embedding dimensions.
    ///
    /// Stores the active model name and dimensionality so Julie can detect
    /// model swaps and recreate vector tables with the correct dimensions.
    pub(super) fn migration_011_add_embedding_config(&self) -> Result<()> {
        info!("Running migration 011: Add embedding_config table");

        let table_exists: bool = self.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='embedding_config'",
            [],
            |row| row.get::<_, i32>(0).map(|c| c > 0),
        )?;

        if table_exists {
            debug!("embedding_config table already exists, skipping migration 011");
            return Ok(());
        }

        self.conn.execute(
            "CREATE TABLE embedding_config (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                model_name TEXT NOT NULL,
                dimensions INTEGER NOT NULL
            )",
            [],
        )?;

        // Seed with current defaults (BGE-small-en-v1.5, 384 dims)
        self.conn.execute(
            "INSERT INTO embedding_config (id, model_name, dimensions) VALUES (1, 'bge-small-en-v1.5', 384)",
            [],
        )?;

        info!("✅ embedding_config table created with defaults (bge-small-en-v1.5, 384-dim)");
        Ok(())
    }

    pub(super) fn migration_012_add_memory_vectors(&self) -> Result<()> {
        info!("Running migration 012: Add memory_vectors virtual table");

        // Check if table already exists (idempotency)
        let table_exists: bool = self.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='memory_vectors'",
            [],
            |row| row.get::<_, i32>(0).map(|c| c > 0),
        )?;

        if table_exists {
            debug!("memory_vectors table already exists, skipping migration 012");
            return Ok(());
        }

        // Read dimensions from embedding_config (set by migration 011).
        // Fall back to 384 if config doesn't exist yet (shouldn't happen in normal flow).
        let dims_i64 = match self.conn.query_row(
            "SELECT dimensions FROM embedding_config WHERE id = 1",
            [],
            |row| row.get::<_, i64>(0),
        ) {
            Ok(dims) => dims,
            Err(rusqlite::Error::QueryReturnedNoRows) => 384,
            Err(err) => {
                return Err(anyhow!(
                    "Failed to read embedding dimensions from embedding_config: {err}"
                ));
            }
        };

        let dims = usize::try_from(dims_i64).map_err(|_| {
            anyhow!(
                "Invalid embedding dimensions in embedding_config: {dims_i64} (expected non-negative integer fitting usize)"
            )
        })?;

        self.conn.execute(
            &format!(
                "CREATE VIRTUAL TABLE memory_vectors USING vec0(
                    checkpoint_id TEXT PRIMARY KEY,
                    embedding float[{dims}]
                )"
            ),
            [],
        )?;

        info!("✅ memory_vectors virtual table created ({dims}-dim float vectors)");
        Ok(())
    }

    /// Migration 013: Add tool_calls table for operational metrics and line_count to files.
    pub(super) fn migration_013_add_tool_calls_and_line_count(&self) -> Result<()> {
        info!("Migration 013: Adding tool_calls table and line_count column");

        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tool_calls (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                tool_name TEXT NOT NULL,
                duration_ms REAL NOT NULL,
                result_count INTEGER,
                source_bytes INTEGER,
                input_bytes INTEGER,
                output_bytes INTEGER,
                success INTEGER NOT NULL DEFAULT 1,
                metadata TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_tool_calls_timestamp ON tool_calls(timestamp);
            CREATE INDEX IF NOT EXISTS idx_tool_calls_tool_name ON tool_calls(tool_name);
            CREATE INDEX IF NOT EXISTS idx_tool_calls_session ON tool_calls(session_id);
            ",
        )?;

        // Only ALTER existing files table; fresh databases get line_count from CREATE TABLE
        let files_exists: bool = self.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='files'",
            [],
            |row| row.get::<_, i32>(0).map(|c| c > 0),
        )?;

        if files_exists && !self.has_column("files", "line_count")? {
            self.conn.execute(
                "ALTER TABLE files ADD COLUMN line_count INTEGER DEFAULT 0",
                [],
            )?;
        }

        info!("Migration 013 complete: tool_calls table and line_count column added");
        Ok(())
    }

    pub(super) fn migration_014_add_embedding_format_version(&self) -> Result<()> {
        info!("Running migration 014: Add format_version to embedding_config");

        if self.has_column("embedding_config", "format_version")? {
            debug!("embedding_config.format_version already exists, skipping migration 014");
            return Ok(());
        }

        self.conn.execute(
            "ALTER TABLE embedding_config ADD COLUMN format_version INTEGER NOT NULL DEFAULT 1",
            [],
        )?;

        info!("Migration 014 complete: format_version column added to embedding_config");
        Ok(())
    }

    pub(super) fn migration_015_add_indexing_repairs(&self) -> Result<()> {
        info!("Running migration 015: Add indexing_repairs table");

        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS indexing_repairs (
                path TEXT PRIMARY KEY,
                reason TEXT NOT NULL,
                detail TEXT,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_indexing_repairs_reason
            ON indexing_repairs(reason);",
        )?;

        info!("Migration 015 complete: indexing_repairs table added");
        Ok(())
    }

    pub(super) fn migration_016_add_canonical_revisions(&self) -> Result<()> {
        info!("Running migration 016: Add canonical_revisions table");

        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS canonical_revisions (
                revision INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace_id TEXT NOT NULL,
                kind TEXT NOT NULL CHECK(kind IN ('fresh', 'incremental')),
                cleaned_file_count INTEGER NOT NULL DEFAULT 0,
                file_count INTEGER NOT NULL DEFAULT 0,
                symbol_count INTEGER NOT NULL DEFAULT 0,
                relationship_count INTEGER NOT NULL DEFAULT 0,
                identifier_count INTEGER NOT NULL DEFAULT 0,
                type_count INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_canonical_revisions_workspace_revision
            ON canonical_revisions(workspace_id, revision DESC);",
        )?;

        info!("Migration 016 complete: canonical_revisions table added");
        Ok(())
    }

    pub(super) fn migration_017_add_projection_states(&self) -> Result<()> {
        info!("Running migration 017: Add projection_states table");

        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS projection_states (
                projection TEXT NOT NULL,
                workspace_id TEXT NOT NULL,
                status TEXT NOT NULL CHECK(status IN ('missing', 'building', 'ready', 'stale')),
                canonical_revision INTEGER,
                projected_revision INTEGER,
                detail TEXT,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (projection, workspace_id)
            );
            CREATE INDEX IF NOT EXISTS idx_projection_states_workspace
            ON projection_states(workspace_id);",
        )?;

        info!("Migration 017 complete: projection_states table added");
        Ok(())
    }

    pub(super) fn migration_018_add_projected_revision_to_projection_states(&self) -> Result<()> {
        info!("Running migration 018: Add projected_revision to projection_states");

        if !self.has_column("projection_states", "projected_revision")? {
            self.conn.execute(
                "ALTER TABLE projection_states ADD COLUMN projected_revision INTEGER",
                [],
            )?;
        }

        self.conn.execute(
            "UPDATE projection_states
             SET projected_revision = canonical_revision
             WHERE status = 'ready' AND projected_revision IS NULL",
            [],
        )?;

        info!("Migration 018 complete: projected_revision backfilled for ready states");
        Ok(())
    }

    pub(super) fn migration_019_add_revision_file_changes(&self) -> Result<()> {
        info!("Running migration 019: Add revision_file_changes table");

        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS revision_file_changes (
                revision INTEGER NOT NULL,
                workspace_id TEXT NOT NULL,
                file_path TEXT NOT NULL,
                change_kind TEXT NOT NULL CHECK(change_kind IN ('added', 'modified', 'deleted')),
                old_hash TEXT,
                new_hash TEXT,
                PRIMARY KEY (revision, workspace_id, file_path)
            );
            CREATE INDEX IF NOT EXISTS idx_revision_file_changes_workspace_revision
            ON revision_file_changes(workspace_id, revision);
            CREATE INDEX IF NOT EXISTS idx_revision_file_changes_workspace_path
            ON revision_file_changes(workspace_id, file_path);",
        )?;

        info!("Migration 019 complete: revision_file_changes table added");
        Ok(())
    }

    pub(super) fn migration_020_add_symbol_annotations(&self) -> Result<()> {
        info!("Running migration 020: Add symbol_annotations table");

        self.create_symbol_annotations_table()?;

        info!("Migration 020 complete: symbol_annotations table added");
        Ok(())
    }
}
