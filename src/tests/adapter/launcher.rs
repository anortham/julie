//! Tests for the adapter's DaemonLauncher (auto-start daemon, socket wait).

#[cfg(test)]
mod tests {
    use crate::adapter::launcher::DaemonLauncher;
    use crate::adapter::launcher::DaemonReadiness;
    use crate::daemon::pid::PidFile;
    use crate::paths::DaemonPaths;
    use std::fs;
    use std::time::Duration;

    #[test]
    fn test_daemon_paths_includes_state_file() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        let state_path = paths.daemon_state();
        assert_eq!(state_path, dir.path().join("daemon.state"));
    }

    #[test]
    fn test_daemon_not_running_when_no_pid_file() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Dead);
    }

    #[test]
    fn test_daemon_detected_as_running_with_valid_pid() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
        // No state file = Starting (PID alive but state unknown)
        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Starting);
    }

    #[test]
    fn test_stale_pid_detected_and_cleaned() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        fs::write(paths.daemon_pid(), "99999999\n").unwrap();
        let launcher = DaemonLauncher::new(paths.clone());
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Dead);
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

    /// D-H4: socket file exists but no daemon is listening (stale from a crash).
    /// wait_for_socket must attempt an actual connect, not just check file existence.
    #[cfg(unix)]
    #[test]
    fn test_wait_for_socket_rejects_stale_socket_file() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        let socket_path = paths.daemon_socket();

        // Bind a real listener (creates the socket file), then immediately drop it.
        // The socket file remains on disk but no one is listening — stale socket.
        {
            let _listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
            // drops here: fd closed, socket file remains
        }

        assert!(
            socket_path.exists(),
            "stale socket file should persist after listener drop"
        );

        let launcher = DaemonLauncher::new(paths);
        // With the old file-exists check: returns Ok immediately (false positive).
        // With the fixed connect attempt: times out and returns Err (correct).
        let result = launcher.wait_for_socket(Duration::from_millis(300));
        assert!(
            result.is_err(),
            "Should fail when socket file exists but no daemon is listening"
        );
    }

    #[test]
    fn test_readiness_dead_when_no_pid() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Dead);
    }

    #[test]
    fn test_readiness_dead_cleans_stale_state_file() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::write(paths.daemon_state(), "ready").unwrap();
        let launcher = DaemonLauncher::new(paths.clone());
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Dead);
        assert!(
            !paths.daemon_state().exists(),
            "stale state file should be cleaned up"
        );
    }

    #[test]
    fn test_readiness_ready_with_live_pid_and_ready_state() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
        fs::write(paths.daemon_state(), "ready").unwrap();
        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Ready);
    }

    #[test]
    fn test_readiness_starting_with_live_pid_and_starting_state() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
        fs::write(paths.daemon_state(), "starting").unwrap();
        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Starting);
    }

    #[test]
    fn test_readiness_starting_with_live_pid_and_no_state_file() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Starting);
    }

    /// IPC fallback: live PID + no state file + listening socket = Ready.
    /// Covers version-skew (old daemon binary without state file support) and
    /// write_daemon_state failures.
    #[cfg(unix)]
    #[test]
    fn test_readiness_ready_via_ipc_fallback_when_no_state_file() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();

        // Simulate a live daemon PID
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();

        // Create a real Unix listener on the daemon socket path (simulates a listening daemon)
        let socket_path = paths.daemon_socket();
        let _listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();

        // No state file at all, but IPC is reachable
        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Ready);
    }

    #[test]
    fn test_readiness_stopping_with_live_pid() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
        fs::write(paths.daemon_state(), "stopping").unwrap();
        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Stopping);
    }

    /// Draining means the daemon is still accepting sessions but will restart
    /// when all sessions disconnect. Adapters must treat this as Ready so they
    /// connect normally instead of waiting for PID exit (which deadlocks when
    /// an orphan session holds remaining > 0).
    #[test]
    fn test_readiness_ready_when_draining_with_live_pid() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
        fs::write(paths.daemon_state(), "draining").unwrap();
        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Ready);
    }

    #[test]
    fn test_readiness_dead_with_stale_pid() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        fs::write(paths.daemon_pid(), "99999999").unwrap();
        fs::write(paths.daemon_state(), "ready").unwrap();
        let launcher = DaemonLauncher::new(paths.clone());
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Dead);
        assert!(!paths.daemon_state().exists());
        assert!(!paths.daemon_pid().exists());
    }

    #[test]
    fn test_ensure_daemon_ready_returns_ok_when_ready() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();

        // Simulate a ready daemon: live PID + "ready" state
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
        fs::write(paths.daemon_state(), "ready").unwrap();

        let launcher = DaemonLauncher::new(paths);
        // Fast path: should return immediately
        let result = launcher.ensure_daemon_ready();
        assert!(result.is_ok());
    }

    #[test]
    fn test_ensure_daemon_ready_waits_for_starting_to_ready() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();

        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
        let state_path = paths.daemon_state();
        fs::write(&state_path, "starting").unwrap();

        // Spawn a thread that transitions to "ready" after 200ms
        let state_path_clone = state_path.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            fs::write(&state_path_clone, "ready").unwrap();
        });

        let launcher = DaemonLauncher::new(paths);
        let result = launcher.ensure_daemon_ready();
        handle.join().unwrap();
        assert!(result.is_ok());
    }

    /// When a daemon transitions from "starting" to "draining" (e.g. stale
    /// binary detected during startup), the adapter waiting for "ready" must
    /// treat "draining" as success since the daemon is accepting connections.
    #[test]
    fn test_ensure_daemon_ready_treats_draining_as_ready() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();

        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
        let state_path = paths.daemon_state();
        fs::write(&state_path, "starting").unwrap();

        let state_path_clone = state_path.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            fs::write(&state_path_clone, "draining").unwrap();
        });

        let launcher = DaemonLauncher::new(paths);
        let result = launcher.ensure_daemon_ready();
        handle.join().unwrap();
        assert!(result.is_ok());
    }
}
