//! Dispatch manager for agent tasks.
//!
//! The `DispatchManager` tracks active and completed dispatches, generates
//! dispatch IDs, and manages broadcast channels for output streaming.
//! Dispatches are ephemeral (held in memory).

use std::collections::HashMap;

use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;

use crate::agent::backend::BackendInfo;

/// Status of an agent dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
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
#[derive(Serialize)]
pub struct AgentDispatch {
    /// Unique dispatch ID: `dispatch_{SHA256(timestamp:task)[..8]}`
    pub id: String,
    /// The task description that was dispatched.
    pub task: String,
    /// The project this dispatch is associated with.
    pub project: String,
    /// The backend that executed this dispatch (e.g. "claude", "codex").
    pub backend: String,
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
    ///
    /// Set to `None` when the dispatch completes or fails, which drops the
    /// sender and causes `BroadcastStream` subscribers to terminate.
    #[serde(skip)]
    broadcast_tx: Option<broadcast::Sender<String>>,
}

/// Default broadcast channel capacity (lines buffered for late subscribers).
const BROADCAST_CAPACITY: usize = 256;

/// Manages agent dispatches.
///
/// Holds dispatches in memory (not persisted to DB).
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

    /// Maximum number of dispatches kept in memory. When exceeded, the oldest
    /// completed dispatches are evicted to bound memory usage.
    const MAX_DISPATCHES: usize = 100;

    /// Start a new dispatch and return its ID.
    ///
    /// Creates a dispatch record in `Running` status with a broadcast
    /// channel for output streaming. Evicts oldest completed dispatches
    /// if the map exceeds `MAX_DISPATCHES`.
    pub fn start_dispatch(&mut self, task: String, project: String, backend: String) -> String {
        // Evict oldest completed dispatches if we're at capacity
        if self.dispatches.len() >= Self::MAX_DISPATCHES {
            self.evict_oldest_completed();
        }

        let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let id = generate_dispatch_id(&timestamp, &task);

        let (broadcast_tx, _) = broadcast::channel(BROADCAST_CAPACITY);

        let dispatch = AgentDispatch {
            id: id.clone(),
            task,
            project,
            backend,
            status: DispatchStatus::Running,
            started_at: timestamp,
            completed_at: None,
            output: String::new(),
            error: None,
            broadcast_tx: Some(broadcast_tx),
        };

        self.dispatches.insert(id.clone(), dispatch);
        id
    }

    /// Evict the oldest completed/failed dispatches to stay under `MAX_DISPATCHES`.
    fn evict_oldest_completed(&mut self) {
        let target = Self::MAX_DISPATCHES / 2; // evict down to half capacity
        let mut completed: Vec<(String, String)> = self
            .dispatches
            .iter()
            .filter(|(_, d)| d.status != DispatchStatus::Running)
            .map(|(id, d)| (id.clone(), d.started_at.clone()))
            .collect();

        // Sort oldest first
        completed.sort_by(|a, b| a.1.cmp(&b.1));

        // Remove enough to get under target
        let to_remove = self.dispatches.len().saturating_sub(target);
        for (id, _) in completed.into_iter().take(to_remove) {
            self.dispatches.remove(&id);
        }
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
                if let Some(ref tx) = dispatch.broadcast_tx {
                    let _ = tx.send(output.to_string());
                }
            }
        }
    }

    /// Set the final accumulated output without broadcasting.
    ///
    /// Used when the agent backend has already streamed output via the
    /// broadcast channel and we just need to store the full result.
    /// Unlike `append_output`, this does NOT broadcast to SSE subscribers
    /// (avoiding duplicate delivery).
    pub fn set_final_output(&mut self, id: &str, output: String) {
        if let Some(dispatch) = self.dispatches.get_mut(id) {
            dispatch.output = output;
        }
    }

    /// Mark a dispatch as completed and drop the broadcast sender.
    ///
    /// Dropping the sender terminates any SSE `BroadcastStream` subscribers,
    /// allowing them to emit the final "done" event and close.
    pub fn complete_dispatch(&mut self, id: &str) {
        if let Some(dispatch) = self.dispatches.get_mut(id) {
            dispatch.status = DispatchStatus::Completed;
            dispatch.completed_at =
                Some(Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
            dispatch.broadcast_tx = None;
        }
    }

    /// Mark a dispatch as failed with an error message and drop the broadcast sender.
    ///
    /// Dropping the sender terminates any SSE `BroadcastStream` subscribers.
    pub fn fail_dispatch(&mut self, id: &str, error: &str) {
        if let Some(dispatch) = self.dispatches.get_mut(id) {
            dispatch.status = DispatchStatus::Failed;
            dispatch.error = Some(error.to_string());
            dispatch.completed_at =
                Some(Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
            dispatch.broadcast_tx = None;
        }
    }

    /// Subscribe to a dispatch's output broadcast channel.
    ///
    /// Returns a receiver that yields output lines as they arrive.
    /// Returns `None` if the dispatch doesn't exist or has already completed
    /// (broadcast sender dropped).
    pub fn subscribe(&self, id: &str) -> Option<broadcast::Receiver<String>> {
        self.dispatches
            .get(id)
            .and_then(|d| d.broadcast_tx.as_ref().map(|tx| tx.subscribe()))
    }

    /// Get a clone of the dispatch's broadcast sender.
    ///
    /// Used by the dispatch handler to pass the sender to the agent backend,
    /// so output lines are broadcast to SSE subscribers in real time.
    /// Returns `None` if the dispatch doesn't exist or has already completed.
    pub fn get_broadcast_tx(&self, id: &str) -> Option<broadcast::Sender<String>> {
        self.dispatches
            .get(id)
            .and_then(|d| d.broadcast_tx.clone())
    }

    /// List all dispatches (both active and completed), sorted by `started_at` descending.
    pub fn list_dispatches(&self) -> Vec<&AgentDispatch> {
        let mut dispatches: Vec<&AgentDispatch> = self.dispatches.values().collect();
        dispatches.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        dispatches
    }
}

impl Default for DispatchManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Lightweight snapshot of dispatch data.
///
/// Used to avoid holding the `DispatchManager` write lock during I/O.
pub struct DispatchSnapshot {
    pub task: String,
    pub output: String,
    pub project: String,
    pub backend: String,
    pub status: DispatchStatus,
    pub error: Option<String>,
}

impl From<&AgentDispatch> for DispatchSnapshot {
    fn from(d: &AgentDispatch) -> Self {
        Self {
            task: d.task.clone(),
            output: d.output.clone(),
            project: d.project.clone(),
            backend: d.backend.clone(),
            status: d.status.clone(),
            error: d.error.clone(),
        }
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
