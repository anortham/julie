//! Integration tests for daemon server startup and lifecycle.

use std::sync::Arc;
use std::time::Duration;

use crate::daemon;
use crate::daemon::session::SessionTracker;
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

// ── D-C3 ──────────────────────────────────────────────────────────────────────
// Transient OS errors from accept() (EMFILE, ConnectionReset, Interrupted)
// must be classified as non-fatal so the accept loop survives them.
#[test]
fn test_transient_accept_errors_are_classified_correctly() {
    use std::io;

    // ConnectionReset: client vanished before accept completed — transient
    assert!(daemon::is_transient_accept_error(
        &io::Error::from(io::ErrorKind::ConnectionReset)
    ));
    // Interrupted (EINTR): always transient
    assert!(daemon::is_transient_accept_error(
        &io::Error::from(io::ErrorKind::Interrupted)
    ));
    // ConnectionAborted: transient
    assert!(daemon::is_transient_accept_error(
        &io::Error::from(io::ErrorKind::ConnectionAborted)
    ));
    // NotFound / PermissionDenied / BrokenPipe: NOT transient (structural errors)
    assert!(!daemon::is_transient_accept_error(
        &io::Error::from(io::ErrorKind::NotFound)
    ));
    assert!(!daemon::is_transient_accept_error(
        &io::Error::from(io::ErrorKind::PermissionDenied)
    ));
    assert!(!daemon::is_transient_accept_error(
        &io::Error::from(io::ErrorKind::BrokenPipe)
    ));
}

// EMFILE (fd exhaustion) must also be classified as transient
#[cfg(unix)]
#[test]
fn test_emfile_is_transient_accept_error() {
    let e = std::io::Error::from_raw_os_error(libc::EMFILE);
    assert!(
        daemon::is_transient_accept_error(&e),
        "EMFILE should be classified as transient"
    );
    let e = std::io::Error::from_raw_os_error(libc::ENFILE);
    assert!(
        daemon::is_transient_accept_error(&e),
        "ENFILE should be classified as transient"
    );
}

// ── D-H1 ──────────────────────────────────────────────────────────────────────
// On shutdown, active sessions must be given time to finish (up to 5s)
// before the daemon exits. drain_sessions encapsulates this wait loop.

#[tokio::test]
async fn test_drain_sessions_waits_for_active_sessions() {
    let sessions = Arc::new(SessionTracker::new());
    let session_id = sessions.add_session();
    assert_eq!(sessions.active_count(), 1);

    // End the session after 100 ms
    let sessions_clone = Arc::clone(&sessions);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        sessions_clone.remove_session(&session_id);
    });

    // drain_sessions should block until the session ends, then return true
    let drained = daemon::drain_sessions(&sessions, Duration::from_secs(2)).await;
    assert!(drained, "drain_sessions should return true when sessions complete before timeout");
    assert_eq!(sessions.active_count(), 0);
}

#[tokio::test]
async fn test_drain_sessions_times_out_with_persistent_session() {
    let sessions = Arc::new(SessionTracker::new());
    sessions.add_session(); // never removed

    // A tiny timeout must expire while session is still active
    let drained = daemon::drain_sessions(&sessions, Duration::from_millis(50)).await;
    assert!(!drained, "drain_sessions should return false on timeout");
    assert_eq!(sessions.active_count(), 1, "session should still be active after timeout");
}
