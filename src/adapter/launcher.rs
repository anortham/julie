//! Daemon launcher: auto-starts the daemon process if not already running.
//!
//! The launcher checks for a running daemon via PID file, acquires an advisory
//! lock to prevent races between multiple adapters, spawns the daemon as a
//! detached background process, and waits for the IPC socket to appear.

use std::io;
use std::time::{Duration, Instant};

use fs2::FileExt;
use tracing::{debug, info};

use crate::daemon::pid::PidFile;
use crate::paths::DaemonPaths;

/// Manages daemon lifecycle from the adapter's perspective: detect, launch, wait.
/// The daemon's current lifecycle phase, as seen by the adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonReadiness {
    /// PID alive, state file says "ready". Safe to connect.
    Ready,
    /// PID alive, state file says "starting" or is missing/unreadable.
    /// Daemon is initializing; wait, don't spawn a second one.
    Starting,
    /// PID alive, state file says "stopping".
    /// Daemon is shutting down; wait for exit, then spawn fresh.
    Stopping,
    /// No PID file, or PID is dead. Safe to spawn a new daemon.
    Dead,
}

pub struct DaemonLauncher {
    paths: DaemonPaths,
}

impl DaemonLauncher {
    /// Create a new launcher bound to the given daemon paths.
    pub fn new(paths: DaemonPaths) -> Self {
        Self { paths }
    }

    /// Read-only accessor for the paths this launcher uses.
    pub fn paths(&self) -> &DaemonPaths {
        &self.paths
    }

    /// Check whether a daemon process is currently running.
    ///
    /// Reads the PID file, validates the process is alive, and cleans up
    /// stale PID files as a side effect.
    fn is_daemon_running(&self) -> bool {
        PidFile::check_running(&self.paths.daemon_pid()).is_some()
    }

    /// Probe the IPC endpoint to check if the daemon is accepting connections.
    /// Used as a fallback when the state file is missing (old binary, write failure).
    fn probe_ipc_endpoint(&self) -> bool {
        let ipc_addr = self.paths.daemon_ipc_addr();

        #[cfg(unix)]
        return std::os::unix::net::UnixStream::connect(&ipc_addr).is_ok();

        #[cfg(windows)]
        return win_pipe_exists(&ipc_addr);
    }

    /// Assess the daemon's lifecycle phase from PID + state file.
    ///
    /// Cleans up stale files as a side effect when the daemon is dead.
    /// When PID is alive but the state file is missing or unreadable (old binary,
    /// write failure, permissions), falls back to probing the IPC endpoint.
    pub fn daemon_readiness(&self) -> DaemonReadiness {
        match PidFile::check_running(&self.paths.daemon_pid()) {
            None => {
                let _ = std::fs::remove_file(self.paths.daemon_state());
                DaemonReadiness::Dead
            }
            Some(_pid) => {
                match std::fs::read_to_string(self.paths.daemon_state()) {
                    Ok(s) if s.trim() == "ready" => DaemonReadiness::Ready,
                    Ok(s) if s.trim() == "stopping" => DaemonReadiness::Stopping,
                    _ => {
                        // State file missing or unreadable (old binary, write failure).
                        // Fall back to IPC probe: if the socket accepts connections,
                        // the daemon is ready regardless of state file.
                        if self.probe_ipc_endpoint() {
                            DaemonReadiness::Ready
                        } else {
                            DaemonReadiness::Starting
                        }
                    }
                }
            }
        }
    }

