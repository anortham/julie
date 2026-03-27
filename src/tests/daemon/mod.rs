pub mod database;
pub mod embedding_service;
pub mod handler;
pub mod ipc;
pub mod lifecycle;
pub mod paths;
pub mod pid;
pub mod server;
pub mod session;
#[cfg(windows)]
pub mod shutdown_event;
pub mod watcher_pool;
pub mod workspace_pool;
