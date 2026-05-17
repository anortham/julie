//! Daemon launcher: auto-starts the daemon process if not already running.
//!
//! The launcher checks for a running daemon via PID file, acquires an advisory
//! lock to prevent races between multiple adapters, spawns the daemon as a
//! detached background process, and waits for the HTTP endpoint to become ready.

use std::io;
use std::time::{Duration, Instant};

use fs2::FileExt;
use tracing::{debug, info};

use crate::daemon::discovery::{DiscoveryFile, DiscoveryRecord, DiscoveryState};
use crate::daemon::http_transport::{MCP_PATH, READINESS_PATH};
use crate::daemon::pid::{PidFile, PidFileStatus};
use crate::daemon::transport::TransportEndpoint;
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
    /// stale PID files as a side effect. Treats `Indeterminate` (e.g.,
    /// fresh empty PID file from a racing daemon mid-write) as "present"
    /// so the poll loop does not declare BrokenPipe and spawn a duplicate.
    fn is_daemon_running(&self) -> bool {
        if matches!(
            DiscoveryFile::read_and_validate(&self.paths.discovery_file()),
            DiscoveryState::Live(_)
        ) {
            return true;
        }

        !matches!(
            PidFile::check_status(&self.paths.daemon_pid()),
            PidFileStatus::Dead
        )
    }

    pub fn transport_endpoint(&self) -> io::Result<TransportEndpoint> {
        match DiscoveryFile::read_and_validate(&self.paths.discovery_file()) {
            DiscoveryState::Live(record) => return discovery_endpoint(&record),
            DiscoveryState::Missing | DiscoveryState::Stale | DiscoveryState::Corrupt(_) => {}
        }

        if let Some(endpoint) = crate::daemon::legacy_migration::detect_and_attach(&self.paths) {
            return Ok(endpoint);
        }

        TransportEndpoint::read_discovery(&self.paths.daemon_mcp_transport())
    }

    /// Probe the daemon transport endpoint to check if it is accepting connections.
    /// Used when the state file is missing or unreadable.
    fn probe_transport_endpoint(&self) -> bool {
        self.transport_endpoint()
            .map(|endpoint| endpoint.probe_readiness().is_ready())
            .unwrap_or(false)
    }

    /// Assess the daemon's lifecycle phase from PID + state file.
    ///
    /// Cleans up stale files as a side effect when the daemon is dead.
    /// When PID is alive but the state file is missing or unreadable (old binary,
    /// write failure, permissions), falls back to probing the daemon transport endpoint.
    ///
    /// `Indeterminate` PID-file status (e.g., racing daemon mid-write of
    /// `create_exclusive`) maps to `Starting`: the caller should wait, not
    /// declare the daemon dead and spawn a replacement. This is the
    /// launcher half of the P2 fix for the 577-daemon cascade — pre-fix,
    /// an empty PID file fed back as `None` from `check_running` and the
    /// launcher unlinked the state file + spawned a new daemon.
    pub fn daemon_readiness(&self) -> DaemonReadiness {
        match DiscoveryFile::read_and_validate(&self.paths.discovery_file()) {
            DiscoveryState::Live(record) => {
                if matches!(record.phase.as_deref(), Some("stopping") | Some("draining")) {
                    return DaemonReadiness::Stopping;
                }

                return match discovery_endpoint(&record)
                    .map(|endpoint| endpoint.probe_readiness().is_ready())
                {
                    Ok(true) => DaemonReadiness::Ready,
                    Ok(false) | Err(_) => DaemonReadiness::Starting,
                };
            }
            DiscoveryState::Missing | DiscoveryState::Stale | DiscoveryState::Corrupt(_) => {}
        }

        match PidFile::check_status(&self.paths.daemon_pid()) {
            PidFileStatus::Dead => {
                let _ = std::fs::remove_file(self.paths.daemon_state());
                DaemonReadiness::Dead
            }
            PidFileStatus::Indeterminate => {
                // Fresh empty / unparseable PID file — a daemon is likely
                // mid-`create_exclusive`. Treat as starting; do NOT delete
                // the state file (it may already say "starting" or even
                // "ready" written by the in-flight daemon).
                DaemonReadiness::Starting
            }
            PidFileStatus::Alive(_pid) => {
                match std::fs::read_to_string(self.paths.daemon_state()) {
                    Ok(s) if s.trim() == "ready" => DaemonReadiness::Ready,
                    Ok(s) if s.trim() == "draining" => DaemonReadiness::Stopping,
                    Ok(s) if s.trim() == "stopping" => DaemonReadiness::Stopping,
                    _ => {
                        // State file missing or unreadable.
                        // Probe the HTTP transport: if the endpoint is
                        // reachable, the daemon is ready regardless of state file.
                        if self.probe_transport_endpoint() {
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
    /// daemon.state file to distinguish starting/ready/stopping.
    ///
    /// **Locking strategy (A1.8)**: holds `daemon.lock` ONLY across the
    /// "should I spawn?" decision and the spawn syscall itself, then drops
    /// the lock before polling for readiness. The lock cannot be held across
    /// the entire wait because the spawned daemon's legacy-migration gate
    /// (A1.5) probes `daemon.lock`, sees `AlreadyHeld`, and exits 2 —
    /// effectively making the spawn path unreachable. The kernel-held
    /// singleton lock (A1.2) inside the new daemon still prevents multiple
    /// daemons from running simultaneously, so the worst case of two
    /// adapters spawning concurrently is one of them losing the singleton
    /// race and exiting cleanly; the surviving daemon serves both adapters.
    pub fn ensure_daemon_ready(&self) -> io::Result<()> {
        // A1.5: legacy detection. If a legacy julie-server daemon is
        // already running for this JULIE_HOME, attach to its HTTP endpoint
        // instead of spawning a new daemon. This is the upgrade path: the
        // adapter keeps working against the old daemon until the operator
        // restarts it. The legacy daemon writes daemon.pid plus
        // daemon-mcp-transport.json; the new daemon writes discovery.json
        // (A1.3) which is picked up
        // by self.daemon_readiness() through the regular path below.
        if let Some(_legacy_endpoint) =
            crate::daemon::legacy_migration::detect_and_attach(&self.paths)
        {
            debug!(
                "Legacy julie-server daemon detected via daemon.pid + daemon-mcp-transport.json; attaching to legacy endpoint"
            );
            return Ok(());
        }

        // Fast path (no lock): if daemon is already ready, skip the lock.
        if matches!(self.daemon_readiness(), DaemonReadiness::Ready) {
            debug!("Daemon already ready (fast path)");
            return Ok(());
        }

        let deadline = Instant::now() + Duration::from_secs(60);
        self.wait_for_daemon_ready(deadline)
    }

    /// Acquire `daemon.lock` for the brief window of evaluating "should I
    /// spawn?" + the `spawn_daemon` syscall, then release. The kernel-held
    /// singleton lock inside the spawned daemon prevents duplicates beyond
    /// this point. See `ensure_daemon_ready` for the rationale on why the
    /// lock cannot wrap the readiness wait.
    fn spawn_under_lock(&self) -> io::Result<()> {
        // Ensure the julie home directory exists for the lock file.
        self.paths.ensure_dirs().map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to create daemon directories: {}", e),
            )
        })?;

        let lock_path = self.paths.daemon_startup_lock();
        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)?;

        debug!("Acquiring daemon startup lock: {}", lock_path.display());
        lock_file.lock_exclusive()?;

        // Re-check readiness under the lock: another adapter may have
        // spawned a daemon between our pre-lock check and acquiring the lock.
        let spawn_result = match self.daemon_readiness() {
            DaemonReadiness::Dead => self.spawn_daemon(),
            _ => Ok(()),
        };

        // Release the lock IMMEDIATELY after spawn (or skip). Holding it
        // longer would block the spawned daemon's legacy-migration gate
        // from succeeding (it probes daemon.lock and refuses on AlreadyHeld).
        lock_file.unlock()?;
        drop(lock_file);

        spawn_result
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
                    self.spawn_under_lock()?;
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

            match self.daemon_readiness() {
                DaemonReadiness::Ready => return Ok(()),
                DaemonReadiness::Stopping if target_state == "ready" => {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "Daemon transitioned to stopping before reaching ready",
                    ));
                }
                DaemonReadiness::Starting | DaemonReadiness::Dead | DaemonReadiness::Stopping => {}
            }

            // Check current state
            if let Ok(s) = std::fs::read_to_string(self.paths.daemon_state()) {
                let state = s.trim();
                if state == target_state {
                    return Ok(());
                }
                if target_state == "ready" && state == "draining" {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "Daemon transitioned to draining before reaching ready",
                    ));
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
    /// A1.8: invokes `julie-daemon start` (the new dedicated daemon binary)
    /// rather than re-execing the current binary as `julie-server daemon`.
    /// The child detaches from the adapter's process group so it survives
    /// adapter exit:
    ///
    ///   * POSIX: `setsid` via `pre_exec` puts the daemon in its own session,
    ///     immune to the adapter's controlling terminal closing.
    ///   * Windows: `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP` so the
    ///     daemon does not inherit the adapter's console and is unaffected
    ///     by Ctrl+C sent to the adapter.
    ///
    /// The child inherits nothing (stdin/stdout/stderr null) so its file
    /// descriptors cannot keep the adapter's stdio pipes open.
    fn spawn_daemon(&self) -> io::Result<()> {
        let daemon_exe = locate_julie_daemon()?;
        info!("Spawning daemon: {} start", daemon_exe.display());

        let mut cmd = std::process::Command::new(&daemon_exe);
        cmd.arg("start")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt as _;
            // Detach the child from the adapter's process group via setsid()
            // so it cannot be killed by a SIGHUP to the controlling terminal
            // and survives adapter exit.
            //
            // SAFETY: setsid is async-signal-safe (POSIX.1-2008). Calling it
            // in pre_exec — between fork() and exec() — is the standard recipe
            // for a daemonized child. We ignore EPERM (already a session
            // leader, e.g. test harness): the child is still detached enough
            // for our purposes (no controlling terminal inherited because
            // stdio is null), and propagating EPERM here would block daemon
            // spawn whenever the adapter inherits an unusual session layout.
            unsafe {
                cmd.pre_exec(|| {
                    if libc::setsid() == -1 {
                        let err = io::Error::last_os_error();
                        if err.raw_os_error() != Some(libc::EPERM) {
                            return Err(err);
                        }
                    }
                    Ok(())
                });
            }
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // DETACHED_PROCESS         (0x00000008): child has no console
            // CREATE_NEW_PROCESS_GROUP (0x00000200): immune to adapter Ctrl+C
            // CREATE_NO_WINDOW         (0x08000000): no console window flash
            const DETACHED_PROCESS: u32 = 0x00000008;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
        }

        cmd.spawn()?;

        Ok(())
    }
}

