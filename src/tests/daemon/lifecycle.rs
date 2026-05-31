use crate::daemon::discovery::{DiscoveryFile, DiscoveryRecord};
use crate::daemon::lifecycle::{
    DaemonLifecycleController, DaemonStatus, DisconnectLifecycleAction, IncomingSessionAction,
    LifecycleEvent, LifecyclePhase, RestartHandoffAction, RestartReason, ShutdownCause,
    check_status, restart_handoff_action, stale_binary_accept_action,
    stale_binary_disconnect_action, stop_daemon, transition, version_gate_action,
};
use crate::daemon::pid::PidFile;
use crate::paths::DaemonPaths;
#[cfg(unix)]
use libc;
use std::fs;

#[test]
fn test_status_reports_not_running_when_no_pid() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    assert_eq!(check_status(&paths), DaemonStatus::NotRunning);
}

#[test]
fn test_status_reports_running_with_valid_pid() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    let _pid = PidFile::create(&paths.daemon_pid()).unwrap();
    match check_status(&paths) {
        DaemonStatus::Running { pid } => assert_eq!(pid, std::process::id()),
        other => panic!("Expected Running, got {:?}", other),
    }
}

#[test]
fn test_status_reports_running_with_valid_discovery_without_pid_file() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    paths.ensure_dirs().unwrap();
    let record = DiscoveryRecord::for_current_process(
        "127.0.0.1",
        17892,
        paths.token_file(),
        paths.julie_home().join("daemon.log"),
    );
    DiscoveryFile::write_atomic(&paths.discovery_file(), &record).unwrap();

    match check_status(&paths) {
        DaemonStatus::Running { pid } => assert_eq!(pid, std::process::id()),
        other => panic!("Expected Running from discovery.json, got {:?}", other),
    }
}

#[test]
fn test_status_detects_stale_pid() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    std::fs::create_dir_all(dir.path()).unwrap();
    // PID 99999999 should not be alive on any reasonable system
    std::fs::write(paths.daemon_pid(), "99999999\n").unwrap();
    assert_eq!(check_status(&paths), DaemonStatus::NotRunning);
}

#[test]
fn test_stop_when_not_running_is_ok() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    assert!(stop_daemon(&paths).is_ok());
}

#[test]
fn test_stop_daemon_not_running() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    let result = stop_daemon(&paths);
    assert!(result.is_ok());
}

#[test]
fn test_stop_daemon_cleans_stale_pid() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    fs::create_dir_all(dir.path()).unwrap();
    // Write a bogus PID that's not alive
    fs::write(paths.daemon_pid(), "99999999").unwrap();
    fs::write(paths.daemon_state(), "ready").unwrap();

    let result = stop_daemon(&paths);
    assert!(result.is_ok());
    // Stale files should be cleaned
    assert!(!paths.daemon_pid().exists());
    assert!(!paths.daemon_state().exists());
}

#[cfg(unix)]
#[test]
fn test_stop_daemon_does_not_delete_files_while_process_alive() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    fs::create_dir_all(dir.path()).unwrap();

    // Ignore SIGTERM so the test process survives the signal
    unsafe {
        libc::signal(libc::SIGTERM, libc::SIG_IGN);
    }

    // Write current process PID (definitely alive)
    let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
    fs::write(paths.daemon_state(), "ready").unwrap();

    // stop_daemon will try to signal, the process won't die (it's us),
    // so after the timeout it should NOT delete the files.
    let result = stop_daemon(&paths);

    // Restore default SIGTERM handler
    unsafe {
        libc::signal(libc::SIGTERM, libc::SIG_DFL);
    }

    // The function should return an error since the process didn't exit
    assert!(result.is_err(), "Should error when daemon doesn't exit");

    // PID file should still exist (not deleted under a live process)
    assert!(
        paths.daemon_pid().exists(),
        "PID file should not be deleted while process is alive"
    );
}

#[test]
fn test_transition_startup_complete_enters_ready() {
    let phase = transition(LifecyclePhase::Starting, LifecycleEvent::StartupComplete);
    assert_eq!(phase, LifecyclePhase::Ready);
}

