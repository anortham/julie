//! Gemini CLI backend implementation.
//!
//! Spawns `gemini -p "prompt"` via `tokio::process::Command` and streams
//! stdout line-by-line through a broadcast channel.

use anyhow::{Context, Result};
use tokio::sync::broadcast;

use super::backend::AgentBackend;

/// Gemini CLI backend.
///
/// Implements the `AgentBackend` trait by spawning `gemini -p` as a child
/// process and streaming its stdout through a broadcast channel.
pub struct GeminiBackend;

impl GeminiBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GeminiBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentBackend for GeminiBackend {
    fn name(&self) -> &str {
        "gemini"
    }

    fn is_available(&self) -> bool {
        super::backend::check_command_exists("gemini")
    }

    fn version(&self) -> Option<String> {
        super::backend::detect_cli_version("gemini")
    }

    fn dispatch(
        &self,
        prompt: &str,
        broadcast_tx: broadcast::Sender<String>,
    ) -> Result<tokio::task::JoinHandle<Result<String>>> {
        let child = tokio::process::Command::new("gemini")
            .arg("-p")
            .arg(prompt)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("Failed to spawn gemini CLI process")?;

        super::backend::spawn_and_stream(child, "gemini", broadcast_tx)
    }
}