fn discovery_endpoint(record: &DiscoveryRecord) -> io::Result<TransportEndpoint> {
    TransportEndpoint::streamable_http(
        record.host.clone(),
        record.port,
        MCP_PATH,
        READINESS_PATH,
        Some(record.token_path.clone()),
    )
}

/// Locate the `julie-daemon` binary the adapter should spawn.
///
/// Search order:
///   1. Same directory as the running adapter's own executable. This is the
///      plugin's installed layout: both binaries ship inside
///      `<plugin>/bin/<arch>/`, so the daemon is the sibling file.
///   2. `PATH`, by handing a bare name to `Command::new` (the OS resolves it
///      at spawn time).
///
/// We surface a clear error if neither path works so operators don't have to
/// guess why their adapter cannot find the daemon binary.
fn locate_julie_daemon() -> io::Result<std::path::PathBuf> {
    let bin_name = if cfg!(windows) {
        "julie-daemon.exe"
    } else {
        "julie-daemon"
    };

    // (1) Sibling of the current adapter executable.
    if let Ok(adapter_exe) = std::env::current_exe() {
        if let Some(parent) = adapter_exe.parent() {
            let sibling = parent.join(bin_name);
            if sibling.is_file() {
                debug!(
                    "Resolved julie-daemon as adapter sibling: {}",
                    sibling.display()
                );
                return Ok(sibling);
            }
        }
    }

    // (2) PATH lookup: hand a bare name to Command::new and let the OS try.
    //     We verify it exists somewhere on PATH so the error message lands
    //     here rather than in a confusing spawn() failure with no context.
    if let Ok(path_env) = std::env::var("PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for entry in path_env.split(sep) {
            let candidate = std::path::Path::new(entry).join(bin_name);
            if candidate.is_file() {
                debug!("Resolved julie-daemon via PATH: {}", candidate.display());
                return Ok(candidate);
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "{} binary not found next to julie-adapter or on PATH; check installation",
            bin_name
        ),
    ))
}
