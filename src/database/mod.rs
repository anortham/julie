//! Julie's Database Module - SQLite Source of Truth
//!
//! Refactored into focused modules for maintainability (<500 lines each)

use anyhow::{anyhow, Result};
use rusqlite::{Connection, Row};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use crate::extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};

// Module declarations
mod bulk_operations;
mod embeddings;
mod files;
mod helpers;
mod migrations;
mod relationships;
mod schema;
mod symbols;
pub mod types;
mod workspace;

// Re-export public types
pub use files::{calculate_file_hash, create_file_info};
pub use migrations::LATEST_SCHEMA_VERSION;
pub use types::*;

/// The main database connection and operations
pub struct SymbolDatabase {
    pub(crate) conn: Connection,
    pub(crate) file_path: PathBuf,
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

        // Configure WAL autocheckpoint to prevent large WAL files
        // Default is 1000 pages (~4MB), we set to 2000 pages (~8MB) for better performance
        // This prevents WAL from growing to 20MB+ which causes "database malformed" errors
        conn.pragma_update(None, "wal_autocheckpoint", 2000)?;

        let mut db = Self { conn, file_path };

        // Run schema migrations BEFORE initializing schema
        db.run_migrations()?;

        db.initialize_schema()?;

        // 🔥 CRITICAL: FTS5 Integrity Check - detect and rebuild corrupted FTS5 indexes
        // This prevents "missing row" and "invalid fts5 file format" errors
        db.check_and_rebuild_fts5_indexes()?;

        info!("Database initialized successfully");
        Ok(db)
    }

    /// Check FTS5 indexes for corruption and rebuild if needed
    /// Detects "invalid fts5 file format" and "missing row" errors
    fn check_and_rebuild_fts5_indexes(&mut self) -> Result<()> {
        // Check symbols_fts integrity
        let symbols_corrupted = self.conn.query_row(
            "SELECT COUNT(*) FROM symbols_fts WHERE symbols_fts MATCH 'test'",
            [],
            |_| Ok(()),
        ).is_err();

        if symbols_corrupted {
            warn!("⚠️  Detected corrupted symbols_fts index - rebuilding...");

            // Try standard rebuild first
            if self.rebuild_symbols_fts().is_err() {
                // Severe corruption - force drop and recreate FTS5 table
                warn!("Standard rebuild failed - performing full FTS5 table recreation");

                // Enable schema modification to manually remove corrupted FTS5 table
                self.conn.execute("PRAGMA writable_schema=ON", [])?;
                // Delete all entries for symbols_fts from sqlite_master
                self.conn.execute(
                    "DELETE FROM sqlite_master WHERE name LIKE 'symbols_fts%'",
                    [],
                )?;
                self.conn.execute("PRAGMA writable_schema=OFF", [])?;
                // Force schema reload with VACUUM
                self.conn.execute("VACUUM", [])?;

                self.create_symbols_fts_table()?;
                self.create_symbols_fts_triggers()?;
                // Rebuild from base table (now that table exists)
                self.conn.execute("INSERT INTO symbols_fts(symbols_fts) VALUES('rebuild')", [])?;
            }

            info!("✅ symbols_fts index rebuilt successfully");
        }

        // Check files_fts integrity
        let files_corrupted = self.conn.query_row(
            "SELECT COUNT(*) FROM files_fts WHERE files_fts MATCH 'test'",
            [],
            |_| Ok(()),
        ).is_err();

        if files_corrupted {
            warn!("⚠️  Detected corrupted files_fts index - rebuilding...");

            // Try standard rebuild first
            if self.rebuild_files_fts().is_err() {
                // Severe corruption - force drop and recreate FTS5 table
                warn!("Standard rebuild failed - performing full FTS5 table recreation");

                // Enable schema modification to manually remove corrupted FTS5 table
                self.conn.execute("PRAGMA writable_schema=ON", [])?;
                // Delete all entries for files_fts from sqlite_master
                self.conn.execute(
                    "DELETE FROM sqlite_master WHERE name LIKE 'files_fts%'",
                    [],
                )?;
                self.conn.execute("PRAGMA writable_schema=OFF", [])?;
                // Force schema reload with VACUUM
                self.conn.execute("VACUUM", [])?;

                self.create_files_fts_table()?;
                self.create_files_fts_triggers()?;
                // Rebuild from base table (now that table exists)
                self.conn.execute("INSERT INTO files_fts(files_fts) VALUES('rebuild')", [])?;
            }

            info!("✅ files_fts index rebuilt successfully");
        }

        Ok(())
    }

    /// Checkpoint the WAL (Write-Ahead Log) to merge changes into main database
    ///
    /// This executes `PRAGMA wal_checkpoint(TRUNCATE)` which:
    /// - Merges all WAL frames into the main database file
    /// - Truncates the WAL file to 0 bytes after checkpoint
    ///
    /// Returns: (busy, log, checkpointed) tuple
    /// - busy: Number of frames that couldn't be checkpointed (should be 0)
    /// - log: Total frames in WAL before checkpoint
    /// - checkpointed: Frames successfully checkpointed
    ///
    /// This should be called periodically or on shutdown to prevent unbounded WAL growth
    pub fn checkpoint_wal(&mut self) -> Result<(i32, i32, i32)> {
        debug!("Checkpointing WAL to prevent unbounded growth");

        // Execute PRAGMA wal_checkpoint(TRUNCATE)
        // Returns: (busy, log, checkpointed)
        let mut stmt = self.conn.prepare("PRAGMA wal_checkpoint(TRUNCATE)")?;
        let result = stmt.query_row([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;

        let (busy, log, checkpointed) = result;

        debug!(
            "WAL checkpoint complete: busy={}, log={}, checkpointed={}",
            busy, log, checkpointed
        );

        Ok((busy, log, checkpointed))
    }
}
