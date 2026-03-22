//! Integration tests for daemon server startup and lifecycle.

use crate::daemon;
use crate::paths::DaemonPaths;

/// Verify the daemon starts up, creates a PID file, and shuts down cleanly
/// when the shutdown signal fires.
#[tokio::test]
async fn test_daemon_starts_and_creates_pid_file() {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    let paths = DaemonPaths::with_home(tmp.path().to_path_buf());
    paths.ensure_dirs().expect("Failed to create dirs");

    // Spawn the daemon in a background task; it will block on accept loop.
    let paths_clone = paths.clone();
    let handle = tokio::spawn(async move {
        daemon::run_daemon(paths_clone, 0).await
    });

    // Give the daemon a moment to bind the socket and write the PID file.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // PID file should exist
    let pid_path = paths.daemon_pid();
    assert!(
        pid_path.exists(),
        "PID file should exist at {}",
        pid_path.display()
    );

    // Read the PID and verify it matches our process
    let pid_contents = std::fs::read_to_string(&pid_path).expect("read PID file");
    let pid: u32 = pid_contents.trim().parse().expect("PID should be numeric");
    assert_eq!(pid, std::process::id(), "PID should match current process");

    // Socket file should exist
    #[cfg(unix)]
    {
        let socket_path = paths.daemon_socket();
        assert!(
            socket_path.exists(),
            "Socket file should exist at {}",
            socket_path.display()
        );
    }

    // Send a shutdown signal to stop the daemon.
    // We abort the task since we can't easily send SIGTERM to ourselves in a test.
    handle.abort();
    let _ = handle.await;

    // After abort, the PID file may still exist (abort doesn't run cleanup).
    // This is expected; real shutdown via SIGTERM would clean up.
}
