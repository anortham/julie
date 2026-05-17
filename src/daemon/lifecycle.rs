//! Daemon lifecycle management: process status, state transitions, and restart decisions.

use crate::daemon::discovery::{DiscoveryFile, DiscoveryState};
use crate::daemon::pid::PidFile;
use crate::paths::DaemonPaths;
#[cfg(unix)]
use libc;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tokio::sync::Notify;
use tracing::{info, warn};

/// Current state of the Julie daemon process.
#[derive(Debug, PartialEq)]
pub enum DaemonStatus {
    Running { pid: u32 },
    NotRunning,
}

/// Coarse daemon runtime phase used by the control plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecyclePhase {
    Starting,
    Ready,
    Draining { cause: ShutdownCause },
    Stopping { cause: ShutdownCause },
}

impl LifecyclePhase {
    pub fn kind(self) -> LifecyclePhaseKind {
        match self {
            Self::Starting => LifecyclePhaseKind::Starting,
            Self::Ready => LifecyclePhaseKind::Ready,
            Self::Draining { .. } => LifecyclePhaseKind::Draining,
            Self::Stopping { .. } => LifecyclePhaseKind::Stopping,
        }
    }

    pub fn shutdown_cause(self) -> Option<ShutdownCause> {
        match self {
            Self::Starting | Self::Ready => None,
            Self::Draining { cause } | Self::Stopping { cause } => Some(cause),
        }
    }

    pub fn state_file_value(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Draining { .. } => "draining",
            Self::Stopping { .. } => "stopping",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecyclePhaseKind {
    Starting,
    Ready,
    Draining,
    Stopping,
}

impl LifecyclePhaseKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Starting => "STARTING",
            Self::Ready => "READY",
            Self::Draining => "DRAINING",
            Self::Stopping => "STOPPING",
        }
    }
}

/// High-level shutdown cause. Specific restart reasons stay on accept-loop decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShutdownCause {
    Signal,
    StopCommand,
    RestartRequired,
}

impl ShutdownCause {
    pub fn label(self) -> &'static str {
        match self {
            Self::Signal => "SIGNAL",
            Self::StopCommand => "STOP COMMAND",
            Self::RestartRequired => "RESTART REQUIRED",
        }
    }
}

/// Specific reason a restart is required while serving sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartReason {
    StaleBinary,
    VersionMismatch,
    TransportUnavailable,
    ImmediateDisconnect,
}

/// Adapter-facing restart handoff decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartHandoffAction {
    Retry { reason: RestartReason },
    Exhausted { reason: RestartReason },
}

/// Events that advance the daemon lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEvent {
    StartupComplete,
    ShutdownRequested {
        cause: ShutdownCause,
        active_sessions: usize,
    },
    SessionsDrained,
}

/// Session-level action for accept-loop lifecycle decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncomingSessionAction {
    Accept,
    AcceptWithRestartPending(RestartReason),
    RejectForRestart(RestartReason),
    ShutdownForRestart(RestartReason),
}

/// Disconnect-time action when stale-binary checks run after a session ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisconnectLifecycleAction {
    None,
    MarkRestartPending(RestartReason),
    TriggerShutdown(ShutdownCause),
}

/// Result of flipping `restart_pending`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestartPendingTransition {
    pub first_request: bool,
    pub next_phase: LifecyclePhase,
}

#[derive(Debug, Clone)]
pub struct DaemonLifecycleController {
    phase: Arc<RwLock<LifecyclePhase>>,
    /// One-way bit; the only legitimate clear is process exit. The first call
    /// to `mark_restart_pending` signals the restart channel, which the
    /// listener in `DaemonApp::serve` bridges into the SIGTERM exit path.
    restart_pending: Arc<AtomicBool>,
    restart_notify: Arc<Notify>,
    state_path: Arc<PathBuf>,
}

impl DaemonLifecycleController {
    pub fn new(state_path: PathBuf) -> Self {
        let controller = Self {
            phase: Arc::new(RwLock::new(LifecyclePhase::Starting)),
            restart_pending: Arc::new(AtomicBool::new(false)),
            restart_notify: Arc::new(Notify::new()),
            state_path: Arc::new(state_path),
        };
        controller.publish(LifecyclePhase::Starting);
        controller
    }

    pub fn phase_handle(&self) -> Arc<RwLock<LifecyclePhase>> {
        Arc::clone(&self.phase)
    }

