//! Integration tests for daemon server startup and lifecycle.

use std::sync::Arc;
use std::time::Duration;
use std::{io::Read, io::Write};

use crate::daemon;
use crate::daemon::session::SessionTracker;
use crate::daemon::transport::{TransportEndpoint, TransportMode, TransportProbe};
use crate::paths::DaemonPaths;

/// Wait for a file to appear on disk, polling up to a deadline.
/// Returns true if the file appeared, false if the deadline was exceeded.
async fn wait_for_file(path: &std::path::Path, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if path.exists() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn wait_for_transport_ready(endpoint: &TransportEndpoint, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let endpoint_for_probe = endpoint.clone();
        let probe = tokio::task::spawn_blocking(move || endpoint_for_probe.probe_readiness())
            .await
            .unwrap_or(TransportProbe::NotReady);
        if probe == TransportProbe::Ready {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn raw_http_readiness_response(endpoint: &TransportEndpoint) -> String {
    let endpoint = endpoint.clone();
    tokio::task::spawn_blocking(move || raw_http_readiness_response_sync(&endpoint))
        .await
        .unwrap_or_else(|error| format!("<readiness-task-error {error}>"))
}

fn raw_http_readiness_response_sync(endpoint: &TransportEndpoint) -> String {
    let Some(url) = endpoint.mcp_url() else {
        return "<not-http>".to_string();
    };
    let Some((authority, _)) = url
        .strip_prefix("http://")
        .and_then(|rest| rest.split_once('/'))
    else {
        return format!("<bad-url {url}>");
    };
    let port = authority
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .unwrap_or(0);
    let mut request =
        format!("GET /mcp/ready HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n");
    if let Some(token_path) = endpoint.token_path() {
        match std::fs::read_to_string(token_path) {
            Ok(token) => {
                request.push_str("Authorization: Bearer ");
                request.push_str(token.trim());
                request.push_str("\r\n");
            }
            Err(error) => return format!("<token-read-error {error}>"),
        }
    }
    request.push_str("\r\n");

    match std::net::TcpStream::connect(("127.0.0.1", port)) {
        Ok(mut stream) => {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
            let _ = stream.write_all(request.as_bytes());
            let mut response = String::new();
            let _ = stream.read_to_string(&mut response);
            response
        }
        Err(error) => format!("<connect-error {error}>"),
    }
}

/// Verify the daemon starts up, creates a PID file, and shuts down cleanly
/// when the shutdown signal fires.
#[tokio::test]
async fn test_daemon_starts_and_creates_pid_file() {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    let paths = DaemonPaths::with_home(tmp.path().to_path_buf());
    paths.ensure_dirs().expect("Failed to create dirs");

    // Spawn the daemon in a background task; it will block on shutdown.
    let paths_clone = paths.clone();
    let handle = tokio::spawn(async move { daemon::run_daemon(paths_clone, 0, true).await });

    // Poll for the PID file rather than using a fixed sleep. The embedding
    // service init can take several seconds on first run, so a fixed 200ms
    // window is too tight.
    let pid_path = paths.daemon_pid();
    assert!(
        wait_for_file(&pid_path, Duration::from_secs(30)).await,
        "PID file should appear within 30s at {}",
        pid_path.display()
    );

    // Read the PID and verify it matches our process. After the v7.7.x
    // format change, the file contains `<pid> <creation_time> <binary_mtime>`,
    // so use the dedicated parser instead of treating the whole contents as
    // a single integer.
    let pid = daemon::pid::PidFile::read_pid(&pid_path).expect("PID file should be readable");
    assert_eq!(pid, std::process::id(), "PID should match current process");

    // Send a shutdown signal to stop the daemon.
    // We abort the task since we can't easily send SIGTERM to ourselves in a test.
    handle.abort();
    let _ = handle.await;

    // After abort, the PID file may still exist (abort doesn't run cleanup).
    // This is expected; real shutdown via SIGTERM would clean up.
}

#[tokio::test]
async fn test_daemon_publishes_http_transport_discovery_with_private_token() {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    let paths = DaemonPaths::with_home(tmp.path().to_path_buf());
    paths.ensure_dirs().expect("Failed to create dirs");

    let paths_clone = paths.clone();
    let mut handle = tokio::spawn(async move { daemon::run_daemon(paths_clone, 0, true).await });

    let discovery_path = paths.daemon_mcp_transport();
    assert!(
        wait_for_file(&discovery_path, Duration::from_secs(30)).await,
        "HTTP MCP transport discovery should appear within 30s at {}",
        discovery_path.display()
    );

    let endpoint = TransportEndpoint::read_discovery(&discovery_path)
        .expect("read HTTP MCP transport discovery");
    assert_eq!(endpoint.mode(), TransportMode::StreamableHttp);
    let ready = tokio::select! {
        ready = wait_for_transport_ready(&endpoint, Duration::from_secs(5)) => {
            ready
        }
        result = &mut handle => {
            panic!("daemon exited before HTTP readiness probe passed: {result:?}");
        }
    };
    if !ready {
        let raw_response = raw_http_readiness_response(&endpoint).await;
        handle.abort();
        let _ = handle.await;
        let _ = std::fs::remove_file(paths.daemon_pid());
        let _ = crate::daemon::lifecycle::stop_daemon(&paths);
        panic!(
            "HTTP MCP transport discovery should probe ready; raw readiness response: {raw_response}"
        );
    }

    let token_path = endpoint
        .token_path()
        .expect("daemon HTTP transport should publish a token path");
    let token = std::fs::read_to_string(token_path).expect("read daemon HTTP token");
    assert!(
        token.trim().len() >= 64,
        "daemon HTTP bearer token should be high entropy, got length {}",
        token.trim().len()
    );
    let discovery = std::fs::read_to_string(&discovery_path).expect("read discovery body");
    assert!(
        !discovery.contains(token.trim()),
        "transport discovery must not copy the bearer token value"
    );

    handle.abort();
    let _ = handle.await;
    let _ = std::fs::remove_file(paths.daemon_pid());
    let _ = crate::daemon::lifecycle::stop_daemon(&paths);
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
    assert!(
        drained,
        "drain_sessions should return true when sessions complete before timeout"
    );
    assert_eq!(sessions.active_count(), 0);
}

#[tokio::test]
async fn test_drain_sessions_times_out_with_persistent_session() {
    let sessions = Arc::new(SessionTracker::new());
    sessions.add_session(); // never removed

    // A tiny timeout must expire while session is still active
    let drained = daemon::drain_sessions(&sessions, Duration::from_millis(50)).await;
    assert!(!drained, "drain_sessions should return false on timeout");
    assert_eq!(
        sessions.active_count(),
        1,
        "session should still be active after timeout"
    );
}