    /// Ensure the daemon is running and ready to accept connections.
    ///
    /// State-file aware: instead of just checking PID liveness, reads the
    /// daemon.state file to distinguish starting/ready/stopping. Holds
    /// daemon.lock through the entire readiness check to prevent multi-adapter
    /// races.
    pub fn ensure_daemon_ready(&self) -> io::Result<()> {
        // Fast path (no lock): if daemon is already ready, skip the lock.
        // If the daemon transitions to stopping between this check and
        // connect_and_handshake, run_adapter's retry loop catches it.
        if matches!(self.daemon_readiness(), DaemonReadiness::Ready) {
            debug!("Daemon already ready (fast path)");
            return Ok(());
        }

        // Ensure the julie home directory exists for the lock file
        self.paths.ensure_dirs().map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to create daemon directories: {}", e),
            )
        })?;

        // Acquire advisory lock to serialize daemon startup across adapters
        let lock_path = self.paths.daemon_lock();
        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)?;

        debug!("Acquiring daemon startup lock: {}", lock_path.display());
        lock_file.lock_exclusive()?;

        let deadline = Instant::now() + Duration::from_secs(60);

        let result = self.wait_for_daemon_ready(deadline);

        // Release lock
        lock_file.unlock()?;
        drop(lock_file);

        result
    }

    /// Internal: poll until the daemon reaches Ready state or deadline expires.
    fn wait_for_daemon_ready(&self, deadline: Instant) -> io::Result<()> {
        loop {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Timed out waiting for daemon readiness",
                ));
            }

            match self.daemon_readiness() {
                DaemonReadiness::Ready => {
                    debug!("Daemon is ready");
                    return Ok(());
                }
                DaemonReadiness::Starting => {
                    debug!("Daemon is starting, waiting for ready...");
                    match self.poll_for_state_change("ready", deadline) {
                        Ok(()) => {} // Will re-check in next loop iteration
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => {
                            // State became "stopping"; loop back to re-assess
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                }
                DaemonReadiness::Stopping => {
                    info!("Daemon is stopping, waiting for exit...");
                    self.wait_for_pid_exit(deadline)?;
                    // PID gone; fall through to Dead on next iteration
                }
                DaemonReadiness::Dead => {
                    info!("Daemon not running, spawning...");
                    self.spawn_daemon()?;
                    match self.poll_for_state_change("ready", deadline) {
                        Ok(()) => {} // Will re-check in next loop iteration
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => {
                            continue;
                        }
                        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {
                            // Daemon died during startup; loop back to re-assess
                            // (will see Dead again and respawn, up to deadline)
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                }
            }
        }
    }

    /// Poll the state file until it contains `target_state`, the daemon dies,
    /// or the state becomes "stopping" (when waiting for "ready").
    fn poll_for_state_change(&self, target_state: &str, deadline: Instant) -> io::Result<()> {
        let mut delay = Duration::from_millis(50);
        let max_delay = Duration::from_millis(500);

        loop {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("Timed out waiting for daemon state '{}'", target_state),
                ));
            }

            std::thread::sleep(delay);
            delay = (delay * 2).min(max_delay);

            // Check if daemon died while we were waiting
            if !self.is_daemon_running() {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "Daemon exited while waiting for readiness",
                ));
            }

            // Check current state
            if let Ok(s) = std::fs::read_to_string(self.paths.daemon_state()) {
                let state = s.trim();
                if state == target_state {
                    return Ok(());
                }
                if target_state == "ready" && state == "stopping" {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "Daemon transitioned to stopping before reaching ready",
                    ));
                }
            }
        }
    }

    /// Poll until the daemon's PID file is gone (process exited).
    fn wait_for_pid_exit(&self, deadline: Instant) -> io::Result<()> {
        let mut delay = Duration::from_millis(50);
        let max_delay = Duration::from_millis(500);

        loop {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Timed out waiting for daemon to exit",
                ));
            }

            if !self.is_daemon_running() {
                let _ = std::fs::remove_file(self.paths.daemon_state());
                return Ok(());
            }

            std::thread::sleep(delay);
            delay = (delay * 2).min(max_delay);
        }
    }

    /// Spawn the daemon as a detached background process.
    ///
    /// Runs the same executable with the `daemon` subcommand. The child process
    /// inherits nothing (stdin/stdout/stderr all null), so it survives the
    /// adapter process exiting.
    fn spawn_daemon(&self) -> io::Result<()> {
        let exe = std::env::current_exe()?;
        info!("Spawning daemon: {} daemon", exe.display());

        let mut cmd = std::process::Command::new(&exe);
        cmd.arg("daemon")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        // Prevent a console window from flashing on Windows when the daemon
        // is spawned as a background process. Without this flag, Command::new
        // inherits the parent's console or creates a new one.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        cmd.spawn()?;

        Ok(())
    }

    /// Poll for the IPC socket file to appear, with exponential backoff.
    ///
    /// Steps: 50ms, 100ms, 200ms, 400ms, 500ms (capped), 500ms, ...
    /// Returns `Err` if the total timeout elapses before the socket appears.
    pub fn wait_for_socket(&self, timeout: Duration) -> io::Result<()> {
        let start = Instant::now();
        let mut delay = Duration::from_millis(50);
        let max_delay = Duration::from_millis(500);
        let ipc_addr = self.paths.daemon_ipc_addr();

        loop {
            // Probe the IPC endpoint to check if the daemon is listening
            #[cfg(unix)]
            if std::os::unix::net::UnixStream::connect(&ipc_addr).is_ok() {
                return Ok(());
            }

            // On Windows, use WaitNamedPipeW to check if the pipe exists without
            // connecting. OpenOptions::open() actually CONNECTS to the pipe,
            // consuming a pipe instance. If the daemon hasn't entered its accept
            // loop yet, this probe eats the only instance and the real connection
            // gets ERROR_PIPE_BUSY (231). WaitNamedPipeW checks existence without
            // consuming any instance.
            #[cfg(windows)]
            if win_pipe_exists(&ipc_addr) {
                return Ok(());
            }

            if start.elapsed() >= timeout {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "Daemon IPC endpoint did not appear within {}ms",
                        timeout.as_millis()
                    ),
                ));
            }

            std::thread::sleep(delay);
            delay = (delay * 2).min(max_delay);
        }
    }
}

/// Check if a Windows named pipe exists without connecting to it.
///
/// Uses `WaitNamedPipeW` with a 1ms timeout. This probes the pipe namespace
/// without consuming a pipe instance (unlike `OpenOptions::open`, which
/// actually connects and eats an instance).
///
/// Returns `true` if the pipe exists (regardless of whether instances are
/// currently available), `false` if the pipe hasn't been created yet.
#[cfg(windows)]
fn win_pipe_exists(pipe_path: &std::path::Path) -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    unsafe extern "system" {
        fn WaitNamedPipeW(lpNamedPipeName: *const u16, nTimeOut: u32) -> i32;
    }

    let wide: Vec<u16> = OsStr::new(pipe_path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // Timeout of 1ms: we don't want to wait, just check existence.
    let result = unsafe { WaitNamedPipeW(wide.as_ptr(), 1) };
    if result != 0 {
        // Pipe exists and has an available instance.
        return true;
    }

    // WaitNamedPipeW failed. Check why:
    // - ERROR_FILE_NOT_FOUND (2): pipe doesn't exist yet
    // - ERROR_SEM_TIMEOUT (121): pipe exists, all instances busy (still means daemon is up)
    let err = io::Error::last_os_error();
    err.raw_os_error() == Some(121)
}
