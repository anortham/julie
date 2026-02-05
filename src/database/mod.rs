//! Julie's Database Module - SQLite Source of Truth
//!
//! Refactored into focused modules for maintainability (<500 lines each)

use anyhow::{Result, anyhow};
use rusqlite::{Connection, Row};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use crate::extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};

// Module declarations
mod bulk_operations;
mod files;
mod helpers;
mod migrations;
mod relationships;
mod schema;
mod symbols;
pub mod types;
mod type_queries;
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

        // 🚨 CRITICAL: Set WAL mode IMMEDIATELY after connection open
        // This MUST happen before ANY other database operations (including migrations)
        // to prevent corruption when multiple processes access the same database.
        // WAL mode allows concurrent readers + single writer without corruption.
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))
            .map_err(|e| anyhow!("Failed to enable WAL mode: {}", e))?;

        // Verify WAL mode was actually set (some filesystems don't support WAL)
        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .map_err(|e| anyhow!("Failed to query journal mode: {}", e))?;

        if !journal_mode.eq_ignore_ascii_case("wal") {
            return Err(anyhow!(
                "Failed to enable WAL mode (got '{}'). This filesystem may not support WAL. \
                 Use a filesystem that supports WAL (NTFS, ext4, APFS, etc.)",
                journal_mode
            ));
        }

        debug!("✅ WAL mode enabled on database connection");

        // Set busy timeout for concurrent access - wait up to 5 seconds for locks
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;

        // Set synchronous mode to NORMAL (safe with WAL, faster than FULL)
        // FULL: fsync after every transaction (slow, overkill with WAL)
        // NORMAL: fsync at WAL checkpoints (safe with WAL, 2-3x faster)
        // OFF: no fsync (fast, data loss on power failure)
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        // Configure WAL autocheckpoint to prevent large WAL files
        // Default is 1000 pages (~4MB), we set to 2000 pages (~8MB) for better performance
        // This prevents WAL from growing to 20MB+ which causes "database malformed" errors
        conn.pragma_update(None, "wal_autocheckpoint", 2000)?;

        let mut db = Self { conn, file_path };

        // 🔥 DEVELOPMENT MODE SAFETY: Detect schema version mismatches during development
        // When building a new version with schema changes while old MCP is running,
        // we can hit corruption. In dev mode, warn and optionally rebuild.
        let current_schema = db.get_schema_version().unwrap_or(0);
        let target_schema = crate::database::LATEST_SCHEMA_VERSION;

        if current_schema > target_schema {
            // Downgrade scenario - old database with newer schema
            return Err(anyhow!(
                "Database schema version ({}) is NEWER than code expects ({}). \
                 This means you're running old Julie code against a database created by newer Julie. \
                 Solutions:\n\
                 1. Build and run the latest Julie version (recommended)\n\
                 2. Delete .julie/indexes/ directory to rebuild with current schema\n\
                 3. Checkout the newer Julie version that created this database",
                current_schema,
                target_schema
            ));
        }

        // Run schema migrations AFTER WAL mode is configured
        db.run_migrations()?;

        db.initialize_schema()?;

        info!("Database initialized successfully");
        Ok(db)
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
        let result = stmt.query_row([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;

        let (busy, log, checkpointed) = result;

        debug!(
            "WAL checkpoint complete: busy={}, log={}, checkpointed={}",
            busy, log, checkpointed
        );

        Ok((busy, log, checkpointed))
    }

    /// Checkpoint the WAL using RESTART mode (waits for readers, more aggressive than PASSIVE)
    ///
    /// This executes `PRAGMA wal_checkpoint(RESTART)` which:
    /// - Blocks until all current readers finish
    /// - Merges all WAL frames into the main database file
    /// - Resets the WAL file after checkpoint
    ///
    /// RESTART mode is more aggressive than PASSIVE (which can fail during heavy writes)
    /// but less aggressive than TRUNCATE (which truncates to 0 bytes).
    ///
    /// Returns: (busy, log, checkpointed) tuple
    /// - busy: Number of frames that couldn't be checkpointed (should be 0)
    /// - log: Total frames in WAL before checkpoint
    /// - checkpointed: Frames successfully checkpointed
    ///
    /// Use this after bulk operations to prevent WAL from growing to 45MB+
    pub fn checkpoint_wal_restart(&mut self) -> Result<(i32, i32, i32)> {
        debug!("Checkpointing WAL (RESTART mode - waits for readers)");

        // Execute PRAGMA wal_checkpoint(RESTART)
        // Returns: (busy, log, checkpointed)
        let mut stmt = self.conn.prepare("PRAGMA wal_checkpoint(RESTART)")?;
        let result = stmt.query_row([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;

        let (busy, log, checkpointed) = result;

        debug!(
            "WAL checkpoint (RESTART) complete: busy={}, log={}, checkpointed={}",
            busy, log, checkpointed
        );

        Ok((busy, log, checkpointed))
    }
}

// 🚨 CRITICAL: Implement Drop to checkpoint WAL on database close
// This prevents corruption when process terminates while WAL has uncommitted changes
impl Drop for SymbolDatabase {
    fn drop(&mut self) {
        // Best-effort checkpoint - log error but don't panic
        // We can't return Result from Drop, so just log failures
        if let Err(e) = self.checkpoint_wal() {
            warn!(
                "Failed to checkpoint WAL on database close (non-fatal): {}",
                e
            );
        } else {
            debug!("✅ WAL checkpointed successfully on database close");
        }
    }
}