#[test]
fn test_transition_shutdown_with_active_sessions_enters_draining() {
    let phase = transition(
        LifecyclePhase::Ready,
        LifecycleEvent::ShutdownRequested {
            cause: ShutdownCause::RestartRequired,
            active_sessions: 2,
        },
    );

    assert_eq!(
        phase,
        LifecyclePhase::Draining {
            cause: ShutdownCause::RestartRequired,
        }
    );
}

#[test]
fn test_transition_draining_to_stopping_after_sessions_drain() {
    let phase = transition(
        LifecyclePhase::Draining {
            cause: ShutdownCause::RestartRequired,
        },
        LifecycleEvent::SessionsDrained,
    );

    assert_eq!(
        phase,
        LifecyclePhase::Stopping {
            cause: ShutdownCause::RestartRequired,
        }
    );
}

#[test]
fn test_version_gate_action_reports_lifecycle_restart_outcomes() {
    // Adapter NEWER than the daemon: the daemon binary is stale, so restart so
    // the newer adapter respawns a matching daemon (converges upward).
    assert_eq!(
        version_gate_action(Some("6.8.0"), "6.7.0", 0),
        IncomingSessionAction::ShutdownForRestart(RestartReason::VersionMismatch)
    );
    assert_eq!(
        version_gate_action(Some("6.8.0"), "6.7.0", 3),
        IncomingSessionAction::RejectForRestart(RestartReason::VersionMismatch)
    );
}

#[test]
fn test_version_gate_action_does_not_restart_for_older_adapter() {
    // Adapter OLDER than the daemon (the v7.12.1-adapter-vs-v7.12.2-daemon flap):
    // the daemon is already newer. Restarting would let the stale adapter
    // respawn an older daemon and the two would flap forever. The gate must
    // reject the stale adapter WITHOUT triggering a restart, regardless of how
    // many sessions are active.
    assert_eq!(
        version_gate_action(Some("7.12.1"), "7.12.2", 0),
        IncomingSessionAction::RejectStaleAdapter(RestartReason::VersionMismatch),
        "older adapter with no active sessions must not shut down the newer daemon"
    );
    assert_eq!(
        version_gate_action(Some("7.12.1"), "7.12.2", 3),
        IncomingSessionAction::RejectStaleAdapter(RestartReason::VersionMismatch),
        "older adapter must not flap a daemon serving live sessions"
    );
}

#[test]
fn test_version_gate_action_accepts_matching_and_missing_versions() {
    assert_eq!(
        version_gate_action(Some("7.12.2"), "7.12.2", 2),
        IncomingSessionAction::Accept
    );
    assert_eq!(
        version_gate_action(None, "7.12.2", 2),
        IncomingSessionAction::Accept,
        "an adapter that sends no version header is accepted (legacy/unknown)"
    );
}

#[test]
fn test_stale_binary_accept_action_marks_restart_pending_when_busy() {
    assert_eq!(
        stale_binary_accept_action(true, 2, false),
        IncomingSessionAction::AcceptWithRestartPending(RestartReason::StaleBinary)
    );
}

#[test]
fn test_stale_binary_accept_action_rejects_new_sessions_after_restart_pending() {
    assert_eq!(
        stale_binary_accept_action(true, 1, true),
        IncomingSessionAction::RejectForRestart(RestartReason::StaleBinary)
    );
}

#[test]
fn test_stale_binary_disconnect_action_marks_restart_pending_with_sessions_remaining() {
    assert_eq!(
        stale_binary_disconnect_action(true, false, 2),
        DisconnectLifecycleAction::MarkRestartPending(RestartReason::StaleBinary)
    );
}

#[test]
fn test_stale_binary_disconnect_action_triggers_shutdown_for_last_session() {
    assert_eq!(
        stale_binary_disconnect_action(true, true, 0),
        DisconnectLifecycleAction::TriggerShutdown(ShutdownCause::RestartRequired)
    );
}

#[test]
fn test_disconnect_action_triggers_shutdown_for_pending_version_restart_after_last_session() {
    assert_eq!(
        stale_binary_disconnect_action(false, true, 0),
        DisconnectLifecycleAction::TriggerShutdown(ShutdownCause::RestartRequired),
        "a version-mismatch restart pending latch must fire once the last session disconnects, even when the binary mtime is not stale"
    );
}

