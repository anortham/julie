//! Tests for daemon state file operations.

#[cfg(test)]
mod tests {
    use crate::daemon::lifecycle::{
        LifecyclePhase, RestartPendingTransition, ShutdownCause, flag_restart_pending_for_restart,
        write_daemon_phase, write_daemon_state,
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
    fn test_write_daemon_phase_uses_lifecycle_state_value() {
        let dir = tempfile::TempDir::new().unwrap();
        let state_path = dir.path().join("daemon.state");
        write_daemon_phase(
            &state_path,
            LifecyclePhase::Draining {
                cause: ShutdownCause::RestartRequired,
            },
        );
        assert_eq!(std::fs::read_to_string(&state_path).unwrap(), "stopping");
    }

    /// Finding #1 regression: when the daemon rejects a version-mismatched
    /// session while other sessions are active, daemon_state must transition
    /// to "stopping" on the first rejection so the adapter's
    /// `ensure_daemon_ready` path waits for PID exit instead of burning
    /// its short retry budget against a daemon still advertising "ready".
    #[test]
    fn test_flag_restart_pending_after_version_reject_writes_stopping_on_first_call() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let dir = tempfile::TempDir::new().unwrap();
        let state_path = dir.path().join("daemon.state");
        // Pre-existing state: "ready" (as set by a running daemon).
        write_daemon_state(&state_path, "ready");

        let restart_pending = AtomicBool::new(false);
        let RestartPendingTransition {
            first_request,
            next_phase,
        } = flag_restart_pending_for_restart(
            &restart_pending,
            &state_path,
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
            "stopping",
            "daemon_state must transition to 'stopping' so adapters wait for PID exit"
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
}
