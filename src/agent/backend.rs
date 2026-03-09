//! Agent backend trait, shared utilities, and backend detection/factory.
//!
//! Defines the `AgentBackend` trait for agent CLI backends and provides
//! auto-detection of available backends and a factory for creating them by name.

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::sync::broadcast;
use tracing;

/// Trait for agent CLI backends.
///
/// Each backend represents a CLI tool that can be dispatched with a prompt
/// and returns streaming output. Supported backends: Claude, Codex, Gemini, Copilot.
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
    /// The returned handle wraps a tokio task that reads stdout line-by-line
    /// and broadcasts each line through the provided sender.
    fn dispatch(
        &self,
        prompt: &str,
        broadcast_tx: broadcast::Sender<String>,
    ) -> Result<tokio::task::JoinHandle<Result<String>>>;
}

/// Information about a detected backend.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct BackendInfo {
    pub name: String,
    pub available: bool,
    pub version: Option<String>,
}

// ---------------------------------------------------------------------------
// Shared CLI utilities (used by all backend implementations)
// ---------------------------------------------------------------------------

/// Check if a command exists using `which` (Unix) or `where` (Windows).
pub fn check_command_exists(cmd: &str) -> bool {
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

/// Get the version of a CLI tool by running `<cmd> --version`.
pub fn detect_cli_version(cmd: &str) -> Option<String> {
    let output = std::process::Command::new(cmd)
        .arg("--version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if output.status.success() {
        // Take first line only — some CLIs (e.g. copilot) append extra text after the version
        let full = String::from_utf8_lossy(&output.stdout);
        let version = full.lines().next().unwrap_or("").trim().to_string();
        if version.is_empty() {
            None
        } else {
            Some(version)
        }
    } else {
        None
    }
}

/// Spawn a CLI process and stream its stdout line-by-line through a broadcast channel.
///
/// This is the shared dispatch implementation used by all backends. It:
/// - Streams stdout lines in real-time via `broadcast_tx`
/// - Captures stderr separately
/// - On failure, includes stderr in the error message (surfaces auth errors, config issues, etc.)
pub fn spawn_and_stream(
    mut child: tokio::process::Child,
    cli_name: &str,
    broadcast_tx: broadcast::Sender<String>,
) -> Result<tokio::task::JoinHandle<Result<String>>> {
    let stdout = child
        .stdout
        .take()
        .context(format!("Failed to capture {} stdout", cli_name))?;

    let stderr = child
        .stderr
        .take()
        .context(format!("Failed to capture {} stderr", cli_name))?;

    let name = cli_name.to_string();

    let handle = tokio::spawn(async move {
        // Read stdout line-by-line and broadcast
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut accumulated = String::new();

        while let Some(line) = lines.next_line().await? {
            let line_with_newline = format!("{}\n", line);
            accumulated.push_str(&line_with_newline);
            let _ = broadcast_tx.send(line_with_newline);
        }

        // Wait for the child process to finish
        let status = child.wait().await?;
        if !status.success() {
            // Read stderr for the actual error message (auth failures, config issues, etc.)
            let mut stderr_buf = String::new();
            let mut stderr_reader = BufReader::new(stderr);
            let _ = stderr_reader.read_to_string(&mut stderr_buf).await;
            let stderr_msg = stderr_buf.trim();

            // Log full stderr server-side for debugging, but don't expose it
            // in API responses (stderr may contain API keys, tokens, or config)
            if !stderr_msg.is_empty() {
                tracing::error!("{} stderr: {}", name, stderr_msg);
            }
            anyhow::bail!(
                "{} process exited with status {} (check server logs for details)",
                name,
                status.code().unwrap_or(-1)
            );
        }

        Ok(accumulated)
    });

    Ok(handle)
}

// ---------------------------------------------------------------------------
// Backend detection and factory
// ---------------------------------------------------------------------------

/// All known backend constructors. Add new backends here.
fn all_backends() -> Vec<Box<dyn AgentBackend>> {
    use crate::agent::claude_backend::ClaudeBackend;
    use crate::agent::codex_backend::CodexBackend;
    use crate::agent::copilot_backend::CopilotBackend;
    use crate::agent::gemini_backend::GeminiBackend;

    vec![
        Box::new(ClaudeBackend::new()),
        Box::new(CodexBackend::new()),
        Box::new(GeminiBackend::new()),
        Box::new(CopilotBackend::new()),
    ]
}

/// Detect all known backends and their availability.
///
/// Checks for: Claude Code, Codex, Gemini CLI, GitHub Copilot CLI.
/// Runs `which` to check availability. This is a synchronous check
/// suitable for startup-time detection.
pub fn detect_backends() -> Vec<BackendInfo> {
    all_backends()
        .iter()
        .map(|b| {
            let available = b.is_available();
            BackendInfo {
                name: b.name().to_string(),
                available,
                version: if available { b.version() } else { None },
            }
        })
        .collect()
}

/// Create a backend by name.
///
/// Returns `None` if the name doesn't match any known backend.
pub fn create_backend(name: &str) -> Option<Box<dyn AgentBackend>> {
    use crate::agent::claude_backend::ClaudeBackend;
    use crate::agent::codex_backend::CodexBackend;
    use crate::agent::copilot_backend::CopilotBackend;
    use crate::agent::gemini_backend::GeminiBackend;

    match name {
        "claude" => Some(Box::new(ClaudeBackend::new())),
        "codex" => Some(Box::new(CodexBackend::new())),
        "gemini" => Some(Box::new(GeminiBackend::new())),
        "copilot" => Some(Box::new(CopilotBackend::new())),
        _ => None,
    }
}
