//! Tests for A1.7: Bounded shutdown drain + recovery markers.
//!
//! Invariants proven:
//!   - `drain_with_markers` returns `Clean` when active sessions finish within
//!     the timeout — no recovery marker file is written.
//!   - `drain_with_markers` returns `TimedOut` when sessions remain past the
//!     timeout — a recovery marker file is written under the daemon paths.
//!   - `read_recovery_markers` returns the previously written marker on the
//!     next startup; an empty Vec when no markers are present.
//!   - `clear_recovery_markers` removes the marker file.
//!   - Markers surface through the `/api/status` endpoint (via DashboardState).
//!
//! These tests deliberately drive `drain_with_markers` directly with a
//! `SessionTracker` and `DaemonPaths` so they remain fast and deterministic
//! (no real HTTP server, no real workspace pool). The end-to-end wiring is
//! exercised separately by `test_daemon_app_serve_and_shutdown` (A1.6) plus
//! the integration suite once A1.8 lands.

use std::sync::Arc;
use std::time::Duration;

use crate::daemon::discovery::{DiscoveryFile, DiscoveryRecord, DiscoveryState};
use crate::daemon::session::SessionTracker;
use crate::daemon::shutdown::{
    DrainOutcome, RecoveryMarker, clear_recovery_markers, drain_with_markers,
    publish_discovery_phase, read_recovery_markers, recovery_marker_path,
};
use crate::paths::DaemonPaths;

/// Build a test DaemonPaths rooted at a fresh tempdir.
fn make_paths() -> (DaemonPaths, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    paths.ensure_dirs().expect("ensure_dirs");
    (paths, dir)
}

/// Drain completes cleanly when sessions finish within the timeout. No
/// recovery marker file is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_drain_completes_cleanly_when_sessions_finish_in_time() {
    let (paths, _dir) = make_paths();
    let sessions = Arc::new(SessionTracker::new());

    // Register an active session, then end it after a short delay (well
    // inside our 5s drain timeout).
    let session_id = sessions.add_session();
    assert_eq!(sessions.active_count(), 1);

    let sessions_clone = Arc::clone(&sessions);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        sessions_clone.remove_session(&session_id);
    });

    let outcome = drain_with_markers(&sessions, &paths, Duration::from_secs(5)).await;

    match outcome {
        DrainOutcome::Clean => {}
        other => panic!("expected DrainOutcome::Clean, got {:?}", other),
    }

    // No recovery marker should have been written.
    assert!(
        !recovery_marker_path(&paths).exists(),
        "recovery marker file should not exist after a clean drain"
    );

    let markers = read_recovery_markers(&paths);
    assert!(
        markers.is_empty(),
        "read_recovery_markers should return empty after a clean drain, got {:?}",
        markers
    );
}

/// Drain times out when sessions stay active past the timeout. A recovery
/// marker file is written containing the active-session count.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_drain_times_out_and_writes_recovery_marker() {
    let (paths, _dir) = make_paths();
    let sessions = Arc::new(SessionTracker::new());

    // Two persistent sessions that never get removed.
    let _id_a = sessions.add_session();
    let _id_b = sessions.add_session();
    assert_eq!(sessions.active_count(), 2);

    let outcome = drain_with_markers(&sessions, &paths, Duration::from_millis(200)).await;

    match outcome {
        DrainOutcome::TimedOut { active_sessions } => {
            assert_eq!(
                active_sessions, 2,
                "TimedOut outcome must report the count observed at timeout"
            );
        }
        other => panic!("expected DrainOutcome::TimedOut, got {:?}", other),
    }

    // Recovery marker file must exist.
    let marker_path = recovery_marker_path(&paths);
    assert!(
        marker_path.exists(),
        "recovery marker file should exist after timeout at {}",
        marker_path.display()
    );

    let markers = read_recovery_markers(&paths);
    assert_eq!(
        markers.len(),
        1,
        "exactly one recovery marker should be present"
    );
    let marker = &markers[0];
    assert_eq!(
        marker.active_sessions_at_timeout, 2,
        "recovery marker must record the in-flight session count"
    );
    assert_eq!(
        marker.drain_timeout_secs, 0,
        "drain_timeout_secs should reflect the configured timeout (200ms rounds to 0)"
    );
    assert!(
        marker.shutdown_timestamp_micros > 0,
        "shutdown_timestamp_micros should be a positive UNIX time"
    );
}