    pub fn restart_pending_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.restart_pending)
    }

    pub fn restart_notify(&self) -> Arc<Notify> {
        Arc::clone(&self.restart_notify)
    }

    pub fn phase(&self) -> LifecyclePhase {
        *self.phase.read().unwrap_or_else(|p| p.into_inner())
    }

    pub fn restart_pending(&self) -> bool {
        self.restart_pending.load(Ordering::Relaxed)
    }

    pub fn startup_complete(&self) -> LifecyclePhase {
        self.apply_event(LifecycleEvent::StartupComplete)
    }

    pub fn request_shutdown(&self, cause: ShutdownCause, active_sessions: usize) -> LifecyclePhase {
        self.apply_event(LifecycleEvent::ShutdownRequested {
            cause,
            active_sessions,
        })
    }

    pub fn sessions_drained(&self) -> LifecyclePhase {
        self.apply_event(LifecycleEvent::SessionsDrained)
    }

    pub fn mark_restart_pending(
        &self,
        active_sessions: usize,
        cause: ShutdownCause,
    ) -> RestartPendingTransition {
        let first_request = !self.restart_pending.swap(true, Ordering::Relaxed);
        let next_phase = self.request_shutdown(cause, active_sessions);
        if first_request {
            // First transition commits to shutdown. The listener wired in
            // DaemonApp::serve bridges this signal into the SIGTERM exit path,
            // which runs the 60s drain and full LIFO teardown. Gating on
            // first_request matches the existing flag semantics and avoids
            // spurious permits if the listener task is restarted by a future
            // refactor. Notify::notify_one would coalesce anyway.
            self.notify_restart();
        }
        RestartPendingTransition {
            first_request,
            next_phase,
        }
    }

    pub fn notify_restart(&self) {
        self.restart_notify.notify_one();
    }

    fn apply_event(&self, event: LifecycleEvent) -> LifecyclePhase {
        let next_phase = transition(self.phase(), event);
        self.publish(next_phase);
        next_phase
    }

    fn publish(&self, phase: LifecyclePhase) {
        publish_phase(self.phase.as_ref(), &self.state_path, phase);
    }
}

/// Apply a lifecycle event to the current daemon phase.
pub fn transition(current: LifecyclePhase, event: LifecycleEvent) -> LifecyclePhase {
    match event {
        LifecycleEvent::StartupComplete => LifecyclePhase::Ready,
        LifecycleEvent::ShutdownRequested {
            cause,
            active_sessions,
        } => match current {
            LifecyclePhase::Draining { .. } | LifecyclePhase::Stopping { .. } => current,
            LifecyclePhase::Starting | LifecyclePhase::Ready => {
                if active_sessions > 0 {
                    LifecyclePhase::Draining { cause }
                } else {
                    LifecyclePhase::Stopping { cause }
                }
            }
        },
        LifecycleEvent::SessionsDrained => match current {
            LifecyclePhase::Draining { cause } => LifecyclePhase::Stopping { cause },
            other => other,
        },
    }
}

/// Decide how the version gate should affect the lifecycle.
pub fn version_gate_action(
    adapter_version: Option<&str>,
    daemon_version: &str,
    active_sessions: usize,
) -> IncomingSessionAction {
    let Some(adapter_version) = adapter_version else {
        return IncomingSessionAction::Accept;
    };

    if adapter_version == daemon_version {
        return IncomingSessionAction::Accept;
    }

    if active_sessions == 0 {
        IncomingSessionAction::ShutdownForRestart(RestartReason::VersionMismatch)
    } else {
        IncomingSessionAction::RejectForRestart(RestartReason::VersionMismatch)
    }
}

/// Decide what stale-binary detection should do before accepting a session.
pub fn stale_binary_accept_action(
    binary_is_stale: bool,
    active_sessions: usize,
    restart_pending: bool,
) -> IncomingSessionAction {
    if !binary_is_stale {
        IncomingSessionAction::Accept
    } else if active_sessions == 0 {
        IncomingSessionAction::ShutdownForRestart(RestartReason::StaleBinary)
    } else if restart_pending {
        IncomingSessionAction::RejectForRestart(RestartReason::StaleBinary)
    } else {
        IncomingSessionAction::AcceptWithRestartPending(RestartReason::StaleBinary)
    }
}

/// Decide what stale-binary detection should do after a session disconnects.
pub fn stale_binary_disconnect_action(
    binary_is_stale: bool,
    restart_pending: bool,
    remaining_sessions: usize,
) -> DisconnectLifecycleAction {
    if !binary_is_stale {
        DisconnectLifecycleAction::None
    } else if remaining_sessions == 0 {
        DisconnectLifecycleAction::TriggerShutdown(ShutdownCause::RestartRequired)
    } else if restart_pending {
        DisconnectLifecycleAction::None
    } else {
        DisconnectLifecycleAction::MarkRestartPending(RestartReason::StaleBinary)
    }
}

