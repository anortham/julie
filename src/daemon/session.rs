//! Session tracking for idle detection and control-plane visibility.
//!
//! Tracks active MCP sessions, drives the idle-reaper that reclaims abandoned
//! sessions, and surfaces coarse lifecycle phases for the dashboard.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;

/// Env override for the session idle-reaper threshold (seconds).
const SESSION_IDLE_TIMEOUT_ENV: &str = "JULIE_DAEMON_SESSION_IDLE_TIMEOUT_SECS";
/// Default: a session with zero activity for this long while a daemon restart
/// is pending is presumed abandoned and reaped so the restart can proceed.
/// Generous enough that ordinary think/read time between requests never trips
/// it; bounded so a genuinely leaked (half-open) connection can't defer a
/// stale-binary restart forever.
const DEFAULT_SESSION_IDLE_TIMEOUT_SECS: u64 = 300;

/// Resolve the session idle-reaper threshold, honoring the env override.
pub fn session_idle_timeout() -> Duration {
    std::env::var(SESSION_IDLE_TIMEOUT_ENV)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(DEFAULT_SESSION_IDLE_TIMEOUT_SECS))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionLifecyclePhase {
    Connecting,
    Bound,
    Serving,
    Closing,
}

impl SessionLifecyclePhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Connecting => "CONNECTING",
            Self::Bound => "BOUND",
            Self::Serving => "SERVING",
            Self::Closing => "CLOSING",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct SessionPhaseCounts {
    pub connecting: usize,
    pub bound: usize,
    pub serving: usize,
    pub closing: usize,
}

#[derive(Clone)]
pub struct SessionLifecycleHandle {
    tracker: Arc<SessionTracker>,
    session_id: String,
}

impl SessionLifecycleHandle {
    pub fn set_phase(&self, phase: SessionLifecyclePhase) {
        self.tracker.set_phase(&self.session_id, phase);
    }

    pub fn set_current_workspace(&self, workspace_id: Option<String>) {
        self.tracker
            .set_current_workspace(&self.session_id, workspace_id);
    }
}

#[derive(Debug, Clone)]
struct SessionRecord {
    phase: SessionLifecyclePhase,
    current_workspace_id: Option<String>,
    /// Last time this session handled a request. Drives the idle-reaper that
    /// unblocks a pending stale-binary restart without force-aborting a
    /// still-active session.
    last_activity: Instant,
}

/// Tracks active MCP sessions connected to the daemon.
///
/// Thread-safe via `RwLock`. Each session gets a UUID on connect;
/// the UUID is removed when the session ends (normally or on error).
pub struct SessionTracker {
    sessions: RwLock<HashMap<String, SessionRecord>>,
}

impl SessionTracker {
    /// Create an empty session tracker.
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Register a new session. Returns the generated session ID (UUID v4).
    pub fn add_session(&self) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let mut sessions = self.sessions.write().unwrap_or_else(|p| p.into_inner());
        sessions.insert(
            id.clone(),
            SessionRecord {
                phase: SessionLifecyclePhase::Connecting,
                current_workspace_id: None,
                last_activity: Instant::now(),
            },
        );
        id
    }

    /// Record activity for a session, resetting its idle clock. Returns false
    /// if the session is not tracked (e.g. already removed). Called on every
    /// request so the idle-reaper never evicts a session that is still in use.
    pub fn touch_session(&self, id: &str) -> bool {
        self.touch_session_at(id, Instant::now())
    }

    /// Like [`touch_session`] but records an explicit instant. Exposed for the
    /// reaper's tests so idle timing can be driven deterministically without
    /// sleeping.
    pub(crate) fn touch_session_at(&self, id: &str, at: Instant) -> bool {
        let mut sessions = self.sessions.write().unwrap_or_else(|p| p.into_inner());
        match sessions.get_mut(id) {
            Some(record) => {
                record.last_activity = at;
                true
            }
            None => false,
        }
    }

    pub fn set_phase(&self, id: &str, phase: SessionLifecyclePhase) -> bool {
        let mut sessions = self.sessions.write().unwrap_or_else(|p| p.into_inner());
        match sessions.get_mut(id) {
            Some(current) => {
                current.phase = phase;
                true
            }
            None => false,
        }
    }

    pub fn set_current_workspace(&self, id: &str, workspace_id: Option<String>) -> bool {
        let mut sessions = self.sessions.write().unwrap_or_else(|p| p.into_inner());
        match sessions.get_mut(id) {
            Some(current) => {
                current.current_workspace_id = workspace_id;
                true
            }
            None => false,
        }
    }

    pub fn session_phase(&self, id: &str) -> Option<SessionLifecyclePhase> {
        self.sessions
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(id)
            .map(|record| record.phase)
    }

    pub fn phase_counts(&self) -> SessionPhaseCounts {
        let sessions = self.sessions.read().unwrap_or_else(|p| p.into_inner());
        let mut counts = SessionPhaseCounts::default();

        for record in sessions.values() {
            match record.phase {
                SessionLifecyclePhase::Connecting => counts.connecting += 1,
                SessionLifecyclePhase::Bound => counts.bound += 1,
                SessionLifecyclePhase::Serving => counts.serving += 1,
                SessionLifecyclePhase::Closing => counts.closing += 1,
            }
        }

        counts
    }

    pub fn lifecycle_handle(self: &Arc<Self>, id: &str) -> SessionLifecycleHandle {
        SessionLifecycleHandle {
            tracker: Arc::clone(self),
            session_id: id.to_string(),
        }
    }

    pub fn current_workspace_counts(&self) -> HashMap<String, usize> {
        let sessions = self.sessions.read().unwrap_or_else(|p| p.into_inner());
        let mut counts = HashMap::new();
        for record in sessions.values() {
            if let Some(workspace_id) = record.current_workspace_id.as_ref() {
                *counts.entry(workspace_id.clone()).or_insert(0) += 1;
            }
        }
        counts
    }

    /// Remove a session by ID. No-op if the ID doesn't exist.
    pub fn remove_session(&self, id: &str) {
        let mut sessions = self.sessions.write().unwrap_or_else(|p| p.into_inner());
        sessions.remove(id);
    }

    /// Number of currently active sessions.
    pub fn active_count(&self) -> usize {
        let sessions = self.sessions.read().unwrap_or_else(|p| p.into_inner());
        sessions.len()
    }

}
