//! Claude CLI backend implementation.
//!
//! Spawns `claude -p "prompt"` via `tokio::process::Command` and streams
//! stdout line-by-line through a broadcast channel.

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::broadcast;

use super::backend::AgentBackend;

/// Claude CLI backend.
///
/// Implements the `AgentBackend` trait by spawning `claude -p` as a child
/// process and streaming its stdout through a broadcast channel.
pub struct ClaudeBackend;

impl ClaudeBackend {
    pub fn new() -> Self {
        Self
    }

    /// Check if a command exists using `which` (Unix) or `where` (Windows).
    fn check_command_exists(cmd: &str) -> bool {
        #[cfg(unix)]
        let result = std::process::Command::new("which")
            .arg(cmd)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        #[cfg(windows)]
        let result = std::process::Command::new("where")
            .arg(cmd)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        matches!(result, Ok(status) if status.success())
    }

    /// Get the version of the Claude CLI by running `claude --version`.
    fn detect_version() -> Option<String> {
        let output = std::process::Command::new("claude")
            .arg("--version")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if version.is_empty() {
                None
            } else {
                Some(version)
            }
        } else {
            None
        }
    }
}

impl Default for ClaudeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentBackend for ClaudeBackend {
    fn name(&self) -> &str {
        "claude"
    }

    fn is_available(&self) -> bool {
        Self::check_command_exists("claude")
    }

    fn version(&self) -> Option<String> {
        Self::detect_version()
    }

    fn dispatch(
        &self,
        prompt: &str,
        broadcast_tx: broadcast::Sender<String>,
    ) -> Result<tokio::task::JoinHandle<Result<String>>> {
        let mut child = tokio::process::Command::new("claude")
            .arg("-p")
            .arg(prompt)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("Failed to spawn claude CLI process")?;

        let stdout = child
            .stdout
            .take()
            .context("Failed to capture claude stdout")?;

        let handle = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            let mut accumulated = String::new();

            while let Some(line) = lines.next_line().await? {
                let line_with_newline = format!("{}\n", line);
                accumulated.push_str(&line_with_newline);
                // Broadcast to any subscribers (ignore errors — no subscribers is fine)
                let _ = broadcast_tx.send(line_with_newline);
            }

            // Wait for the child process to finish
            let status = child.wait().await?;
            if !status.success() {
                anyhow::bail!(
                    "claude process exited with status: {}",
                    status.code().unwrap_or(-1)
                );
            }

            Ok(accumulated)
        });

        Ok(handle)
    }
}
