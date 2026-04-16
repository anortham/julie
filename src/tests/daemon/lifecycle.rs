use crate::daemon::lifecycle::{
    DaemonStatus, DisconnectLifecycleAction, IncomingSessionAction, LifecycleEvent, LifecyclePhase,
    RestartHandoffAction, RestartReason, ShutdownCause, check_status, restart_handoff_action,
    stale_binary_accept_action, stale_binary_disconnect_action, stop_daemon, transition,
    version_gate_action,
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
fn test_stale_binary_accept_action_marks_restart_pending_when_busy() {
    assert_eq!(
        stale_binary_accept_action(true, 2, false),
        IncomingSessionAction::AcceptWithRestartPending(RestartReason::StaleBinary)
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
