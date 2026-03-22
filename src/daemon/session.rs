//! Simple session tracking for idle detection.
//!
//! Tracks active IPC sessions so the daemon can detect when it has been
//! idle (zero sessions) for graceful shutdown or resource reclamation.

use std::collections::HashSet;
use std::sync::RwLock;

/// Tracks active IPC sessions connected to the daemon.
///
/// Thread-safe via `RwLock`. Each session gets a UUID on connect;
/// the UUID is removed when the session ends (normally or on error).
pub struct SessionTracker {
    sessions: RwLock<HashSet<String>>,
}

impl SessionTracker {
    /// Create an empty session tracker.
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashSet::new()),
        }
    }

    /// Register a new session. Returns the generated session ID (UUID v4).
    pub fn add_session(&self) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let mut sessions = self.sessions.write().expect("session lock poisoned");
        sessions.insert(id.clone());
        id
    }

    /// Remove a session by ID. No-op if the ID doesn't exist.
    pub fn remove_session(&self, id: &str) {
        let mut sessions = self.sessions.write().expect("session lock poisoned");
        sessions.remove(id);
    }

    /// Number of currently active sessions.
    pub fn active_count(&self) -> usize {
        let sessions = self.sessions.read().expect("session lock poisoned");
        sessions.len()
    }

    /// Returns true when no sessions are connected.
    pub fn is_idle(&self) -> bool {
        self.active_count() == 0
    }
}
