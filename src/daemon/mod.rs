//! Julie daemon: background-process state, session tracking, and recovery-marker reads.

pub mod database;

pub mod connection_pool;
pub mod discovery;
pub mod embedding_service;
pub mod lifecycle;
pub mod project_log;
pub mod session;
pub mod shutdown;
pub mod workspace_registry_store;
pub mod workspace_session_attachment;

pub use self::connection_pool::{PooledConn, WorkspaceConnectionPool};
