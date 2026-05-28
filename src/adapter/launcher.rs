//! Daemon launcher: auto-starts the daemon process if not already running.
//!
//! The launcher checks for a running daemon via `discovery.json`, acquires an
//! advisory lock to prevent races between multiple adapters, spawns the daemon
//! as a detached background process, and waits for the HTTP endpoint to become
//! ready.

use std::io;
use std::time::{Duration, Instant};

use fs2::FileExt;
use tracing::{debug, info, warn};

use crate::daemon::discovery::{DiscoveryFile, DiscoveryRecord, DiscoveryState};
use crate::daemon::http_transport::{MCP_PATH, READINESS_PATH};
use crate::daemon::transport::TransportEndpoint;
use crate::paths::DaemonPaths;

/// Manages daemon lifecycle from the adapter's perspective: detect, launch, wait.
/// The daemon's current lifecycle phase, as seen by the adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonReadiness {
    /// `discovery.json` says running and the readiness endpoint responds.
    Ready,
    /// `discovery.json` says starting, or running but the endpoint is not ready.
    /// Daemon is initializing; wait, don't spawn a second one.
    Starting,
    /// `discovery.json` says stopping or draining.
    /// Daemon is shutting down; wait for exit, then spawn fresh.
    Stopping,
    /// No live `discovery.json`. Safe to spawn a new daemon.
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
    /// For the split daemon, `discovery.json` is the adapter-facing lifecycle
    /// source. Legacy PID/state files are handled only by the explicit legacy
    /// attach path in `ensure_daemon_ready`.
    fn is_daemon_running(&self) -> bool {
        matches!(
            DiscoveryFile::read_and_validate(&self.paths.discovery_file()),
            DiscoveryState::Live(_)
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

        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "no live daemon discovery.json and no live legacy daemon transport",
        ))
    }

    /// Assess the daemon's lifecycle phase from validated `discovery.json`.
    pub fn daemon_readiness(&self) -> DaemonReadiness {
        match DiscoveryFile::read_and_validate(&self.paths.discovery_file()) {
            DiscoveryState::Live(record) => {
                match record.phase.as_deref().unwrap_or("running") {
                    "stopping" | "draining" => DaemonReadiness::Stopping,
                    // `phase="starting"` is published immediately after lock
                    // acquisition and listener bind, before DB migrations and
                    // workspace backfill run inside `DaemonApp::new`. During
                    // that window the listener can already accept TCP
                    // connections even though HTTP routing is not ready.
                    "starting" => DaemonReadiness::Starting,
                    "running" => match discovery_endpoint(&record)
                        .map(|endpoint| endpoint.probe_readiness().is_ready())
                    {
                        Ok(true) => DaemonReadiness::Ready,
                        Ok(false) | Err(_) => DaemonReadiness::Starting,
                    },
                    phase => {
                        warn!(
                            phase,
                            "Unrecognized discovery.json phase; treating daemon as starting"
                        );
                        DaemonReadiness::Starting
                    }
                }
            }
            DiscoveryState::Missing | DiscoveryState::Stale | DiscoveryState::Corrupt(_) => {
                DaemonReadiness::Dead
            }
        }
    }

    /// Ensure the daemon is running and ready to accept connections.
    ///
    /// Discovery-aware: reads `discovery.json.phase` to distinguish
    /// starting/running/stopping.
    ///
    /// **Locking strategy**: serialization across concurrent adapter spawns
    /// runs on `daemon-startup.lock` (launcher-only; the daemon never opens
    /// it). The launcher holds that lock across the "should I spawn?"
    /// decision, the spawn syscall, AND a short wait for the spawned
    /// daemon to publish liveness (`discovery.json`), then
    /// releases. Subsequent adapters waiting on the lock then see
    /// `Starting`/`Ready` on their re-check and skip the spawn instead of
    /// cascading. The daemon-side singleton lock (`daemon.lock`,
    /// kernel-held) remains the ultimate guarantee that only one daemon
    /// runs at a time. See `spawn_under_startup_lock_with` for the wait
    /// rationale.
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

    /// Acquire `daemon-startup.lock`, re-check readiness, spawn if Dead,
    /// wait for the new daemon to publish liveness, then release. See
    /// `spawn_under_startup_lock_with` for the wait-under-lock rationale.
    ///
    /// Returns `true` if the spawn closure actually ran (re-check inside the
    /// lock confirmed Dead), `false` if it was skipped (re-check found
    /// Stopping/Starting/Ready — another daemon is already mid-lifecycle).
    /// Callers must inspect this flag instead of unconditionally polling for
    /// "ready" — see the bug fix in `wait_for_daemon_ready` for why.
    fn spawn_under_lock(&self) -> io::Result<bool> {
        self.spawn_under_startup_lock_with(|| self.spawn_daemon(), Duration::from_secs(5))
    }

    /// Test-friendly variant: acquire `daemon-startup.lock`, re-check
    /// readiness, invoke `spawn_fn` if Dead, wait until the spawned daemon
    /// publishes liveness (`discovery.json` appears) or
    /// `liveness_timeout` elapses, then release the lock.
    ///
    /// The wait-for-liveness window is what prevents the concurrent-adapter
    /// spawn cascade: without it, adapter A spawns a daemon then releases
    /// the lock before the child has written `discovery.json`, so adapter B
    /// acquires the lock, re-checks readiness, still sees `Dead`, and
    /// spawns ANOTHER daemon. The daemon-side singleton lock
    /// (`daemon.lock`) kills the losers silently, but each loser still
    /// burns a fork+exec and surfaces transient processes in `ps`. With
    /// the wait, adapter B sees `Starting` on its re-check and skips.
    ///
    /// **Why holding `daemon-startup.lock` across the wait is safe**: only
    /// the launcher contends on this file; the daemon contends on the
    /// separate `daemon.lock` (kernel singleton) and never opens
    /// `daemon-startup.lock`, so no deadlock with the just-spawned daemon
    /// is possible.
    ///
    /// Returns `Ok(true)` if `spawn_fn` actually ran (re-check confirmed
    /// Dead), `Ok(false)` if the re-check found a daemon already alive
    /// in some other phase (Stopping/Starting/Ready) and the spawn was
    /// skipped. The boolean lets `wait_for_daemon_ready` distinguish "I
    /// just started a daemon, poll for it to reach ready" from "someone
    /// else's daemon is here, re-evaluate state from the top" — without
    /// it the caller blindly polls for "ready" against a dying daemon
    /// that will never get there, burning the full drain window before
    /// `discovery.json` disappears and reports BrokenPipe (the 83s cold-start
    /// hang observed in adapter.log at 2026-05-17T19:40-19:42).
    pub(crate) fn spawn_under_startup_lock_with<F>(
        &self,
        spawn_fn: F,
        liveness_timeout: Duration,
    ) -> io::Result<bool>
    where
        F: FnOnce() -> io::Result<()>,
    {
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
        let (spawn_ran, spawn_result): (bool, io::Result<()>) = match self.daemon_readiness() {
            DaemonReadiness::Dead => {
                let result = spawn_fn();
                if result.is_ok() {
                    self.wait_for_spawned_liveness(liveness_timeout);
                }
                (true, result)
            }
            other => {
                debug!(
                    ?other,
                    "Daemon readiness re-check inside startup lock returned non-Dead; skipping spawn"
                );
                (false, Ok(()))
            }
        };

        lock_file.unlock()?;
        drop(lock_file);

        spawn_result.map(|_| spawn_ran)
    }

    /// Poll readiness until the spawned daemon leaves `Dead` (`discovery.json`
    /// appears) or the timeout expires. Best-effort: on
    /// timeout we release the lock anyway so the outer `wait_for_daemon_ready`
    /// loop can re-evaluate.
    fn wait_for_spawned_liveness(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        let mut delay = Duration::from_millis(10);
        let max_delay = Duration::from_millis(100);

        loop {
            if !matches!(self.daemon_readiness(), DaemonReadiness::Dead) {
                return;
            }

            if Instant::now() >= deadline {
                // Cascade protection failed: the spawned daemon did not
                // publish liveness in time, so other waiting adapters
                // will re-check Dead and may spawn duplicates that the
                // kernel singleton lock will then kill silently. Log
                // loud so operators notice repeated occurrences.
                warn!(
                    "Spawned daemon did not publish liveness within {:?}; \
                     releasing startup lock — concurrent adapters may spawn duplicates",
                    timeout
                );
                return;
            }

            std::thread::sleep(delay);
            delay = (delay * 2).min(max_delay);
        }
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
                    match self.poll_for_readiness_change(deadline) {
                        Ok(()) => {} // Will re-check in next loop iteration
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => {
                            // Phase became "stopping"; loop back to re-assess
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                }
                DaemonReadiness::Stopping => {
                    info!("Daemon is stopping, waiting for exit...");
                    self.wait_for_discovery_exit(deadline)?;
                    // Discovery gone; fall through to Dead on next iteration
                }
                DaemonReadiness::Dead => {
                    info!("Daemon not running, spawning...");
                    let spawn_ran = self.spawn_under_lock()?;
                    if !spawn_ran {
                        // Re-check inside the startup lock found a daemon
                        // that wasn't visible to our outer check — usually
                        // one mid-drain whose discovery.json
                        // flickered as Stale during shutdown. Polling for
                        // "ready" here would wait the full drain window
                        // because the dying daemon is heading to shutdown,
                        // not "ready" (observed in adapter.log at
                        // 2026-05-17T19:40-19:42: 83s hang). Sleep briefly
                        // to avoid a tight loop while state stabilizes,
                        // then let the outer match re-dispatch on the
                        // actual phase (Stopping → wait_for_discovery_exit,
                        // Starting/Ready → poll, Dead again → retry spawn).
                        info!(
                            "Spawn skipped under lock (another daemon is alive); \
                             re-evaluating readiness"
                        );
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
                    }
                    match self.poll_for_readiness_change(deadline) {
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

    /// Poll `discovery.json.phase` until the daemon is ready, exits, or starts stopping.
    fn poll_for_readiness_change(&self, deadline: Instant) -> io::Result<()> {
        let mut delay = Duration::from_millis(50);
        let max_delay = Duration::from_millis(500);

        loop {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Timed out waiting for daemon readiness",
                ));
            }

            std::thread::sleep(delay);
            delay = (delay * 2).min(max_delay);

            match self.daemon_readiness() {
                DaemonReadiness::Ready => return Ok(()),
                DaemonReadiness::Stopping => {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "Daemon transitioned to stopping before reaching ready",
                    ));
                }
                DaemonReadiness::Dead => {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "Daemon exited while waiting for readiness",
                    ));
                }
                DaemonReadiness::Starting => {}
            }
        }
    }

    /// Poll until the daemon's discovery file is gone or no longer live.
    fn wait_for_discovery_exit(&self, deadline: Instant) -> io::Result<()> {
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
