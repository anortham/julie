//! Agent backend trait and backend detection.
//!
//! Defines the `AgentBackend` trait for agent CLI backends and provides
//! auto-detection of available backends (currently just Claude CLI).

use anyhow::Result;
use tokio::sync::broadcast;

/// A stream of string chunks from an agent process.
///
/// Wraps a broadcast receiver so multiple consumers (e.g. SSE endpoints)
/// can subscribe to the same output stream.
pub struct AgentStream {
    pub receiver: broadcast::Receiver<String>,
}

/// Trait for agent CLI backends.
///
/// Each backend represents a CLI tool that can be dispatched with a prompt
/// and returns streaming output. Currently only Claude CLI is supported,
/// but the trait allows future backends (e.g. other AI CLIs).
pub trait AgentBackend: Send + Sync {
    /// Human-readable name of this backend (e.g. "claude").
    fn name(&self) -> &str;

    /// Check if this backend's CLI is available on the system.
    ///
    /// Typically runs `which <cli>` or equivalent to detect installation.
    fn is_available(&self) -> bool;

    /// Get the version string of the backend CLI, if detectable.
    fn version(&self) -> Option<String>;

    /// Dispatch a prompt to the backend and return a stream of output chunks.
    ///
    /// The returned `AgentStream` wraps a broadcast channel that yields
    /// line-by-line output from the child process.
    fn dispatch(
        &self,
        prompt: &str,
        broadcast_tx: broadcast::Sender<String>,
    ) -> Result<tokio::task::JoinHandle<Result<String>>>;
}

/// Information about a detected backend.
#[derive(Debug, Clone)]
pub struct BackendInfo {
    pub name: String,
    pub available: bool,
    pub version: Option<String>,
}

/// Detect all known backends and their availability.
///
/// Currently checks for:
/// - `claude` CLI (Claude Code)
///
/// Runs `which` to check availability. This is a synchronous check
/// suitable for startup-time detection.
pub fn detect_backends() -> Vec<BackendInfo> {
    use crate::agent::claude_backend::ClaudeBackend;

    let claude = ClaudeBackend::new();
    let available = claude.is_available();
    let version = if available { claude.version() } else { None };

    vec![BackendInfo {
        name: claude.name().to_string(),
        available,
        version,
    }]
}
