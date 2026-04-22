//! Tests for daemon state file operations.

#[cfg(test)]
mod tests {
    use crate::daemon::lifecycle::{
        LifecyclePhase, RestartPendingTransition, ShutdownCause, flag_restart_pending_for_restart,
        store_phase, write_daemon_phase, write_daemon_state,
    };

    #[test]
    fn test_write_daemon_state_creates_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let state_path = dir.path().join("daemon.state");
        write_daemon_state(&state_path, "starting");
        assert_eq!(std::fs::read_to_string(&state_path).unwrap(), "starting");
    }

    #[test]
    fn test_write_daemon_state_overwrites() {
        let dir = tempfile::TempDir::new().unwrap();
        let state_path = dir.path().join("daemon.state");
        write_daemon_state(&state_path, "starting");
        write_daemon_state(&state_path, "ready");
        assert_eq!(std::fs::read_to_string(&state_path).unwrap(), "ready");
    }

    #[test]
    fn test_write_daemon_state_stopping() {
        let dir = tempfile::TempDir::new().unwrap();
        let state_path = dir.path().join("daemon.state");
        write_daemon_state(&state_path, "stopping");
        assert_eq!(std::fs::read_to_string(&state_path).unwrap(), "stopping");
    }

    #[test]
    fn test_write_daemon_phase_draining_writes_draining() {
        let dir = tempfile::TempDir::new().unwrap();
        let state_path = dir.path().join("daemon.state");
        write_daemon_phase(
            &state_path,
            LifecyclePhase::Draining {
                cause: ShutdownCause::RestartRequired,
            },
        );
        assert_eq!(
            std::fs::read_to_string(&state_path).unwrap(),
            "draining",
            "Draining must write 'draining' so adapters can still connect"
        );
    }

    #[test]
    fn test_write_daemon_phase_stopping_writes_stopping() {
        let dir = tempfile::TempDir::new().unwrap();
        let state_path = dir.path().join("daemon.state");
        write_daemon_phase(
            &state_path,
            LifecyclePhase::Stopping {
                cause: ShutdownCause::RestartRequired,
            },
        );
        assert_eq!(std::fs::read_to_string(&state_path).unwrap(), "stopping");
    }

    /// When the daemon flags restart-pending with active sessions, the state
    /// file must say "draining" (not "stopping") so adapters can still connect.
    /// The daemon continues to accept sessions during Draining; only Stopping
    /// means "wait for PID exit." Writing "stopping" here caused a liveness
    /// deadlock: adapters waited for PID death, but the daemon wouldn't exit
    /// while sessions remained.
    #[test]
    fn test_flag_restart_pending_with_active_sessions_writes_draining() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let dir = tempfile::TempDir::new().unwrap();
        let state_path = dir.path().join("daemon.state");
        write_daemon_state(&state_path, "ready");

        let restart_pending = AtomicBool::new(false);
        let RestartPendingTransition {
            first_request,
            next_phase,
        } = flag_restart_pending_for_restart(
            &restart_pending,
            &state_path,
            LifecyclePhase::Ready,
            1,
            ShutdownCause::RestartRequired,
        );

        assert!(first_request, "first rejection must report as first-time");
        assert!(
            restart_pending.load(Ordering::Relaxed),
            "restart_pending must be set"
        );
        assert_eq!(
            next_phase,
            LifecyclePhase::Draining {
                cause: ShutdownCause::RestartRequired,
            }
        );
        assert_eq!(
            std::fs::read_to_string(&state_path).unwrap(),
            "draining",
            "daemon_state must say 'draining' so adapters can still connect"
        );
    }

    /// When no sessions are active, restart-pending should write "stopping"
    /// since the daemon will shut down immediately.
    #[test]
    fn test_flag_restart_pending_with_zero_sessions_writes_stopping() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let dir = tempfile::TempDir::new().unwrap();
        let state_path = dir.path().join("daemon.state");
        write_daemon_state(&state_path, "ready");

        let restart_pending = AtomicBool::new(false);
        let RestartPendingTransition {
            first_request,
            next_phase,
        } = flag_restart_pending_for_restart(
            &restart_pending,
            &state_path,
            LifecyclePhase::Ready,
            0,
            ShutdownCause::RestartRequired,
        );

        assert!(first_request);
        assert!(restart_pending.load(Ordering::Relaxed));
        assert_eq!(
            next_phase,
            LifecyclePhase::Stopping {
                cause: ShutdownCause::RestartRequired,
            }
        );
        assert_eq!(
            std::fs::read_to_string(&state_path).unwrap(),
            "stopping",
            "zero sessions means immediate shutdown; state must be 'stopping'"
        );
    }

    #[test]
    fn test_flag_restart_pending_after_version_reject_idempotent_on_subsequent_calls() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let dir = tempfile::TempDir::new().unwrap();
        let state_path = dir.path().join("daemon.state");
        write_daemon_state(&state_path, "ready");

        let restart_pending = AtomicBool::new(false);
        let _ = flag_restart_pending_for_restart(
            &restart_pending,
            &state_path,
            LifecyclePhase::Ready,
            1,
            ShutdownCause::RestartRequired,
        );

        // Simulate a third-party writer overwriting the state after we set it.
        // This mirrors what happens if the shutdown path later writes "stopping"
        // again — the second call must not corrupt state back to an earlier
        // value. Write a DIFFERENT value here to prove the second call is a
        // no-op, not an overwrite.
        write_daemon_state(&state_path, "custom-marker");
        let RestartPendingTransition { first_request, .. } = flag_restart_pending_for_restart(
            &restart_pending,
            &state_path,
            LifecyclePhase::Ready,
            1,
            ShutdownCause::RestartRequired,
        );

        assert!(
            !first_request,
            "subsequent rejection must NOT report as first-time"
        );
        assert!(restart_pending.load(Ordering::Relaxed));
        assert_eq!(
            std::fs::read_to_string(&state_path).unwrap(),
            "custom-marker",
            "subsequent rejections must not re-write daemon_state"
        );
    }

    #[test]
    fn test_flag_restart_pending_preserves_existing_shutdown_phase() {
        use std::sync::RwLock;
        use std::sync::atomic::AtomicBool;

        let dir = tempfile::TempDir::new().unwrap();
        let state_path = dir.path().join("daemon.state");
        write_daemon_state(&state_path, "stopping");

        let daemon_phase = RwLock::new(LifecyclePhase::Stopping {
            cause: ShutdownCause::Signal,
        });
        let restart_pending = AtomicBool::new(true);

        let transition = flag_restart_pending_for_restart(
            &restart_pending,
            &state_path,
            *daemon_phase.read().unwrap(),
            2,
            ShutdownCause::RestartRequired,
        );
        store_phase(&daemon_phase, transition.next_phase);

        assert_eq!(
            *daemon_phase.read().unwrap(),
            LifecyclePhase::Stopping {
                cause: ShutdownCause::Signal,
            },
            "follow-on restart requests must not clobber a richer shutdown phase"
        );
    }
}
