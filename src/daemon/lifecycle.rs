//! Daemon lifecycle management: process status, state transitions, and restart decisions.

use crate::daemon::pid::PidFile;
use crate::paths::DaemonPaths;
#[cfg(unix)]
use libc;
use serde::Serialize;
use std::path::Path;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};
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
            Self::Draining { .. } | Self::Stopping { .. } => "stopping",
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

/// Apply a lifecycle event to the current daemon phase.
pub fn transition(current: LifecyclePhase, event: LifecycleEvent) -> LifecyclePhase {
    match event {
        LifecycleEvent::StartupComplete => LifecyclePhase::Ready,
        LifecycleEvent::ShutdownRequested {
            cause,
            active_sessions,
        } => {
            if active_sessions > 0 {
                LifecyclePhase::Draining { cause }
            } else {
                LifecyclePhase::Stopping { cause }
            }
        }
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
        IncomingSessionAction::Accept
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
pub fn write_daemon_state(path: &Path, state: &str) {
    if let Err(e) = std::fs::write(path, state) {
        warn!("Failed to write daemon state '{}': {}", state, e);
    }
}

/// Write a lifecycle phase through the state-file mapping.
pub fn write_daemon_phase(path: &Path, phase: LifecyclePhase) {
    write_daemon_state(path, phase.state_file_value());
}

pub fn store_phase(target: &RwLock<LifecyclePhase>, phase: LifecyclePhase) {
    *target.write().unwrap_or_else(|p| p.into_inner()) = phase;
}

pub fn publish_phase(target: &RwLock<LifecyclePhase>, path: &Path, phase: LifecyclePhase) {
    store_phase(target, phase);
    write_daemon_phase(path, phase);
}

/// Record a restart request and transition the daemon into draining or stopping.
pub fn flag_restart_pending_for_restart(
    restart_pending: &AtomicBool,
    daemon_state_path: &Path,
    active_sessions: usize,
    cause: ShutdownCause,
) -> RestartPendingTransition {
    let first_request = !restart_pending.load(Ordering::Relaxed);
    restart_pending.store(true, Ordering::Relaxed);
    let next_phase = transition(
        LifecyclePhase::Ready,
        LifecycleEvent::ShutdownRequested {
            cause,
            active_sessions,
        },
    );
    if first_request {
        write_daemon_phase(daemon_state_path, next_phase);
    }
    RestartPendingTransition {
        first_request,
        next_phase,
    }
}

/// Check whether the daemon is currently running by inspecting the PID file.
pub fn check_status(paths: &DaemonPaths) -> DaemonStatus {
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
    match PidFile::check_running(&paths.daemon_pid()) {
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

            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                if !PidFile::is_process_alive(pid) {
                    let _ = std::fs::remove_file(paths.daemon_pid());
                    let _ = std::fs::remove_file(paths.daemon_state());
                    #[cfg(unix)]
                    let _ = std::fs::remove_file(paths.daemon_socket());
                    info!("Daemon stopped");
                    return Ok(());
                }
                if std::time::Instant::now() >= deadline {
                    anyhow::bail!(
                        "Daemon did not stop within 10s (PID {}). \
                         Use `kill {}` to force.",
                        pid,
                        pid
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        None => {
            let _ = std::fs::remove_file(paths.daemon_pid());
            let _ = std::fs::remove_file(paths.daemon_state());
            #[cfg(unix)]
            let _ = std::fs::remove_file(paths.daemon_socket());
            info!("Daemon is not running (cleaned stale files if any)");
            Ok(())
        }
    }
}
