//! Tests for daemon state file operations.

#[cfg(test)]
mod tests {
    use crate::daemon::lifecycle::{
        LifecyclePhase, ShutdownCause, write_daemon_phase, write_daemon_state,
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
}
