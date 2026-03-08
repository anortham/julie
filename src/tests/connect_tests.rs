//! Tests for the connect command: daemon-ensure logic, workspace registration,
//! and bridge error handling.
//!
//! These tests focus on the deterministic parts of the connect flow:
//! - Daemon detection (already running vs. needs start)
//! - Workspace registration HTTP interaction (via mock server)
//! - Backoff schedule constants
//!
//! The full stdio↔HTTP bridge is NOT tested here — it requires a running daemon
//! and real MCP traffic, which belongs in integration tests.

use std::path::PathBuf;

use crate::daemon::{is_daemon_running, pid_file_path, write_pid_file};

// ============================================================================
// DAEMON DETECTION TESTS
// ============================================================================

/// Helper: create a temporary PID file path for testing
fn temp_pid_file() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let pid_path = dir.path().join("daemon.pid");
    (dir, pid_path)
}

#[test]
fn test_daemon_not_running_when_no_pid_file() {
    let (_dir, pid_path) = temp_pid_file();
    let result = is_daemon_running(&pid_path);
    assert!(
        result.is_none(),
        "Should detect daemon as not running when PID file is absent"
    );
}

#[test]
fn test_daemon_not_running_when_stale_pid() {
    let (_dir, pid_path) = temp_pid_file();
    // PID 99999999 almost certainly doesn't exist
    write_pid_file(&pid_path, 99999999, 7890).unwrap();
    let result = is_daemon_running(&pid_path);
    assert!(
        result.is_none(),
        "Should detect daemon as not running when PID is stale"
    );
}

#[test]
fn test_daemon_detected_as_running_for_current_process() {
    let (_dir, pid_path) = temp_pid_file();
    let our_pid = std::process::id();
    write_pid_file(&pid_path, our_pid, 7890).unwrap();
    let result = is_daemon_running(&pid_path);
    assert!(
        result.is_some(),
        "Should detect daemon as running when PID matches current process"
    );
    let info = result.unwrap();
    assert_eq!(info.port, 7890);
}

#[test]
fn test_daemon_running_returns_existing_port() {
    let (_dir, pid_path) = temp_pid_file();
    let our_pid = std::process::id();
    write_pid_file(&pid_path, our_pid, 9999).unwrap();
    let info = is_daemon_running(&pid_path).unwrap();
    assert_eq!(
        info.port, 9999,
        "Should return the port from the PID file, not a default"
    );
}

// ============================================================================
// WORKSPACE REGISTRATION TESTS (with real HTTP server)
// ============================================================================