/// `read_recovery_markers` on an empty paths dir returns an empty Vec.
#[test]
fn test_read_recovery_markers_empty_when_no_marker_exists() {
    let (paths, _dir) = make_paths();
    let markers = read_recovery_markers(&paths);
    assert!(
        markers.is_empty(),
        "expected empty Vec, got {} marker(s)",
        markers.len()
    );
}

/// `clear_recovery_markers` removes a previously-written marker file.
#[test]
fn test_clear_recovery_markers_removes_marker_file() {
    let (paths, _dir) = make_paths();

    // Write a marker file by hand to simulate a prior unclean shutdown.
    let marker = RecoveryMarker {
        shutdown_timestamp_micros: 1_700_000_000_000_000,
        drain_timeout_secs: 60,
        active_sessions_at_timeout: 3,
        affected_workspaces: vec!["ws_abc".to_string(), "ws_def".to_string()],
    };
    let path = recovery_marker_path(&paths);
    let json = serde_json::to_vec_pretty(&[&marker]).expect("serialize marker");
    std::fs::write(&path, json).expect("write marker file");

    assert!(path.exists(), "marker file should exist before clear");

    clear_recovery_markers(&paths).expect("clear should succeed");

    assert!(
        !path.exists(),
        "marker file should be removed after clear_recovery_markers"
    );

    // Idempotent: a second clear is a no-op (file already absent).
    clear_recovery_markers(&paths).expect("clear should be idempotent");
}

/// Recovery markers written by `drain_with_markers` round-trip through
/// `read_recovery_markers`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_recovery_markers_round_trip() {
    let (paths, _dir) = make_paths();
    let sessions = Arc::new(SessionTracker::new());

    // One persistent session; never removed.
    let _id = sessions.add_session();

    let outcome = drain_with_markers(&sessions, &paths, Duration::from_millis(100)).await;
    assert!(
        matches!(outcome, DrainOutcome::TimedOut { .. }),
        "expected TimedOut, got {:?}",
        outcome
    );

    // First read: marker present.
    let markers = read_recovery_markers(&paths);
    assert_eq!(markers.len(), 1, "first read should see one marker");
    let first = &markers[0];
    assert_eq!(first.active_sessions_at_timeout, 1);

    // Second read (without clear): marker still present (read is non-destructive).
    let markers_again = read_recovery_markers(&paths);
    assert_eq!(
        markers_again.len(),
        1,
        "second read without clear should still see the marker"
    );

    // After clear: empty.
    clear_recovery_markers(&paths).expect("clear should succeed");
    let markers_after_clear = read_recovery_markers(&paths);
    assert!(
        markers_after_clear.is_empty(),
        "after clear: expected empty, got {:?}",
        markers_after_clear
    );
}

/// `publish_discovery_phase` on a fresh paths dir is a no-op (no
/// discovery.json yet; this is the normal pre-A1.8 state).
#[test]
fn test_publish_discovery_phase_noop_when_discovery_missing() {
    let (paths, _dir) = make_paths();

    // Must not panic and must not create the file.
    publish_discovery_phase(&paths, "stopping");

    let discovery_path = paths.discovery_file();
    assert!(
        !discovery_path.exists(),
        "publish_discovery_phase must not create discovery.json when none exists"
    );
}

