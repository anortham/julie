//! Daemon lifecycle management: PID file, start/stop/status, signal handling.
//!
//! Provides cross-platform daemon process management:
//! - PID file at `~/.julie/daemon.pid` (TOML format) with exclusive file lock
//! - Process existence checking via `kill(pid, 0)` (Unix) or `tasklist` (Windows)
//! - Graceful shutdown on SIGTERM/SIGINT with PID file cleanup
//! - Double-start detection with atomic file locking (prevents TOCTOU races)

use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use fs2::FileExt;
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
/// - Windows: `%USERPROFILE%\.julie` (consistent with Unix — works naturally in MSYS/Git Bash)
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
        let base = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .context("Cannot determine Julie home directory: neither %USERPROFILE% nor $HOME is set")?;
        let new_home = PathBuf::from(&base).join(".julie");

        // Migrate from old location (%APPDATA%\julie) if it exists and new doesn't
        if !new_home.exists() {
            if let Ok(appdata) = std::env::var("APPDATA") {
                let old_home = PathBuf::from(appdata).join("julie");
                if old_home.exists() {
                    eprintln!(
                        "Migrating julie home: {:?} → {:?}",
                        old_home, new_home
                    );
                    if let Err(e) = std::fs::rename(&old_home, &new_home) {
                        eprintln!("Migration failed (will use new location): {e}");
                    }
                }
            }
        }

        Ok(new_home)
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

/// Open (or create) the PID file, acquire an exclusive lock, and write daemon info.
///
/// Returns the locked `File` handle. The caller MUST keep this handle alive for the
/// daemon's lifetime — dropping it releases the lock. The OS also releases the lock
/// automatically if the process crashes.
///
/// If another process already holds the lock, returns an error immediately
/// (non-blocking via `try_lock_exclusive`).
///
/// Uses a separate `.lock` file for the exclusive lock so the PID file itself
/// remains readable on all platforms. On Windows, exclusive byte-range locks
/// prevent ALL reads from other handles, so locking the PID file directly
/// would break `read_pid_file` / `is_daemon_running`.
pub fn lock_and_write_pid_file(path: &Path, pid: u32, port: u16) -> Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {:?}", parent))?;
    }

    // Lock a separate file to avoid blocking reads of the PID file on Windows
    let lock_path = path.with_extension("lock");
    let lock_file = File::create(&lock_path)
        .with_context(|| format!("Failed to create lock file {:?}", lock_path))?;

    lock_file.try_lock_exclusive().map_err(|_| {
        anyhow::anyhow!(
            "Another Julie daemon is already running (PID file {:?} is locked). \
             Use 'julie-server daemon stop' first.",
            path
        )
    })?;

    // Lock acquired — write the PID info to the (unlocked) PID file
    let info = DaemonInfo { pid, port };
    let content = toml::to_string(&info).context("Failed to serialize DaemonInfo to TOML")?;
    fs::write(path, content.as_bytes())
        .with_context(|| format!("Failed to write PID file {:?}", path))?;

    Ok(lock_file)
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
    // Guard: PIDs above i32::MAX would wrap negative, causing kill() to target
    // a process group instead of a single process — that's never what we want.
    if pid > i32::MAX as u32 {
        return false;
    }
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

    // Atomically lock the PID file to prevent TOCTOU race between is_daemon_running()
    // and starting the server. The lock is held for the daemon's entire lifetime.
    // _pid_file_lock must stay alive until after server shutdown — dropping it releases the lock.
    let _pid_file_lock = lock_and_write_pid_file(&pid_path, pid, port)?;
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

    // Drop the lock before removing the file (required on Windows where locked files
    // can't be deleted; on Unix this is harmless since flock is on the fd, not the path)
    drop(_pid_file_lock);

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

