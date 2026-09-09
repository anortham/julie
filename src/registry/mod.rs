//! Julie registry: background-process state and recovery-marker reads.

pub mod database;

pub mod connection_pool;
pub mod embedding_service;
pub mod lifecycle;
pub mod project_log;
pub mod shutdown;
pub mod workspace_registry_store;

pub use self::connection_pool::{PooledConn, WorkspaceConnectionPool};
