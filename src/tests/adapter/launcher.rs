//! Tests for the adapter's DaemonLauncher (auto-start daemon, HTTP readiness).

#[cfg(test)]
mod tests {
    use crate::adapter::launcher::DaemonLauncher;
    use crate::adapter::launcher::DaemonReadiness;
    use crate::daemon::pid::PidFile;
    use crate::daemon::transport::TransportEndpoint;
    use crate::paths::DaemonPaths;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread::{self, JoinHandle};
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

    #[test]
    fn test_launcher_uses_correct_paths() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        let launcher = DaemonLauncher::new(paths.clone());
        // Verify the launcher's paths match what we gave it
        assert_eq!(launcher.paths().julie_home(), paths.julie_home());
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

    fn spawn_http_readiness_server(listener: TcpListener) -> JoinHandle<()> {
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            loop {
                let mut chunk = [0u8; 256];
                let n = stream.read(&mut chunk).unwrap();
                assert_ne!(n, 0, "client closed before sending full HTTP request");
                request.extend_from_slice(&chunk[..n]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8_lossy(&request);
            assert!(request.starts_with("GET /mcp/ready HTTP/1.1"));
            stream
                .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        })
    }

    #[test]
    fn test_readiness_ready_via_http_discovery_when_no_state_file() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = spawn_http_readiness_server(listener);
        let endpoint =
            TransportEndpoint::streamable_http("127.0.0.1", port, "/mcp", "/mcp/ready", None)
                .unwrap();
        endpoint
            .publish_discovery(&paths.daemon_mcp_transport())
            .unwrap();

        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Ready);
        server.join().unwrap();
    }

    #[test]
    fn test_readiness_starting_when_http_discovery_is_stale() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let endpoint =
            TransportEndpoint::streamable_http("127.0.0.1", port, "/mcp", "/mcp/ready", None)
                .unwrap();
        endpoint
            .publish_discovery(&paths.daemon_mcp_transport())
            .unwrap();

        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Starting);
    }

    #[cfg(unix)]
    #[test]
    fn test_readiness_starting_when_http_discovery_is_stale_even_if_legacy_socket_exists() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let endpoint =
            TransportEndpoint::streamable_http("127.0.0.1", port, "/mcp", "/mcp/ready", None)
                .unwrap();
        endpoint
            .publish_discovery(&paths.daemon_mcp_transport())
            .unwrap();

        let socket_path = dir.path().join("legacy-daemon.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();

        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Starting);
    }

    /// HTTP-only readiness: live PID + no state file + legacy listening socket
    /// stays Starting. Version-skew must not silently fall back to legacy transport.
    #[cfg(unix)]
    #[test]
    fn test_readiness_starting_when_no_state_file_even_if_legacy_socket_exists() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();

        // Simulate a live daemon PID
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();

        // Create a real Unix listener at the old daemon socket path shape.
        let socket_path = dir.path().join("legacy-daemon.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();

        // No state file at all, and the legacy socket is reachable.
        // HTTP discovery remains the readiness contract.
        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Starting);
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

    /// Draining means the daemon is finishing existing sessions before a
    /// restart. New adapters must wait for that daemon to exit instead of
    /// attaching new sessions to the draining process.
    #[test]
    fn test_readiness_stopping_when_draining_with_live_pid() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
        fs::write(paths.daemon_state(), "draining").unwrap();
        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Stopping);
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
    /// binary detected during startup), readiness should classify it as a
    /// shutdown handoff instead of ready for fresh sessions.
    #[test]
    fn test_readiness_reclassifies_starting_to_draining_as_stopping() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();

        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
        let state_path = paths.daemon_state();
        fs::write(&state_path, "starting").unwrap();

        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Starting);

        fs::write(&state_path, "draining").unwrap();
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Stopping);
    }
}
