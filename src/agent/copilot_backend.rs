//! GitHub Copilot CLI backend implementation.
//!
//! Spawns `copilot -p "prompt" --autopilot` via `tokio::process::Command`
//! and streams stdout line-by-line through a broadcast channel.

use anyhow::{Context, Result};
use tokio::sync::broadcast;

use super::backend::AgentBackend;

/// GitHub Copilot CLI backend.
///
/// Implements the `AgentBackend` trait by spawning `copilot -p` as a child
/// process and streaming its stdout through a broadcast channel.
pub struct CopilotBackend;

impl CopilotBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CopilotBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentBackend for CopilotBackend {
    fn name(&self) -> &str {
        "copilot"
    }

    fn is_available(&self) -> bool {
        super::backend::check_command_exists("copilot")
    }

    fn version(&self) -> Option<String> {
        super::backend::detect_cli_version("copilot")
    }

    fn dispatch(
        &self,
        prompt: &str,
        broadcast_tx: broadcast::Sender<String>,
    ) -> Result<tokio::task::JoinHandle<Result<String>>> {
        let child = tokio::process::Command::new("copilot")
            .arg("-p")
            .arg(prompt)
            .arg("--autopilot")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("Failed to spawn copilot CLI process")?;

        super::backend::spawn_and_stream(child, "copilot", broadcast_tx)
    }
}
