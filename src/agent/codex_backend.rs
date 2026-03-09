//! Codex CLI backend implementation.
//!
//! Spawns `codex exec "prompt" --full-auto --color never` via `tokio::process::Command`
//! and streams stdout line-by-line through a broadcast channel.

use anyhow::{Context, Result};
use tokio::sync::broadcast;

use super::backend::AgentBackend;

/// Codex CLI backend.
///
/// Implements the `AgentBackend` trait by spawning `codex exec` as a child
/// process and streaming its stdout through a broadcast channel.
pub struct CodexBackend;

impl CodexBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CodexBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentBackend for CodexBackend {
    fn name(&self) -> &str {
        "codex"
    }

    fn is_available(&self) -> bool {
        super::backend::check_command_exists("codex")
    }

    fn version(&self) -> Option<String> {
        super::backend::detect_cli_version("codex")
    }

    fn dispatch(
        &self,
        prompt: &str,
        broadcast_tx: broadcast::Sender<String>,
    ) -> Result<tokio::task::JoinHandle<Result<String>>> {
        let child = tokio::process::Command::new("codex")
            .arg("exec")
            .arg(prompt)
            .arg("--full-auto")
            .arg("--color")
            .arg("never")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("Failed to spawn codex CLI process")?;

        super::backend::spawn_and_stream(child, "codex", broadcast_tx)
    }
}
