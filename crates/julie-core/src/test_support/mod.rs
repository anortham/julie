//! Handler-free test helpers for the Julie workspace.
//!
//! These helpers live in `julie-core` itself (behind the `test-support` feature or
//! `cfg(test)`) so that julie-core's own test binary can use them without a
//! dev-dependency cycle.  See ADR-0006 for the full rationale.
//!
//! `julie-test-support` is a thin re-export of this module for downstream consumers.

pub mod cleanup;
pub mod db;
pub mod tempdir;

// Flat re-exports so callers can `use crate::test_support::open_test_connection` etc.
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