/// Decide whether the adapter should retry a restart handoff or stop.
pub fn restart_handoff_action(
    attempt: u32,
    max_retries: u32,
    reason: RestartReason,
) -> RestartHandoffAction {
    if attempt < max_retries {
        RestartHandoffAction::Retry { reason }
    } else {
        RestartHandoffAction::Exhausted { reason }
    }
}

/// Write the daemon lifecycle state to the state file.
///
/// Best effort: failure to write is advisory and must not crash the daemon.
///
/// Uses a write-to-temp + atomic-rename pattern so that concurrent readers
/// never observe an empty or partial state string.  On Windows,
/// `std::fs::rename` maps to `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`, which
/// is atomic for same-filesystem paths.
pub(crate) fn write_daemon_state(path: &Path, state: &str) {
    let tmp = path.with_extension("state.tmp");
    if let Err(e) = std::fs::write(&tmp, state) {
        warn!("Failed to write daemon state tmp file '{}': {}", state, e);
        return;
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        warn!(
            "Failed to atomically replace daemon state '{}': {}",
            state, e
        );
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Write a lifecycle phase through the state-file mapping.
pub(crate) fn write_daemon_phase(path: &Path, phase: LifecyclePhase) {
    write_daemon_state(path, phase.state_file_value());
}

fn publish_phase(target: &RwLock<LifecyclePhase>, path: &Path, phase: LifecyclePhase) {
    *target.write().unwrap_or_else(|p| p.into_inner()) = phase;
    write_daemon_phase(path, phase);
}

/// Check whether the daemon is currently running by inspecting the PID file.
pub fn check_status(paths: &DaemonPaths) -> DaemonStatus {
    if let DiscoveryState::Live(record) = DiscoveryFile::read_and_validate(&paths.discovery_file())
    {
        return DaemonStatus::Running { pid: record.pid };
    }

    match PidFile::check_running(&paths.daemon_pid()) {
        Some(pid) => DaemonStatus::Running { pid },
        None => DaemonStatus::NotRunning,
    }
}

/// Stop the daemon process if it is running.
///
/// On Unix, sends SIGTERM for graceful shutdown. On Windows, signals a named
/// event that the daemon waits on, falling back to `taskkill /F` for older
/// daemons that predate the event mechanism.
pub fn stop_daemon(paths: &DaemonPaths) -> anyhow::Result<()> {
    let discovery_pid = match DiscoveryFile::read_and_validate(&paths.discovery_file()) {
        DiscoveryState::Live(record) => Some(record.pid),
        DiscoveryState::Missing | DiscoveryState::Stale | DiscoveryState::Corrupt(_) => None,
    };

    match discovery_pid.or_else(|| PidFile::check_running(&paths.daemon_pid())) {
        Some(pid) => {
            info!("Sending shutdown signal to daemon PID {}", pid);

            #[cfg(unix)]
            {
                let ret = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
                if ret != 0 {
                    anyhow::bail!("Failed to send SIGTERM to PID {}", pid);
                }
            }

            #[cfg(windows)]
            {
                use super::shutdown_event;

                let event_name = paths.daemon_shutdown_event();
                let signaled = shutdown_event::signal_shutdown(&event_name).unwrap_or(false);
                if signaled {
                    info!("Signaled shutdown event: {}", event_name);
                } else {
                    info!("Shutdown event not found, falling back to taskkill /F");
                    let _ = std::process::Command::new("taskkill")
                        .args(["/F", "/T", "/PID", &pid.to_string()])
                        .output();
                }
            }

            let timeout = super::drain_timeout();
            let deadline = std::time::Instant::now() + timeout;
            loop {
                if !PidFile::is_process_alive(pid) {
                    remove_lifecycle_files(paths);
                    info!("Daemon stopped");
                    return Ok(());
                }
                if std::time::Instant::now() >= deadline {
                    anyhow::bail!(
                        "Daemon did not stop within {}s (PID {}). \
                         Use `kill {}` to force.",
                        timeout.as_secs(),
                        pid,
                        pid
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        None => {
            remove_lifecycle_files(paths);
            info!("Daemon is not running (cleaned stale files if any)");
            Ok(())
        }
    }
}

fn remove_lifecycle_files(paths: &DaemonPaths) {
    let _ = std::fs::remove_file(paths.daemon_pid());
    let _ = std::fs::remove_file(paths.daemon_state());
    let _ = std::fs::remove_file(paths.discovery_file());
    let _ = std::fs::remove_file(paths.token_file());
    let _ = std::fs::remove_file(paths.daemon_mcp_transport());
}
