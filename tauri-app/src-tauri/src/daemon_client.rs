//! Daemon lifecycle management via subprocess + HTTP.
//!
//! This module reimplements the minimal subset of Julie's daemon detection
//! to avoid depending on the full `julie` crate. Daemon start/stop is
//! delegated to the `julie-server` binary via subprocess calls.
//!
//! Key patterns borrowed from `julie::connect`:
//! - Startup lock (`daemon.startup.lock`) prevents race conditions
//! - Health check with exponential backoff after starting
//! - Log redirection to `~/.julie/logs/`

use std::fs::File;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// Default daemon port — matches `julie-server` default.
pub const DEFAULT_PORT: u16 = 7890;

/// Backoff schedule for health checks after starting daemon (milliseconds).
/// Mirrors `julie::connect::BACKOFF_MS`.
const HEALTH_BACKOFF_MS: &[u64] = &[50, 100, 200, 400, 800, 1600, 2000];

/// PID file content — mirrors `julie::daemon::DaemonInfo`.
#[derive(Debug, Clone, Deserialize)]
pub struct DaemonInfo {
    pub pid: u32,
    pub port: u16,
}

/// Health response from `/api/health`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub version: Option<String>,
    pub uptime_seconds: Option<f64>,
}

/// Per-project response from `GET /api/projects`.
/// Matches `julie::api::projects::ProjectResponse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectResponse {
    pub name: String,
    pub status: String,
    pub symbol_count: Option<u64>,
    pub file_count: Option<u64>,
    pub embedding_status: Option<EmbeddingStatus>,
}

/// Embedding status per project.
/// Matches `julie::api::projects::EmbeddingStatusResponse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingStatus {
    pub backend: String,
    pub accelerated: bool,
    pub degraded_reason: Option<String>,
}

// ============================================================================
// JULIE HOME + PID FILE
// ============================================================================

/// Returns `~/.julie` — same logic as `julie::daemon::julie_home()`.
pub fn julie_home() -> Result<PathBuf> {
    #[cfg(unix)]
    {
        let home =
            std::env::var("HOME").context("Cannot determine home directory: $HOME is not set")?;
        Ok(PathBuf::from(home).join(".julie"))
    }
    #[cfg(windows)]
    {
        let base = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .context("Cannot determine home directory")?;
        Ok(PathBuf::from(base).join(".julie"))
    }
}

/// Path to `~/.julie/daemon.pid`.
pub fn pid_file_path() -> Result<PathBuf> {
    Ok(julie_home()?.join("daemon.pid"))
}

/// Read and parse `~/.julie/daemon.pid` (TOML: `pid = N\nport = N`).
pub fn read_pid_file() -> Option<DaemonInfo> {
    let path = julie_home().ok()?.join("daemon.pid");
    let content = std::fs::read_to_string(&path).ok()?;
    toml::from_str(&content).ok()
}

// ============================================================================
// PROCESS EXISTENCE (cross-platform)
// ============================================================================

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    if pid > i32::MAX as u32 {
        return false;
    }
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(windows)]
fn process_exists(pid: u32) -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid), "/NH"])
        .output()
        .map(|output| {
            let stdout = String::from_utf8_lossy(&output.stdout);
            output.status.success() && !stdout.contains("No tasks")
        })
        .unwrap_or(false)
}

/// Check if the daemon is running by reading the PID file and verifying the process.
pub fn is_daemon_running() -> Option<DaemonInfo> {
    let info = read_pid_file()?;
    if process_exists(info.pid) {
        Some(info)
    } else {
        None
    }
}

// ============================================================================
// STARTUP LOCK
// ============================================================================

/// Acquire the startup lock (`~/.julie/daemon.startup.lock`).
///
/// This prevents races between the tray app and `julie connect` both
/// trying to start the daemon simultaneously. Uses non-blocking
/// `try_lock_exclusive()` with polling to avoid blocking the async runtime.
///
/// Returns the lock file handle — the lock is held until the handle is dropped.
pub async fn acquire_startup_lock() -> Result<File> {
    let lock_path = julie_home()?.join("daemon.startup.lock");

    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {:?}", parent))?;
    }

    let start = tokio::time::Instant::now();
    let timeout = Duration::from_secs(30);

    loop {
        let lock_file = File::create(&lock_path)
            .with_context(|| format!("Failed to create startup lock {:?}", lock_path))?;

        match fs2::FileExt::try_lock_exclusive(&lock_file) {
            Ok(()) => return Ok(lock_file),
            Err(_) if start.elapsed() < timeout => {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            Err(_) => {
                bail!(
                    "Timed out waiting for daemon startup lock. \
                     Another process may be stuck. Remove {:?} and retry.",
                    lock_path
                );
            }
        }
    }
}

// ============================================================================
// BINARY DISCOVERY
// ============================================================================