/// Restart the daemon: stop → copy current binary to ~/.julie/bin/ → start.
///
/// Designed for the development loop: `cargo build --release && julie-server daemon restart`
/// copies the freshly-built binary to the install location and bounces the daemon.
pub fn daemon_restart(port: u16) -> Result<()> {
    // 1. Stop existing daemon (if running)
    let pid_path = pid_file_path()?;
    if is_daemon_running(&pid_path).is_some() {
        daemon_stop()?;
    } else {
        println!("No running daemon found, starting fresh.");
    }

    // 2. Copy current binary to ~/.julie/bin/
    let home = julie_home()?;
    let bin_dir = home.join("bin");
    fs::create_dir_all(&bin_dir)
        .with_context(|| format!("Failed to create {}", bin_dir.display()))?;

    let installed_binary = if cfg!(windows) {
        bin_dir.join("julie-server.exe")
    } else {
        bin_dir.join("julie-server")
    };

    let current_exe =
        std::env::current_exe().context("Could not determine path of current executable")?;
    let current_canonical = current_exe.canonicalize().unwrap_or(current_exe.clone());
    let target_canonical = installed_binary
        .canonicalize()
        .unwrap_or(installed_binary.clone());

    if current_canonical != target_canonical {
        fs::copy(&current_exe, &installed_binary).with_context(|| {
            format!(
                "Failed to copy {} to {}",
                current_exe.display(),
                installed_binary.display()
            )
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o755);
            fs::set_permissions(&installed_binary, perms)
                .context("Failed to set binary permissions")?;
        }

        println!("Updated {}", installed_binary.display());
    } else {
        println!("Binary already at {}", installed_binary.display());
    }

    // 3. Start the daemon from the installed binary
    let port_str = port.to_string();

    #[cfg(unix)]
    {
        use std::process::Command;
        let child = Command::new(&installed_binary)
            .args(["daemon", "start", "--foreground", "--port", &port_str])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .with_context(|| format!("Failed to start {}", installed_binary.display()))?;
        println!("Julie daemon started (PID {}, port {})", child.id(), port);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

        let child = std::process::Command::new(&installed_binary)
            .args(["daemon", "start", "--foreground", "--port", &port_str])
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .spawn()
            .with_context(|| format!("Failed to start {}", installed_binary.display()))?;
        println!("Julie daemon started (PID {}, port {})", child.id(), port);
    }

    // 4. Wait briefly and verify it came up
    std::thread::sleep(std::time::Duration::from_secs(2));
    if is_daemon_running(&pid_path).is_some() {
        println!("Julie daemon is running on port {}.", port);
    } else {
        eprintln!("Warning: daemon may not have started. Check logs at ~/.julie/logs/");
    }

    Ok(())
}

/// Send a termination signal to the process.
#[cfg(unix)]
fn send_terminate_signal(pid: u32) -> Result<()> {
    if pid > i32::MAX as u32 {
        bail!("PID {} exceeds safe range for kill()", pid);
    }
    // SAFETY: Sending SIGTERM to a known PID within i32 range
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

/// Check if the current binary is newer than the running daemon.
///
/// Queries the daemon's health endpoint for uptime, then compares the binary's
/// file modification time against when the daemon started. Returns `true` if
/// the binary was modified after the daemon started (indicating a rebuild).
///
/// Defaults to `false` on any error (fail-safe: don't restart unless certain).
pub(crate) async fn is_binary_newer_than_daemon(port: u16) -> bool {
    use std::time::Duration;
    use tracing::{debug, info};

    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            debug!("Could not determine executable path: {}", e);
            return false;
        }
    };

    let exe_mtime = match fs::metadata(&exe_path).and_then(|m| m.modified()) {
        Ok(t) => t,
        Err(e) => {
            debug!("Could not get executable mtime: {}", e);
            return false;
        }
    };

    let url = format!("http://localhost:{}/api/health", port);
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    let resp = match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => return false,
    };

    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            debug!("Could not parse health response: {}", e);
            return false;
        }
    };

    let uptime_seconds = match body["uptime_seconds"].as_u64() {
        Some(u) => u,
        None => {
            debug!("Health response missing uptime_seconds");
            return false;
        }
    };

    let now = std::time::SystemTime::now();
    let daemon_start = now - Duration::from_secs(uptime_seconds);

    if exe_mtime > daemon_start {
        info!(
            "Binary is newer than running daemon (daemon started ~{}s ago)",
            uptime_seconds
        );
        true
    } else {
        false
    }
}