#[test]
fn test_restart_handoff_action_retries_before_last_attempt() {
    assert_eq!(
        restart_handoff_action(0, 2, RestartReason::TransportUnavailable),
        RestartHandoffAction::Retry {
            reason: RestartReason::TransportUnavailable,
        }
    );
}

#[test]
fn test_restart_handoff_action_exhausts_on_last_attempt() {
    assert_eq!(
        restart_handoff_action(2, 2, RestartReason::ImmediateDisconnect),
        RestartHandoffAction::Exhausted {
            reason: RestartReason::ImmediateDisconnect,
        }
    );
}

#[test]
fn test_controller_startup_complete_publishes_ready_state() {
    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("daemon.state");
    let controller = DaemonLifecycleController::new(state_path.clone());

    assert_eq!(controller.phase(), LifecyclePhase::Starting);
    assert_eq!(fs::read_to_string(&state_path).unwrap(), "starting");

    let phase = controller.startup_complete();

    assert_eq!(phase, LifecyclePhase::Ready);
    assert_eq!(controller.phase(), LifecyclePhase::Ready);
    assert_eq!(fs::read_to_string(&state_path).unwrap(), "ready");
}

#[test]
fn test_controller_restart_pending_is_idempotent_with_active_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("daemon.state");
    let controller = DaemonLifecycleController::new(state_path.clone());
    controller.startup_complete();

    let first = controller.mark_restart_pending(2, ShutdownCause::RestartRequired);
    let second = controller.mark_restart_pending(2, ShutdownCause::RestartRequired);

    assert!(first.first_request);
    assert!(!second.first_request);
    assert!(controller.restart_pending());
    assert_eq!(
        first.next_phase,
        LifecyclePhase::Draining {
            cause: ShutdownCause::RestartRequired,
        }
    );
    assert_eq!(second.next_phase, first.next_phase);
    assert_eq!(controller.phase(), first.next_phase);
    assert_eq!(fs::read_to_string(&state_path).unwrap(), "draining");
}

#[test]
fn test_controller_restart_pending_without_active_sessions_publishes_stopping() {
    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("daemon.state");
    let controller = DaemonLifecycleController::new(state_path.clone());
    controller.startup_complete();

    let transition = controller.mark_restart_pending(0, ShutdownCause::RestartRequired);

    assert!(transition.first_request);
    assert!(controller.restart_pending());
    assert_eq!(
        transition.next_phase,
        LifecyclePhase::Stopping {
            cause: ShutdownCause::RestartRequired,
        }
    );
    assert_eq!(controller.phase(), transition.next_phase);
    assert_eq!(fs::read_to_string(&state_path).unwrap(), "stopping");
}

#[test]
fn test_controller_restart_pending_preserves_existing_shutdown_phase() {
    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("daemon.state");
    let controller = DaemonLifecycleController::new(state_path.clone());
    controller.startup_complete();
    controller.request_shutdown(ShutdownCause::Signal, 0);

    let transition = controller.mark_restart_pending(2, ShutdownCause::RestartRequired);

    assert!(transition.first_request);
    assert!(controller.restart_pending());
    assert_eq!(
        transition.next_phase,
        LifecyclePhase::Stopping {
            cause: ShutdownCause::Signal,
        }
    );
    assert_eq!(controller.phase(), transition.next_phase);
    assert_eq!(fs::read_to_string(&state_path).unwrap(), "stopping");
}

#[tokio::test]
async fn test_mark_restart_pending_signals_listener_when_no_active_sessions() {
    use std::time::Duration;
    use tokio::time::timeout;

    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("daemon.state");
    let controller = DaemonLifecycleController::new(state_path);
    controller.startup_complete();

    // Register a waiter on the restart channel BEFORE triggering the transition.
    let restart_notify = controller.restart_notify();
    let waiter = tokio::spawn(async move {
        restart_notify.notified().await;
    });

    // Give the waiter a tick to actually arm `.notified()`.
    tokio::task::yield_now().await;

    // Arming with zero active sessions (e.g. a stale-binary connection arriving
    // while the daemon is otherwise idle — ShutdownForRestart) must immediately
    // wake the waiter via the restart channel: there is no live session to
    // force-abort, so it is safe to shut down for restart now.
    let transition = controller.mark_restart_pending(0, ShutdownCause::RestartRequired);
    assert!(transition.first_request);

    // Waiter must wake within 100ms; otherwise the restart channel has no signal.
    timeout(Duration::from_millis(100), waiter)
        .await
        .expect("restart_notify waiter did not wake within 100ms of mark_restart_pending")
        .expect("restart_notify waiter task panicked");
}

