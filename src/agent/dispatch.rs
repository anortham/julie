//! Dispatch manager for agent tasks.
//!
//! The `DispatchManager` tracks active and completed dispatches, generates
//! dispatch IDs, and manages broadcast channels for output streaming.
//! Dispatches are ephemeral (held in memory); completed results are
//! persisted as checkpoints via the memory system.

use std::collections::HashMap;

use chrono::Utc;
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;

/// Status of an agent dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchStatus {
    Running,
    Completed,
    Failed,
}

impl DispatchStatus {
    /// String representation for serialization/display.
    pub fn as_str(&self) -> &'static str {
        match self {
            DispatchStatus::Running => "running",
            DispatchStatus::Completed => "completed",
            DispatchStatus::Failed => "failed",
        }
    }
}

/// An active or completed agent dispatch.
///
/// Tracks the full lifecycle of a dispatched agent task: creation,
/// streaming output accumulation, and completion/failure.
pub struct AgentDispatch {
    /// Unique dispatch ID: `dispatch_{SHA256(timestamp:task)[..8]}`
    pub id: String,
    /// The task description that was dispatched.
    pub task: String,
    /// The project this dispatch is associated with.
    pub project: String,
    /// Current status.
    pub status: DispatchStatus,
    /// ISO 8601 UTC timestamp when the dispatch started.
    pub started_at: String,
    /// ISO 8601 UTC timestamp when the dispatch completed (or failed).
    pub completed_at: Option<String>,
    /// Accumulated output from the agent process.
    pub output: String,
    /// Error message if the dispatch failed.
    pub error: Option<String>,
    /// Broadcast sender for streaming output to SSE subscribers.
    broadcast_tx: broadcast::Sender<String>,
}

use crate::agent::backend::BackendInfo;

/// Default broadcast channel capacity (lines buffered for late subscribers).
const BROADCAST_CAPACITY: usize = 256;

/// Manages agent dispatches.
///
/// Holds dispatches in memory (not persisted to DB). Completed results
/// can be saved as checkpoints via the memory system by callers.
pub struct DispatchManager {
    dispatches: HashMap<String, AgentDispatch>,
    backends: Vec<BackendInfo>,
}

impl DispatchManager {
    /// Create a new empty dispatch manager.
    pub fn new() -> Self {
        Self {
            dispatches: HashMap::new(),
            backends: Vec::new(),
        }
    }

    /// Create a dispatch manager with detected backends.
    pub fn with_backends(backends: Vec<BackendInfo>) -> Self {
        Self {
            dispatches: HashMap::new(),
            backends,
        }
    }

    /// Get the list of detected backends.
    pub fn backends(&self) -> &[BackendInfo] {
        &self.backends
    }

    /// Start a new dispatch and return its ID.
    ///
    /// Creates a dispatch record in `Running` status with a broadcast
    /// channel for output streaming.
    pub fn start_dispatch(&mut self, task: String, project: String) -> String {
        let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let id = generate_dispatch_id(&timestamp, &task);

        let (broadcast_tx, _) = broadcast::channel(BROADCAST_CAPACITY);

        let dispatch = AgentDispatch {
            id: id.clone(),
            task,
            project,
            status: DispatchStatus::Running,
            started_at: timestamp,
            completed_at: None,
            output: String::new(),
            error: None,
            broadcast_tx,
        };

        self.dispatches.insert(id.clone(), dispatch);
        id
    }

    /// Get a dispatch by ID.
    pub fn get_dispatch(&self, id: &str) -> Option<&AgentDispatch> {
        self.dispatches.get(id)
    }

    /// Append output to a running dispatch and broadcast it.
    ///
    /// If the dispatch doesn't exist or is not running, this is a no-op.
    pub fn append_output(&mut self, id: &str, output: &str) {
        if let Some(dispatch) = self.dispatches.get_mut(id) {
            if dispatch.status == DispatchStatus::Running {
                dispatch.output.push_str(output);
                // Broadcast to subscribers (ignore errors — no subscribers is fine)
                let _ = dispatch.broadcast_tx.send(output.to_string());
            }
        }
    }

    /// Mark a dispatch as completed.
    pub fn complete_dispatch(&mut self, id: &str) {
        if let Some(dispatch) = self.dispatches.get_mut(id) {
            dispatch.status = DispatchStatus::Completed;
            dispatch.completed_at =
                Some(Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        }
    }

    /// Mark a dispatch as failed with an error message.
    pub fn fail_dispatch(&mut self, id: &str, error: &str) {
        if let Some(dispatch) = self.dispatches.get_mut(id) {
            dispatch.status = DispatchStatus::Failed;
            dispatch.error = Some(error.to_string());
            dispatch.completed_at =
                Some(Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        }
    }

    /// Subscribe to a dispatch's output broadcast channel.
    ///
    /// Returns a receiver that yields output lines as they arrive.
    /// Returns `None` if the dispatch doesn't exist.
    pub fn subscribe(&self, id: &str) -> Option<broadcast::Receiver<String>> {
        self.dispatches
            .get(id)
            .map(|d| d.broadcast_tx.subscribe())
    }

    /// List all dispatches (both active and completed).
    pub fn list_dispatches(&self) -> Vec<&AgentDispatch> {
        self.dispatches.values().collect()
    }
}

impl Default for DispatchManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a deterministic dispatch ID from timestamp and task.
///
/// Format: `dispatch_{first 8 hex chars of SHA-256(timestamp:task)}`
fn generate_dispatch_id(timestamp: &str, task: &str) -> String {
    let input = format!("{}:{}", timestamp, task);
    let hash = Sha256::digest(input.as_bytes());
    let hex = hex::encode(hash);
    format!("dispatch_{}", &hex[..8])
}