/// Find the `julie-server` binary. Search order:
/// 1. `~/.julie/bin/julie-server` (installed location)
/// 2. Same directory as this tray app binary
/// 3. PATH lookup
pub fn find_julie_binary() -> Option<PathBuf> {
    let bin_name = if cfg!(windows) {
        "julie-server.exe"
    } else {
        "julie-server"
    };

    // 1. Installed location
    if let Ok(home) = julie_home() {
        let installed = home.join("bin").join(bin_name);
        if installed.exists() {
            return Some(installed);
        }
    }

    // 2. Adjacent to tray app
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let adjacent = dir.join(bin_name);
            if adjacent.exists() {
                return Some(adjacent);
            }
        }
    }

    // 3. PATH lookup
    which_in_path(bin_name)
}

fn which_in_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|p| p.exists())
    })
}

// ============================================================================
// DAEMON LIFECYCLE (via subprocess)
// ============================================================================

/// Start the daemon, acquiring the startup lock first to prevent races.
/// Waits for the daemon to become healthy before returning.
pub async fn start_daemon_locked(port: u16) -> Result<()> {
    let _lock = acquire_startup_lock().await?;

    // Re-check under lock — another process may have started it
    if is_daemon_running().is_some() {
        return Ok(());
    }

    spawn_daemon(port)?;
    wait_for_health(port).await
    // _lock dropped here — releases for waiting clients
}

/// Spawn the daemon process with log redirection.
/// Matches `julie::connect::spawn_daemon` pattern.
fn spawn_daemon(port: u16) -> Result<()> {
    let binary = find_julie_binary().context("julie-server binary not found")?;

    // Set up log files
    let logs_dir = julie_home()?.join("logs");
    std::fs::create_dir_all(&logs_dir)
        .with_context(|| format!("Failed to create logs directory {:?}", logs_dir))?;

    let stdout_log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(logs_dir.join("daemon-stdout.log"))
        .context("Failed to open daemon stdout log")?;
    let stderr_log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(logs_dir.join("daemon-stderr.log"))
        .context("Failed to open daemon stderr log")?;

    #[cfg(unix)]
    {
        Command::new(&binary)
            .args(["daemon", "start", "--foreground", "--port", &port.to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::from(stdout_log))
            .stderr(std::process::Stdio::from(stderr_log))
            .spawn()
            .with_context(|| format!("Failed to start daemon: {:?}", binary))?;
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

        Command::new(&binary)
            .args(["daemon", "start", "--foreground", "--port", &port.to_string()])
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::from(stdout_log))
            .stderr(std::process::Stdio::from(stderr_log))
            .spawn()
            .with_context(|| format!("Failed to start daemon: {:?}", binary))?;
    }

    Ok(())
}

/// Stop the daemon by spawning `julie-server daemon stop`.
pub fn stop_daemon() -> Result<()> {
    let binary = find_julie_binary().context("julie-server binary not found")?;
    let output = Command::new(&binary)
        .args(["daemon", "stop"])
        .output()
        .context("Failed to stop daemon")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("daemon stop failed: {}", stderr.trim());
    }
    Ok(())
}

/// Restart the daemon by spawning `julie-server daemon restart`.
pub fn restart_daemon(port: u16) -> Result<()> {
    let binary = find_julie_binary().context("julie-server binary not found")?;
    let output = Command::new(&binary)
        .args(["daemon", "restart", "--port", &port.to_string()])
        .output()
        .context("Failed to restart daemon")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("daemon restart failed: {}", stderr.trim());
    }
    Ok(())
}

// ============================================================================
// SHARED HTTP CLIENT
// ============================================================================

/// Shared HTTP client — reused across all health checks, project fetches, etc.
/// Created once on first use to avoid allocating a new connection pool per call.
static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .user_agent("julie-tray")
            .build()
            .expect("Failed to create HTTP client")
    })
}

// ============================================================================
// HTTP HEALTH CHECKS
// ============================================================================

/// Wait for daemon to become healthy using exponential backoff.
/// Matches `julie::connect::wait_for_daemon_health`.
pub async fn wait_for_health(port: u16) -> Result<()> {
    let client = http_client();
    let url = format!("http://127.0.0.1:{}/api/health", port);

    for &delay_ms in HEALTH_BACKOFF_MS {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;

        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            _ => continue,
        }
    }

    bail!(
        "Daemon did not become healthy within {}ms",
        HEALTH_BACKOFF_MS.iter().sum::<u64>()
    )
}

/// Check daemon health via HTTP. Returns None on connection failure.
pub async fn check_health(port: u16) -> Option<HealthResponse> {
    http_client()
        .get(format!("http://127.0.0.1:{}/api/health", port))
        .send()
        .await
        .ok()?
        .json::<HealthResponse>()
        .await
        .ok()
}

/// Fetch project list via HTTP (`GET /api/projects`).
pub async fn fetch_projects(port: u16) -> Option<Vec<ProjectResponse>> {
    http_client()
        .get(format!("http://127.0.0.1:{}/api/projects", port))
        .send()
        .await
        .ok()?
        .json::<Vec<ProjectResponse>>()
        .await
        .ok()
}
