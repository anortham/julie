//! Shared state for the dashboard HTTP server and SSE broadcast channel.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::broadcast;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::session::SessionTracker;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::dashboard::error_buffer::{ErrorBuffer, LogEntry};

// ---------------------------------------------------------------------------
// DashboardEvent
// ---------------------------------------------------------------------------

/// Events broadcast over the SSE channel to connected dashboard clients.
#[derive(Debug, Clone)]
pub enum DashboardEvent {
    ToolCall {
        tool_name: String,
        workspace: String,
        duration_ms: f64,
    },
    SessionChange {
        active_count: usize,
    },
    LogEntry(LogEntry),
}

// ---------------------------------------------------------------------------
// DashboardState
// ---------------------------------------------------------------------------

/// Shared state injected into every dashboard route handler.
///
/// Cheap to clone — all fields are either `Arc`-wrapped or `Copy`.
#[derive(Clone)]
pub struct DashboardState {
    sessions: Arc<SessionTracker>,
    daemon_db: Option<Arc<DaemonDatabase>>,
    restart_pending: Arc<AtomicBool>,
    start_time: Instant,
    error_buffer: ErrorBuffer,
    embedding_available: bool,
    workspace_pool: Option<Arc<WorkspacePool>>,
    tx: broadcast::Sender<DashboardEvent>,
}

impl DashboardState {
    /// Create a new `DashboardState`.
    ///
    /// Internally creates an `ErrorBuffer` with the given capacity and a
    /// broadcast channel with capacity 256.
    pub fn new(
        sessions: Arc<SessionTracker>,
        daemon_db: Option<Arc<DaemonDatabase>>,
        restart_pending: Arc<AtomicBool>,
        start_time: Instant,
        embedding_available: bool,
        workspace_pool: Option<Arc<WorkspacePool>>,
        error_buffer_capacity: usize,
    ) -> Self {
        let error_buffer = ErrorBuffer::new(error_buffer_capacity);
        let (tx, _rx) = broadcast::channel(256);
        Self {
            sessions,
            daemon_db,
            restart_pending,
            start_time,
            error_buffer,
            embedding_available,
            workspace_pool,
            tx,
        }
    }

    /// Reference to the session tracker.
    pub fn sessions(&self) -> &SessionTracker {
        &self.sessions
    }

    /// Reference to the daemon database, if available.
    pub fn daemon_db(&self) -> Option<&Arc<DaemonDatabase>> {
        self.daemon_db.as_ref()
    }

    /// Whether a daemon restart is pending.
    pub fn is_restart_pending(&self) -> bool {
        self.restart_pending.load(Ordering::Relaxed)
    }

    /// Time elapsed since the daemon started.
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Reference to the error ring buffer.
    pub fn error_buffer(&self) -> &ErrorBuffer {
        &self.error_buffer
    }

    /// Snapshot of recent error/warn log entries, oldest first.
    pub fn error_entries(&self) -> Vec<LogEntry> {
        self.error_buffer.recent_entries()
    }

    /// Whether an embedding provider is available.
    pub fn embedding_available(&self) -> bool {
        self.embedding_available
    }

    /// Reference to the workspace pool, if available.
    pub fn workspace_pool(&self) -> Option<&Arc<WorkspacePool>> {
        self.workspace_pool.as_ref()
    }

    /// Subscribe to the broadcast channel. Each call returns an independent receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<DashboardEvent> {
        self.tx.subscribe()
    }

    /// Send an event to all current subscribers. Ignores send errors (no subscribers is fine).
    pub fn send_event(&self, event: DashboardEvent) {
        let _ = self.tx.send(event);
    }

    /// Clone the broadcast sender (for use in middleware or background tasks).
    pub fn sender(&self) -> broadcast::Sender<DashboardEvent> {
        self.tx.clone()
    }
}
