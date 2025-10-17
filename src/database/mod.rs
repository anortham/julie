//! Julie's Database Module - SQLite Source of Truth
//!
//! Refactored into focused modules for maintainability (<500 lines each)

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use crate::extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};

// Module declarations
pub mod types;
mod migrations;
mod schema;
mod files;
mod symbols;
mod relationships;
mod bulk_operations;
mod workspace;
mod helpers;
mod embeddings;

// Re-export public types
pub use types::*;
pub use files::{calculate_file_hash, create_file_info};
pub use migrations::LATEST_SCHEMA_VERSION;

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

        let mut db = Self { conn, file_path };

        // Run schema migrations BEFORE initializing schema
        db.run_migrations()?;

        db.initialize_schema()?;

        info!("Database initialized successfully");
        Ok(db)
    }
}