#[tokio::test]
async fn test_mark_restart_pending_with_active_sessions_never_signals() {
    use std::time::Duration;
    use tokio::time::timeout;

    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("daemon.state");
    let controller = DaemonLifecycleController::new(state_path);
    controller.startup_complete();

    // Arming restart_pending while a session is live must NOT signal the
    // restart channel, no matter how many times it is invoked. A signal here
    // would bridge into shutdown()'s 60s drain and force-abort the live
    // session. The signal is deferred until the last session disconnects
    // (active_sessions == 0), exercised by
    // `test_restart_channel_fires_only_after_last_session_disconnects`.
    let first = controller.mark_restart_pending(2, ShutdownCause::RestartRequired);
    assert!(first.first_request);

    let signaled_after_first = timeout(
        Duration::from_millis(50),
        controller.restart_notify().notified(),
    )
    .await
    .is_ok();
    assert!(
        !signaled_after_first,
        "mark_restart_pending signaled the restart channel while a session was still active"
    );

    // A second arm with sessions still active is idempotent and likewise silent.
    let second = controller.mark_restart_pending(2, ShutdownCause::RestartRequired);
    assert!(!second.first_request);

    let signaled_after_second = timeout(
        Duration::from_millis(50),
        controller.restart_notify().notified(),
    )
    .await
    .is_ok();
    assert!(
        !signaled_after_second,
        "a repeated active-session arm unexpectedly signaled the restart channel"
    );
}

#[tokio::test]
async fn test_restart_channel_fires_only_after_last_session_disconnects() {
    use std::time::Duration;
    use tokio::time::timeout;

    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("daemon.state");
    let controller = DaemonLifecycleController::new(state_path);
    controller.startup_complete();

    // Arm restart_pending while a session is still active (the stale-binary
    // auto-restart path with a live session). This MUST NOT signal the restart
    // channel: firing now bridges into shutdown()'s bounded 60s drain, which
    // waits for the live session to DISCONNECT — not merely for in-flight
    // requests to finish. Long-lived MCP sessions never disconnect on their own
    // during a daemon-initiated restart, so the drain times out and force-aborts
    // the session, writing a recovery marker. That is the storm we are fixing.
    let armed = controller.mark_restart_pending(1, ShutdownCause::RestartRequired);
    assert!(armed.first_request);
    assert!(controller.restart_pending());

    let signaled_early = timeout(
        Duration::from_millis(50),
        controller.restart_notify().notified(),
    )
    .await
    .is_ok();
    assert!(
        !signaled_early,
        "mark_restart_pending signaled the restart channel while a session was still active; \
         this triggers shutdown()'s 60s drain and force-aborts the live session"
    );

    // When the last session disconnects (active_sessions == 0) the channel MUST
    // fire — even though restart_pending was already latched above
    // (first_request == false). This is the documented "exit after the last
    // session disconnects" trigger.
    let final_notify = controller.restart_notify();
    let final_waiter = tokio::spawn(async move {
        final_notify.notified().await;
    });
    tokio::task::yield_now().await;

    let last = controller.mark_restart_pending(0, ShutdownCause::RestartRequired);
    assert!(
        !last.first_request,
        "restart_pending latch was already armed by the active-session call"
    );

    timeout(Duration::from_millis(100), final_waiter)
        .await
        .expect("restart channel must fire once the last active session disconnects")
        .expect("restart_notify waiter task panicked");
}

#[test]
fn test_controller_sessions_drained_transitions_to_stopping() {
    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("daemon.state");
    let controller = DaemonLifecycleController::new(state_path.clone());
    controller.startup_complete();
    controller.mark_restart_pending(1, ShutdownCause::RestartRequired);

    let phase = controller.sessions_drained();

    assert_eq!(
        phase,
        LifecyclePhase::Stopping {
            cause: ShutdownCause::RestartRequired,
        }
    );
    assert_eq!(controller.phase(), phase);
    assert_eq!(fs::read_to_string(&state_path).unwrap(), "stopping");
}
