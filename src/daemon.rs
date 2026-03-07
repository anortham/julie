//! Daemon lifecycle management: PID file, start/stop/status, signal handling.
//!
//! Provides cross-platform daemon process management:
//! - PID file at `~/.julie/daemon.pid` (TOML format)
//! - Process existence checking via `kill(pid, 0)` (Unix) or `tasklist` (Windows)
//! - Graceful shutdown on SIGTERM/SIGINT with PID file cleanup
//! - Double-start detection

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// Information stored in the PID file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonInfo {
    pub pid: u32,
    pub port: u16,
}

// ============================================================================
// JULIE HOME DIRECTORY
// ============================================================================

/// Returns the Julie home directory.
///
/// - Unix: `$HOME/.julie`
/// - Windows: `%APPDATA%\julie` (falls back to `%USERPROFILE%\julie`)
///
/// Returns an error if the home directory cannot be determined.
pub fn julie_home() -> Result<PathBuf> {
    #[cfg(unix)]
    {
        let home = std::env::var("HOME")
            .context("Cannot determine Julie home directory: $HOME is not set")?;
        Ok(PathBuf::from(home).join(".julie"))
    }
    #[cfg(windows)]
    {
        let base = std::env::var("APPDATA")
            .or_else(|_| std::env::var("USERPROFILE"))
            .context("Cannot determine Julie home directory: neither %APPDATA% nor %USERPROFILE% is set")?;
        Ok(PathBuf::from(base).join("julie"))
    }
}

/// Returns the path to the PID file: `julie_home()/daemon.pid`
pub fn pid_file_path() -> Result<PathBuf> {
    Ok(julie_home()?.join("daemon.pid"))
}

// ============================================================================
// PID FILE OPERATIONS
// ============================================================================

/// Write a PID file with the given process ID and port.
///
/// Creates parent directories if they don't exist.
pub fn write_pid_file(path: &Path, pid: u32, port: u16) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {:?}", parent))?;
    }

    let info = DaemonInfo { pid, port };
    let content = toml::to_string(&info).context("Failed to serialize DaemonInfo to TOML")?;
    fs::write(path, content).with_context(|| format!("Failed to write PID file {:?}", path))?;
    Ok(())
}

/// Read and parse a PID file. Returns `None` if the file doesn't exist.
///
/// Returns an error if the file exists but contains invalid content.
pub fn read_pid_file(path: &Path) -> Result<Option<DaemonInfo>> {
    if !path.exists() {
        return Ok(None);
    }

    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read PID file {:?}", path))?;
    let info: DaemonInfo =
        toml::from_str(&content).with_context(|| format!("Failed to parse PID file {:?}", path))?;
    Ok(Some(info))
}

/// Remove the PID file. No-op if the file doesn't exist.
pub fn remove_pid_file(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path)
            .with_context(|| format!("Failed to remove PID file {:?}", path))?;
    }
    Ok(())
}

// ============================================================================
// PROCESS EXISTENCE CHECKING (Cross-Platform)
// ============================================================================

/// Check if a process with the given PID exists.
#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    // SAFETY: kill with signal 0 doesn't send a signal, just checks process existence.
    // Returns 0 if the process exists and we have permission to signal it.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(windows)]
fn process_exists(pid: u32) -> bool {
    use std::process::Command;
    // tasklist with PID filter — exit code 0 if process found
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid), "/NH"])
        .output()
        .map(|output| {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // tasklist prints "INFO: No tasks are running..." when not found
            output.status.success() && !stdout.contains("No tasks")
        })
        .unwrap_or(false)
}

/// Check if the daemon is running by reading the PID file and verifying the process.
///
/// Returns `Some(DaemonInfo)` if the daemon is running, `None` otherwise.
/// Cleans up stale PID files (where the process no longer exists).
pub fn is_daemon_running(pid_path: &Path) -> Option<DaemonInfo> {
    let info = match read_pid_file(pid_path) {
        Ok(Some(info)) => info,
        Ok(None) => return None,
        Err(_) => {
            // Corrupt PID file — clean it up
            let _ = remove_pid_file(pid_path);
            return None;
        }
    };

    if process_exists(info.pid) {
        Some(info)
    } else {
        // Stale PID file — process no longer exists
        let _ = remove_pid_file(pid_path);
        None
    }
}

// ============================================================================
// DAEMON LIFECYCLE: START / STOP / STATUS
// ============================================================================

