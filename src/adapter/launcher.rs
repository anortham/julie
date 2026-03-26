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
    pub fn is_daemon_running(&self) -> bool {
        PidFile::check_running(&self.paths.daemon_pid()).is_some()
    }

    /// Ensure the daemon is running, launching it if necessary.
    ///
    /// Uses an advisory file lock (`daemon.lock`) to serialize startup across
    /// multiple concurrent adapters. The sequence:
    /// 1. Acquire exclusive lock on `daemon.lock`
    /// 2. Double-check PID (another adapter may have started it while we waited)
    /// 3. Spawn daemon process if still not running
    /// 4. Release lock
    /// 5. Wait for socket to appear
    pub fn ensure_daemon_running(&self) -> io::Result<()> {
        // Fast path: already running
        if self.is_daemon_running() {
            debug!("Daemon already running (fast path)");
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

        // Double-check after acquiring lock (another adapter may have started it)
        let need_spawn = !self.is_daemon_running();

        if need_spawn {
            info!("Daemon not running, spawning...");
            self.spawn_daemon()?;
        } else {
            debug!("Daemon already running (detected after lock acquisition)");
        }

        // Release lock before waiting for socket
        lock_file.unlock()?;
        drop(lock_file);

        if need_spawn {
            // Wait for the socket to appear (daemon needs time to bind)
            // Timeout must exceed the daemon's cold-start time, which includes
            // ORT embedding model loading (can take 25-30s on Windows with DirectML).
            self.wait_for_socket(Duration::from_secs(45))?;
            info!("Daemon socket ready");
        }

        Ok(())
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
