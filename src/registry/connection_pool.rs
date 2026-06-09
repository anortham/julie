//! Per-workspace SQLite connection pool — implementation lives in `julie-core`.
//!
//! All types are re-exported here so every `crate::registry::connection_pool::*`
//! import path in the codebase remains valid without any changes.
pub use julie_core::connection_pool::{PoolStats, PooledConn, WorkspaceConnectionPool};