/// Start the daemon.
///
/// Currently only foreground mode is supported. Background daemonization is planned
/// for a later phase — calling with `foreground == false` returns an error.
///
/// 1. Rejects background mode (not yet implemented)
/// 2. Checks for an already-running daemon (double-start detection)
/// 3. Writes PID file
/// 4. Starts HTTP server with graceful shutdown
/// 5. When shutdown signal received, server stops gracefully
/// 6. Cleans up PID file on exit
pub async fn daemon_start(port: u16, workspace_root: PathBuf, foreground: bool) -> Result<()> {
    if !foreground {
        bail!(
            "Background daemon mode is not yet implemented. Use --foreground flag."
        );
    }

    let pid_path = pid_file_path()?;

    // Double-start detection
    if let Some(info) = is_daemon_running(&pid_path) {
        bail!(
            "Julie daemon is already running (PID {}, port {}). \
             Use 'julie-server daemon stop' first.",
            info.pid,
            info.port
        );
    }

    let pid = std::process::id();

    // Ensure julie_home directory exists
    let home = julie_home()?;
    fs::create_dir_all(&home)
        .with_context(|| format!("Failed to create Julie home directory {:?}", home))?;

    // Load global registry (creates empty if missing)
    let registry = crate::registry::GlobalRegistry::load(&home)
        .with_context(|| format!("Failed to load global registry from {:?}", home))?;
    tracing::info!("Loaded global registry: {} project(s)", registry.projects.len());

    write_pid_file(&pid_path, pid, port)?;
    println!("Julie daemon started (PID {}, port {})", pid, port);

    // Start the HTTP server — runs until a shutdown signal is received
    let server_result = crate::server::start_server(
        port,
        workspace_root,
        shutdown_signal(),
        registry,
        home.clone(),
    )
    .await;

    // Always clean up PID file on exit
    if let Err(e) = remove_pid_file(&pid_path) {
        eprintln!("Warning: failed to remove PID file: {}", e);
    }
    println!("Julie daemon stopped (PID {})", pid);

    server_result
}

/// Cross-platform shutdown signal handler.
///
/// Waits for SIGINT (ctrl-c) on all platforms, plus SIGTERM on Unix.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install ctrl-c handler");
    };

    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler");

        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await;
    }
}

/// Stop the running daemon.
///
/// Reads the PID file, sends a termination signal, waits for the process to exit,
/// and removes the PID file.
pub fn daemon_stop() -> Result<()> {
    let pid_path = pid_file_path()?;

    let info = match is_daemon_running(&pid_path) {
        Some(info) => info,
        None => {
            println!("Julie daemon is not running.");
            return Ok(());
        }
    };

    println!("Stopping Julie daemon (PID {})...", info.pid);
    send_terminate_signal(info.pid)?;

    // Wait for the process to exit (up to 5 seconds)
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if !process_exists(info.pid) {
            break;
        }
        if std::time::Instant::now() > deadline {
            eprintln!("Warning: daemon (PID {}) did not exit within 5 seconds", info.pid);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Clean up PID file
    remove_pid_file(&pid_path)?;
    println!("Julie daemon stopped.");
    Ok(())
}

/// Send a termination signal to the process.
#[cfg(unix)]
fn send_terminate_signal(pid: u32) -> Result<()> {
    // SAFETY: Sending SIGTERM to a known PID
    let result = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    if result != 0 {
        let err = std::io::Error::last_os_error();
        bail!("Failed to send SIGTERM to PID {}: {}", pid, err);
    }
    Ok(())
}

#[cfg(windows)]
fn send_terminate_signal(pid: u32) -> Result<()> {
    use std::process::Command;
    let output = Command::new("taskkill")
        .args(["/PID", &pid.to_string()])
        .output()
        .context("Failed to run taskkill")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("taskkill failed for PID {}: {}", pid, stderr);
    }
    Ok(())
}

/// Show daemon status.
///
/// Reports whether the daemon is running, and if so, its PID and port.
pub fn daemon_status() -> Result<()> {
    let pid_path = pid_file_path()?;

    match is_daemon_running(&pid_path) {
        Some(info) => {
            println!("Julie daemon is running:");
            println!("  PID:  {}", info.pid);
            println!("  Port: {}", info.port);
        }
        None => {
            println!("Julie daemon is not running.");
        }
    }
    Ok(())
}