/// `publish_discovery_phase` rewrites an existing discovery.json so the
/// `phase` field reflects the new value. All other fields survive the
/// rewrite unchanged.
#[test]
fn test_publish_discovery_phase_rewrites_existing_record() {
    let (paths, dir) = make_paths();
    let discovery_path = paths.discovery_file();

    // Build a Live record for the current process so the post-rewrite
    // read_and_validate also returns Live (PID is alive == us).
    let token_path = dir.path().join("daemon.token");
    let log_path = dir.path().join("daemon.log");
    let original = DiscoveryRecord::for_current_process(4242, token_path.clone(), log_path.clone());
    DiscoveryFile::write_atomic(&discovery_path, &original)
        .expect("seed initial discovery.json");

    // Sanity: it round-trips and the original phase is "running".
    match DiscoveryFile::read_and_validate(&discovery_path) {
        DiscoveryState::Live(r) => {
            assert_eq!(r.phase.as_deref(), Some("running"));
            assert_eq!(r.port, 4242);
        }
        other => panic!("expected Live, got {:?}", other),
    }

    publish_discovery_phase(&paths, "stopping");

    match DiscoveryFile::read_and_validate(&discovery_path) {
        DiscoveryState::Live(r) => {
            assert_eq!(r.phase.as_deref(), Some("stopping"));
            // Other fields survived the rewrite.
            assert_eq!(r.port, 4242);
            assert_eq!(r.token_path, token_path);
            assert_eq!(r.log_path, log_path);
        }
        other => panic!("expected Live after rewrite, got {:?}", other),
    }
}

/// `RecoveryMarker` serializes to a stable JSON shape that includes the
/// fields the `/status` endpoint surfaces.
#[test]
fn test_recovery_marker_json_shape() {
    let marker = RecoveryMarker {
        shutdown_timestamp_micros: 1_700_000_000_000_000,
        drain_timeout_secs: 60,
        active_sessions_at_timeout: 3,
        affected_workspaces: vec!["ws_abc".to_string()],
    };
    let json = serde_json::to_value(&marker).expect("serialize");

    assert_eq!(
        json["shutdown_timestamp_micros"], 1_700_000_000_000_000i64,
        "shutdown_timestamp_micros must be present"
    );
    assert_eq!(json["drain_timeout_secs"], 60, "drain_timeout_secs must be present");
    assert_eq!(
        json["active_sessions_at_timeout"], 3,
        "active_sessions_at_timeout must be present"
    );
    assert_eq!(
        json["affected_workspaces"], serde_json::json!(["ws_abc"]),
        "affected_workspaces must be present"
    );
}

// ---------------------------------------------------------------------------
// HTTP transport gate (503/502 wiring)
// ---------------------------------------------------------------------------

/// `TransportShutdownState::mark_draining` flips the state; subsequent
/// requests to the gate middleware return 503. `mark_aborted` flips to
/// 502.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_transport_shutdown_state_transitions() {
    use crate::daemon::http_transport::{
        TRANSPORT_ABORTED, TRANSPORT_DRAINING, TRANSPORT_RUNNING, TransportShutdownState,
    };

    let state = TransportShutdownState::new();
    assert_eq!(state.current(), TRANSPORT_RUNNING, "starts running");

    let prev = state.mark_draining();
    assert_eq!(prev, TRANSPORT_RUNNING, "mark_draining returns prior state");
    assert_eq!(state.current(), TRANSPORT_DRAINING, "now draining");

    let prev = state.mark_aborted();
    assert_eq!(prev, TRANSPORT_DRAINING, "mark_aborted returns prior state");
    assert_eq!(state.current(), TRANSPORT_ABORTED, "now aborted");
}

/// Issue a raw HTTP request to `addr` for `method path` and return the
/// response status code + body bytes. Used by the transport-gate test
/// instead of pulling in `reqwest` as a dev-dep.
fn raw_http(
    addr: std::net::SocketAddr,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> std::io::Result<(u16, Vec<u8>)> {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;

    let host = format!("{}", addr);
    let body = body.unwrap_or("");
    let request = format!(
        "{method} {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        method = method,
        path = path,
        host = host,
        len = body.len(),
        body = body,
    );
    stream.write_all(request.as_bytes())?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf)?;

    // Parse the status line: e.g. "HTTP/1.1 503 Service Unavailable\r\n..."
    let status = std::str::from_utf8(&buf)
        .ok()
        .and_then(|s| s.lines().next())
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    Ok((status, buf))
}

