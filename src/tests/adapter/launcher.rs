//! Tests for the adapter's DaemonLauncher (auto-start daemon, socket wait).

#[cfg(test)]
mod tests {
    use crate::adapter::launcher::DaemonLauncher;
    use crate::daemon::pid::PidFile;
    use crate::paths::DaemonPaths;
    use std::fs;
    use std::time::Duration;

    #[test]
    fn test_daemon_not_running_when_no_pid_file() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        let launcher = DaemonLauncher::new(paths);
        assert!(!launcher.is_daemon_running());
    }

    #[test]
    fn test_daemon_detected_as_running_with_valid_pid() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        // Write current process PID (definitely alive)
        fs::create_dir_all(dir.path()).unwrap();
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
        let launcher = DaemonLauncher::new(paths);
        assert!(launcher.is_daemon_running());
    }

    #[test]
    fn test_stale_pid_detected_and_cleaned() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        // Write a bogus PID that doesn't correspond to any running process
        fs::write(paths.daemon_pid(), "99999999\n").unwrap();
        let launcher = DaemonLauncher::new(paths.clone());
        assert!(!launcher.is_daemon_running());
        // Stale PID file should have been cleaned up
        assert!(!paths.daemon_pid().exists());
    }

    #[cfg(unix)]
    #[test]
    fn test_wait_for_socket_returns_ok_when_socket_exists() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        let socket_path = paths.daemon_socket();

        // Create a real Unix listener to produce a socket file
        let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        let _keep = listener; // keep alive

        let launcher = DaemonLauncher::new(paths);
        let result = launcher.wait_for_socket(Duration::from_millis(200));
        assert!(result.is_ok(), "Should succeed when socket file exists");
    }

    #[cfg(unix)]
    #[test]
    fn test_wait_for_socket_times_out_when_no_socket() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());

        let launcher = DaemonLauncher::new(paths);
        let result = launcher.wait_for_socket(Duration::from_millis(200));
        assert!(
            result.is_err(),
            "Should fail when socket file never appears"
        );
    }

    #[test]
    fn test_launcher_uses_correct_paths() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        let launcher = DaemonLauncher::new(paths.clone());
        // Verify the launcher's paths match what we gave it
        assert_eq!(launcher.paths().julie_home(), paths.julie_home());
    }
}
