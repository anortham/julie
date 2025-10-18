//! Julie's Database Module - SQLite Source of Truth
//!
//! Refactored into focused modules for maintainability (<500 lines each)

use anyhow::{anyhow, Result};
use rusqlite::{Connection, Row};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

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

        info!("Database initialized successfully");
        Ok(db)
    }
}