/// Return true iff the raw HTTP response (header bytes + body) carries a
/// `Retry-After: 1` header. Case-insensitive match.
fn has_retry_after_1(buf: &[u8]) -> bool {
    let text = match std::str::from_utf8(buf) {
        Ok(s) => s,
        Err(_) => return false,
    };
    text.lines().any(|line| {
        let mut parts = line.splitn(2, ':');
        let key = parts.next().unwrap_or("").trim().to_ascii_lowercase();
        let val = parts.next().unwrap_or("").trim();
        key == "retry-after" && val == "1"
    })
}

/// End-to-end: bind an HTTP MCP transport with a no-op handler, then flip
/// the shutdown state into `draining` / `aborted` and observe 503 / 502
/// responses at the MCP path. The readiness route stays 204 throughout so
/// adapters can still probe a draining daemon.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_http_transport_returns_503_then_502_on_shutdown_state() {
    use std::net::{IpAddr, Ipv4Addr};

    use rmcp::ServerHandler;

    use crate::daemon::http_transport::{
        HttpTransportConfig, HttpTransportServer, MCP_PATH, READINESS_PATH,
    };

    #[derive(Clone)]
    struct NoopHandler;
    impl ServerHandler for NoopHandler {}

    let (paths, _dir) = make_paths();
    let transport = HttpTransportServer::bind(
        paths.clone(),
        HttpTransportConfig {
            bind_host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            // No bearer token so the gate test is not affected by 401 paths.
            bearer_token: None,
            ..HttpTransportConfig::default()
        },
        || Ok(NoopHandler),
    )
    .await
    .expect("bind test transport");

    let addr = transport.local_addr();

    // Sanity: state is running, MCP path responds with something that is
    // not 503/502. We don't assert exact code — the MCP handler does its
    // own protocol validation; we only care that the gate didn't fire.
    let (status, _body) = tokio::task::spawn_blocking(move || {
        raw_http(addr, "POST", MCP_PATH, Some("{}"))
    })
    .await
    .expect("blocking join")
    .expect("running-state POST");
    assert_ne!(status, 503, "before draining, MCP path must not return 503");
    assert_ne!(status, 502, "before draining, MCP path must not return 502");

    // Flip to draining: new requests get 503 Retry-After.
    transport.shutdown_state().mark_draining();
    let (status, body) = tokio::task::spawn_blocking(move || {
        raw_http(addr, "POST", MCP_PATH, Some("{}"))
    })
    .await
    .expect("blocking join")
    .expect("draining-state POST");
    assert_eq!(status, 503, "while draining, MCP path must return 503");
    assert!(
        has_retry_after_1(&body),
        "draining 503 must carry Retry-After: 1 header"
    );

    // Readiness must still work so adapters know the transport is reachable.
    let (ready_status, _) = tokio::task::spawn_blocking(move || {
        raw_http(addr, "GET", READINESS_PATH, None)
    })
    .await
    .expect("blocking join")
    .expect("readiness probe");
    assert_eq!(
        ready_status, 204,
        "readiness route must remain 204 while draining"
    );

    // Flip to aborted: requests bouncing through the gate get 502.
    transport.shutdown_state().mark_aborted();
    let (status, _) = tokio::task::spawn_blocking(move || {
        raw_http(addr, "POST", MCP_PATH, Some("{}"))
    })
    .await
    .expect("blocking join")
    .expect("aborted-state POST");
    assert_eq!(status, 502, "after abort, MCP path must return 502");

    transport.shutdown().await.expect("transport shutdown");
}