#[tokio::test]
async fn test_register_workspace_created() {
    // Spin up a minimal axum server that mimics POST /api/projects
    use axum::{Router, routing::post, Json};

    let app = Router::new().route(
        "/api/projects",
        post(|| async {
            let response = serde_json::json!({
                "workspace_id": "test_abc123",
                "name": "test-project",
                "path": "/tmp/test-project",
                "status": "registered"
            });
            (axum::http::StatusCode::CREATED, Json(response))
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let workspace_root = PathBuf::from("/tmp/test-project");
    let result = crate::connect::register_workspace(port, &workspace_root).await;
    assert!(result.is_ok(), "Registration should succeed: {:?}", result);
    assert_eq!(result.unwrap(), "test_abc123");

    server.abort();
}

#[tokio::test]
async fn test_register_workspace_already_exists() {
    use axum::{Router, routing::post, Json};

    let app = Router::new().route(
        "/api/projects",
        post(|| async {
            let response = serde_json::json!({
                "workspace_id": "existing_xyz",
                "name": "existing-project",
                "path": "/tmp/existing-project",
                "status": "ready"
            });
            (axum::http::StatusCode::CONFLICT, Json(response))
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let workspace_root = PathBuf::from("/tmp/existing-project");
    let result = crate::connect::register_workspace(port, &workspace_root).await;
    assert!(
        result.is_ok(),
        "Already-exists should succeed (idempotent): {:?}",
        result
    );
    assert_eq!(result.unwrap(), "existing_xyz");

    server.abort();
}

#[tokio::test]
async fn test_register_workspace_error_on_bad_request() {
    use axum::{Router, routing::post};

    let app = Router::new().route(
        "/api/projects",
        post(|| async { (axum::http::StatusCode::BAD_REQUEST, "Path does not exist") }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let workspace_root = PathBuf::from("/nonexistent/path");
    let result = crate::connect::register_workspace(port, &workspace_root).await;
    assert!(
        result.is_err(),
        "Should fail for non-existent workspace path"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("400") || err_msg.contains("Bad Request"),
        "Error should mention HTTP status: {}",
        err_msg
    );

    server.abort();
}

#[tokio::test]
async fn test_register_workspace_error_on_unreachable_server() {
    // Port 1 is unlikely to have anything listening
    let workspace_root = PathBuf::from("/tmp/test-project");
    let result = crate::connect::register_workspace(1, &workspace_root).await;
    assert!(
        result.is_err(),
        "Should fail when daemon is unreachable"
    );
}

// ============================================================================
// HEALTH CHECK BACKOFF TESTS
// ============================================================================

#[test]
fn test_backoff_schedule_is_reasonable() {
    let total_ms: u64 = crate::connect::BACKOFF_MS.iter().sum();
    assert!(
        total_ms >= 4000 && total_ms <= 6000,
        "Total backoff should be ~5s, got {}ms",
        total_ms
    );
    assert_eq!(
        crate::connect::BACKOFF_MS.len(),
        7,
        "Should have 7 backoff steps"
    );
}

#[test]
fn test_backoff_schedule_is_monotonically_increasing() {
    for window in crate::connect::BACKOFF_MS.windows(2) {
        assert!(
            window[1] >= window[0],
            "Backoff should be non-decreasing: {} < {}",
            window[1],
            window[0]
        );
    }
}

// ============================================================================
// ENSURE DAEMON RUNNING TESTS (with mock health endpoint)
// ============================================================================

#[tokio::test]
async fn test_ensure_daemon_running_reuses_existing() {
    let (_dir, pid_path) = temp_pid_file();
    let our_pid = std::process::id();
    write_pid_file(&pid_path, our_pid, 8888).unwrap();

    // Temporarily set the PID file path by checking the daemon utility directly
    // (ensure_daemon_running uses pid_file_path() which reads the real ~/.julie,
    //  so we test the component logic here instead)
    let result = is_daemon_running(&pid_path);
    assert!(result.is_some());
    let info = result.unwrap();
    assert_eq!(info.port, 8888, "Should reuse the existing daemon's port");
}

// ============================================================================
// CLI PARSING TESTS
// ============================================================================

#[test]
fn test_connect_command_parsed_with_default_port() {
    use clap::Parser;
    use crate::cli::Cli;

    let cli = Cli::parse_from(["julie-server", "connect"]);
    match cli.command {
        Some(crate::cli::Commands::Connect { port }) => {
            assert_eq!(port, 7890, "Default port should be 7890");
        }
        other => panic!("Expected Connect command, got {:?}", other.is_some()),
    }
}

#[test]
fn test_connect_command_parsed_with_custom_port() {
    use clap::Parser;
    use crate::cli::Cli;

    let cli = Cli::parse_from(["julie-server", "connect", "--port", "9999"]);
    match cli.command {
        Some(crate::cli::Commands::Connect { port }) => {
            assert_eq!(port, 9999, "Custom port should be parsed");
        }
        other => panic!("Expected Connect command, got {:?}", other.is_some()),
    }
}

#[test]
fn test_no_subcommand_still_works() {
    use clap::Parser;
    use crate::cli::Cli;

    let cli = Cli::parse_from(["julie-server"]);
    assert!(
        cli.command.is_none(),
        "No subcommand should parse as None (backward compatible stdio mode)"
    );
}
