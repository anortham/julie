//! Handler-free test helpers for the Julie workspace.
//!
//! This crate holds ONLY the low-stack helpers that the database test slice
//! needs. It depends on `julie-core` (for `SymbolDatabase`/`FileInfo`) and
//! `julie-extractors`, but NOT on the top-level `julie` crate — ensuring the
//! leaf can be used from `julie-core`'s own test suite without a dep cycle.
//!
//! Included:
//! - DB row builders (`file_info_builder`, `symbol_builder`, `identifier_builder`,
//!   `relationship_builder`, `set_symbol_reference_scores`, `store_file_info_if_missing`)
//! - `open_test_connection` — a rusqlite connection pre-configured for tests
//! - `unique_temp_dir` — counter-based unique temp directory for parallel tests
//! - `atomic_cleanup_julie_dir` — safe `.julie` directory removal with retry logic
//!
//! NOT included (stay in the top `julie` crate, they import handler/tools):
//! - `helpers/workspace.rs` (`JulieServerHandler`, `DaemonDatabase`, `WorkspacePool`)
//! - `fixtures/julie_db.rs` `JulieTestFixture` (`ManageWorkspaceTool`)
//! - `helpers/mcp.rs` (rmcp)

pub mod db;
pub mod tempdir;
pub mod cleanup;

// Flat re-exports so callers can `use julie_test_support::open_test_connection` etc.
pub use cleanup::atomic_cleanup_julie_dir;
pub use db::rows::{
    file_info_builder, identifier_builder, relationship_builder, set_symbol_reference_scores,
    store_file_info_if_missing, symbol_builder, FileInfoBuilder, IdentifierBuilder,
    RelationshipBuilder, SymbolBuilder,
};
pub use open_test_connection::open_test_connection;
pub use tempdir::unique_temp_dir;

mod open_test_connection {
    use anyhow::Result;
    use std::path::Path;

    /// Open a rusqlite connection configured for concurrent test access.
    ///
    /// Sets a 5-second busy timeout and configures WAL autocheckpoint (2000 pages)
    /// to prevent "database malformed" errors from WAL corruption during parallel tests.
    pub fn open_test_connection<P: AsRef<Path>>(db_path: P) -> Result<rusqlite::Connection> {
        use rusqlite::Connection;

        let conn = Connection::open(db_path.as_ref())?;

        // Set busy timeout - wait up to 5 seconds for locks
        // This prevents immediate failures when another connection holds a lock
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        // Configure WAL autocheckpoint to prevent large WAL files
        // This prevents "database malformed" errors from WAL corruption
        conn.pragma_update(None, "wal_autocheckpoint", 2000)?;

        Ok(conn)
    }
}
