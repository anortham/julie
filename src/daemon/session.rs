//! Simple session tracking for idle detection.
//!
//! Tracks active IPC sessions so the daemon can detect when it has been
//! idle (zero sessions) for graceful shutdown or resource reclamation.

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use tokio::sync::Notify;

/// Tracks active IPC sessions connected to the daemon.
///
/// Thread-safe via `RwLock`. Each session gets a UUID on connect;
/// the UUID is removed when the session ends (normally or on error).
/// A `Notify` wakes any `drain_sessions` waiter whenever the count drops.
pub struct SessionTracker {
    sessions: RwLock<HashSet<String>>,
    notify: Arc<Notify>,
}

impl SessionTracker {
    /// Create an empty session tracker.
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashSet::new()),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Register a new session. Returns the generated session ID (UUID v4).
    pub fn add_session(&self) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let mut sessions = self.sessions.write().unwrap_or_else(|p| p.into_inner());
        sessions.insert(id.clone());
        id
    }

    /// Remove a session by ID. No-op if the ID doesn't exist.
    /// Notifies any `drain_sessions` waiter so it can re-check the count.
    pub fn remove_session(&self, id: &str) {
        let mut sessions = self.sessions.write().unwrap_or_else(|p| p.into_inner());
        sessions.remove(id);
        drop(sessions); // release lock before notifying
        self.notify.notify_one();
    }

    /// Number of currently active sessions.
    pub fn active_count(&self) -> usize {
        let sessions = self.sessions.read().unwrap_or_else(|p| p.into_inner());
        sessions.len()
    }

    /// Returns true when no sessions are connected.
    pub fn is_idle(&self) -> bool {
        self.active_count() == 0
    }

    /// Access the notify handle for `drain_sessions`.
    pub(crate) fn session_notify(&self) -> &Arc<Notify> {
        &self.notify
    }
}
