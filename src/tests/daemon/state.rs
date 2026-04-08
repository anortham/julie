//! Tests for daemon state file operations.

#[cfg(test)]
mod tests {
    use crate::daemon::write_daemon_state;

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
}
