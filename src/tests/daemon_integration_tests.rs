//! End-to-end daemon integration tests.
//!
//! These tests exercise the full daemon flow using axum's `oneshot` test
//! utility -- no TCP server is started, avoiding port conflicts and keeping
//! tests fast.
//!
//! Coverage:
//! - Full flow: register project -> trigger index -> verify status
//! - MCP Streamable HTTP via per-workspace endpoint
//! - Web UI assets served correctly
//! - Stdio mode backward compatibility (CLI parsing)
//! - Startup banner logging

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt; // for `oneshot`

use crate::api;
use crate::daemon_indexer::IndexRequest;
use crate::daemon_state::DaemonState;
use crate::mcp_http;
use crate::registry::GlobalRegistry;
use crate::server::AppState;
use crate::ui;

// ============================================================================
// TEST HELPERS
// ============================================================================

/// Build the full daemon router (API + per-workspace MCP + default MCP + UI).
///
/// This mirrors the router construction in `start_server` so integration tests
/// exercise the same routing topology as production.
fn full_app(state: Arc<AppState>) -> Router {
    let workspace_root = std::env::current_dir().unwrap();
    let ct = state.cancellation_token.clone();

    let default_mcp_service = mcp_http::create_mcp_service(workspace_root, ct.clone(), Some(state.daemon_state.clone()));

    let workspace_mcp_router = Router::new()
        .route(
            "/mcp/{workspace_id}",
            axum::routing::any(mcp_http::workspace_mcp_handler),
        )
        .with_state(state.clone());

    Router::new()
        .nest("/api", api::routes(state))
        .merge(workspace_mcp_router)
        .route_service("/mcp", default_mcp_service)
        .route("/ui/", axum::routing::get(ui::ui_handler))
        .route("/ui/{*path}", axum::routing::get(ui::ui_handler))
        .layer(tower_http::cors::CorsLayer::permissive())
}

/// Create a fresh AppState with a temp directory as julie_home.
///
/// Returns (AppState, TempDir, indexing_receiver). Hold the TempDir to
/// prevent cleanup during the test. The receiver can be used to verify
/// indexing requests were sent.
fn test_setup() -> (
    Arc<AppState>,
    tempfile::TempDir,
    tokio::sync::mpsc::Receiver<IndexRequest>,
) {
    let temp_dir = tempfile::tempdir().unwrap();
    let julie_home = temp_dir.path().join("julie-home");
    std::fs::create_dir_all(&julie_home).unwrap();

    let (indexing_sender, indexing_rx) = tokio::sync::mpsc::channel::<IndexRequest>(16);
    let registry = Arc::new(tokio::sync::RwLock::new(GlobalRegistry::new()));
    let cancellation_token = CancellationToken::new();

    let mut daemon = DaemonState::new(
        registry.clone(),
        julie_home.clone(),
        cancellation_token.clone(),
    );
    // Wire the indexing sender into DaemonState so register_project can queue jobs
    daemon.set_indexing_sender(indexing_sender.clone());

    let state = Arc::new(AppState {
        start_time: Instant::now(),
        registry: registry.clone(),
        julie_home: julie_home.clone(),
        daemon_state: Arc::new(tokio::sync::RwLock::new(daemon)),
        cancellation_token,
        indexing_sender,
        dispatch_manager: Arc::new(tokio::sync::RwLock::new(crate::agent::dispatch::DispatchManager::new())),
        backends: vec![],
    });
    (state, temp_dir, indexing_rx)
}

/// Helper to parse a JSON response body.
async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

/// Helper to get the response body as a string.
async fn text_body(response: axum::response::Response) -> String {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    String::from_utf8_lossy(&body).into_owned()
}

// ============================================================================
// FULL FLOW: REGISTER -> INDEX -> STATUS
// ============================================================================

#[tokio::test]
async fn test_full_flow_register_index_status() {
    let (state, temp_dir, mut indexing_rx) = test_setup();

    // Create a fake project directory with a source file
    let project_dir = temp_dir.path().join("test-project");
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(
        project_dir.join("main.rs"),
        "fn main() { println!(\"hello\"); }",
    )
    .unwrap();

    let app = full_app(state.clone());

    // Step 1: Register the project
    let body = serde_json::json!({ "path": project_dir.to_string_lossy() });
    let req = Request::builder()
        .method("POST")
        .uri("/api/projects")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "Register should return 201 Created"
    );

    let json = json_body(response).await;
    let workspace_id = json["workspace_id"].as_str().unwrap().to_string();
    assert_eq!(json["name"], "test-project");
    assert_eq!(json["status"], "registered");

    // Step 2: Trigger indexing
    let req = Request::builder()
        .method("POST")
        .uri(&format!("/api/projects/{}/index", workspace_id))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::ACCEPTED,
        "Trigger index should return 202 Accepted"
    );

    let json = json_body(response).await;
    assert_eq!(json["status"], "indexing");
    assert_eq!(json["message"], "Indexing queued");

    // Drain auto-index request sent by create_project, then verify the explicit trigger
    let auto_req = indexing_rx.try_recv().unwrap();
    assert_eq!(auto_req.workspace_id, workspace_id);
    assert!(!auto_req.force, "Auto-index should not be forced");

    let index_req = indexing_rx.try_recv().unwrap();
    assert_eq!(index_req.workspace_id, workspace_id);
    assert!(!index_req.force);

    // Step 3: Check status (should still be "registered" since background
    // worker isn't running in this test -- but the endpoint works)
    let req = Request::builder()
        .uri(&format!("/api/projects/{}/status", workspace_id))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let json = json_body(response).await;
    assert_eq!(json["workspace_id"], workspace_id);
    // Status is "registered" because the background worker didn't actually run
    assert_eq!(json["status"], "registered");

    // Step 4: List projects -- should show our registered project
    let req = Request::builder()
        .uri("/api/projects")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let json = json_body(response).await;
    let projects = json.as_array().unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["workspace_id"], workspace_id);
    assert_eq!(projects[0]["name"], "test-project");
}

#[tokio::test]
async fn test_full_flow_register_force_index() {
    let (state, temp_dir, mut indexing_rx) = test_setup();

    let project_dir = temp_dir.path().join("force-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let app = full_app(state.clone());

    // Register
    let body = serde_json::json!({ "path": project_dir.to_string_lossy() });
    let req = Request::builder()
        .method("POST")
        .uri("/api/projects")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    let json = json_body(response).await;
    let workspace_id = json["workspace_id"].as_str().unwrap().to_string();

    // Trigger force indexing
    let force_body = serde_json::json!({ "force": true });
    let req = Request::builder()
        .method("POST")
        .uri(&format!("/api/projects/{}/index", workspace_id))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&force_body).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let json = json_body(response).await;
    assert_eq!(json["message"], "Force re-indexing queued");

    // Drain the auto-index request that create_project now sends
    let auto_req = indexing_rx.try_recv().unwrap();
    assert!(!auto_req.force, "Auto-index request should not be forced");

    // Verify force flag was set on the explicit trigger
    let index_req = indexing_rx.try_recv().unwrap();
    assert!(index_req.force, "Force flag should be true");
}

#[tokio::test]
async fn test_full_flow_register_and_delete() {
    let (state, temp_dir, _rx) = test_setup();

    let project_dir = temp_dir.path().join("delete-me");
    std::fs::create_dir_all(&project_dir).unwrap();

    let app = full_app(state.clone());

    // Register
    let body = serde_json::json!({ "path": project_dir.to_string_lossy() });
    let req = Request::builder()
        .method("POST")
        .uri("/api/projects")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    let json = json_body(response).await;
    let workspace_id = json["workspace_id"].as_str().unwrap().to_string();

    // Delete
    let req = Request::builder()
        .method("DELETE")
        .uri(&format!("/api/projects/{}", workspace_id))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify it's gone from the list
    let req = Request::builder()
        .uri("/api/projects")
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    let json = json_body(response).await;
    assert_eq!(json.as_array().unwrap().len(), 0);

    // Verify status returns 404
    let req = Request::builder()
        .uri(&format!("/api/projects/{}/status", workspace_id))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ============================================================================
// MCP STREAMABLE HTTP VIA FULL APP
// ============================================================================

#[tokio::test]
async fn test_mcp_initialize_via_full_app() {
    let (state, _temp, _rx) = test_setup();
    let app = full_app(state);

    let init_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "IntegrationTestClient",
                "version": "1.0.0"
            }
        }
    });

    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(Body::from(serde_json::to_string(&init_request).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers().contains_key("mcp-session-id"),
        "MCP initialize should return a session ID"
    );

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("text/event-stream"),
        "Response should be SSE, got: {}",
        content_type
    );
}

#[tokio::test]
async fn test_per_workspace_mcp_via_full_app() {
    let (state, temp_dir, _rx) = test_setup();

    // Register a workspace in daemon state so the per-workspace endpoint can find it
    let project_dir = temp_dir.path().join("mcp-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    {
        let mut ds = state.daemon_state.write().await;
        ds.register_workspace(
            "integration-ws".to_string(),
            project_dir,
            state.daemon_state.clone(),
        );
    }

    let app = full_app(state);

    let init_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "WorkspaceTestClient",
                "version": "1.0.0"
            }
        }
    });

    let req = Request::builder()
        .method("POST")
        .uri("/mcp/integration-ws")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(Body::from(serde_json::to_string(&init_request).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Per-workspace MCP initialize should return 200"
    );
    assert!(response.headers().contains_key("mcp-session-id"));
}

#[tokio::test]
async fn test_per_workspace_mcp_unknown_workspace_returns_404() {
    let (state, _temp, _rx) = test_setup();
    let app = full_app(state);

    let req = Request::builder()
        .method("POST")
        .uri("/mcp/nonexistent-workspace")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(Body::from("{}"))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ============================================================================
// WEB UI ASSETS VIA FULL APP
// ============================================================================

#[tokio::test]
async fn test_ui_served_via_full_app() {
    let (state, _temp, _rx) = test_setup();
    let app = full_app(state);

    let req = Request::builder()
        .uri("/ui/")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("text/html"),
        "UI root should serve HTML, got: {}",
        content_type
    );

    let body = text_body(response).await;
    assert!(
        body.contains("<div id=\"app\">"),
        "index.html should contain the Vue mount point"
    );
}

#[tokio::test]
async fn test_ui_spa_fallback_via_full_app() {
    let (state, _temp, _rx) = test_setup();
    let app = full_app(state);

    let req = Request::builder()
        .uri("/ui/projects/some-id")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = text_body(response).await;
    assert!(
        body.contains("<div id=\"app\">"),
        "SPA fallback should serve index.html for unknown routes"
    );
}

#[tokio::test]
async fn test_ui_static_assets_via_full_app() {
    let (state, _temp, _rx) = test_setup();
    let app = full_app(state);

    let req = Request::builder()
        .uri("/ui/favicon.svg")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("svg"),
        "favicon.svg should be served with SVG content type, got: {}",
        content_type
    );
}

// ============================================================================
// ALL ROUTES COEXIST (API + MCP + UI)
// ============================================================================

#[tokio::test]
async fn test_all_routes_coexist() {
    let (state, _temp, _rx) = test_setup();
    let app = full_app(state);

    // API health check
    let req = Request::builder()
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "API health should work");

    // API projects list
    let req = Request::builder()
        .uri("/api/projects")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "API projects should work");

    // UI root
    let req = Request::builder()
        .uri("/ui/")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "UI should work");

    // MCP (default endpoint -- reject GET without session)
    let req = Request::builder()
        .method("GET")
        .uri("/mcp")
        .header("Accept", "text/event-stream")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Default MCP should reject GET without session"
    );
}

// ============================================================================
// STDIO MODE BACKWARD COMPATIBILITY (CLI parsing)
// ============================================================================

#[test]
fn test_stdio_mode_no_subcommand() {
    use crate::cli::Cli;
    use clap::Parser;

    // No subcommand = stdio mode
    let cli = Cli::parse_from(["julie-server"]);
    assert!(
        cli.command.is_none(),
        "No subcommand should mean stdio MCP mode"
    );
}

#[test]
fn test_stdio_mode_with_workspace() {
    use crate::cli::Cli;
    use clap::Parser;

    // --workspace with no subcommand = stdio mode with custom workspace
    let cli = Cli::parse_from(["julie-server", "--workspace", "/tmp/myproject"]);
    assert!(cli.command.is_none());
    assert_eq!(
        cli.workspace,
        Some(std::path::PathBuf::from("/tmp/myproject"))
    );
}

#[test]
fn test_daemon_mode_is_separate_from_stdio() {
    use crate::cli::{Cli, Commands, DaemonAction};
    use clap::Parser;

    // "daemon start" = daemon mode, NOT stdio
    let cli = Cli::parse_from(["julie-server", "daemon", "start", "--foreground"]);
    assert!(cli.command.is_some());
    match cli.command {
        Some(Commands::Daemon {
            action: DaemonAction::Start { foreground, .. },
        }) => {
            assert!(foreground);
        }
        other => panic!(
            "Expected Daemon Start, got command.is_some()={}",
            other.is_some()
        ),
    }
}

// ============================================================================
// ERROR HANDLING
// ============================================================================

#[tokio::test]
async fn test_index_nonexistent_project_returns_404() {
    let (state, _temp, _rx) = test_setup();
    let app = full_app(state);

    let req = Request::builder()
        .method("POST")
        .uri("/api/projects/nonexistent-id/index")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Indexing a non-existent project should return 404"
    );
}

#[tokio::test]
async fn test_status_nonexistent_project_returns_404() {
    let (state, _temp, _rx) = test_setup();
    let app = full_app(state);

    let req = Request::builder()
        .uri("/api/projects/nonexistent-id/status")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Status for non-existent project should return 404"
    );
}

#[tokio::test]
async fn test_register_nonexistent_path_returns_400() {
    let (state, _temp, _rx) = test_setup();
    let app = full_app(state);

    let body = serde_json::json!({ "path": "/nonexistent/path/xyzzy/nope" });
    let req = Request::builder()
        .method("POST")
        .uri("/api/projects")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Registering a nonexistent path should return 400"
    );
}

#[tokio::test]
async fn test_register_duplicate_returns_conflict() {
    let (state, temp_dir, _rx) = test_setup();

    let project_dir = temp_dir.path().join("dup-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let app = full_app(state);

    // Register once
    let body = serde_json::json!({ "path": project_dir.to_string_lossy() });
    let req = Request::builder()
        .method("POST")
        .uri("/api/projects")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    // Register again -- should be 409 Conflict
    let body = serde_json::json!({ "path": project_dir.to_string_lossy() });
    let req = Request::builder()
        .method("POST")
        .uri("/api/projects")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "Duplicate registration should return 409"
    );
}

// ============================================================================
// CORS HEADERS (permissive CORS layer is applied)
// ============================================================================

#[tokio::test]
async fn test_cors_headers_present() {
    let (state, _temp, _rx) = test_setup();
    let app = full_app(state);

    // Send a preflight OPTIONS request
    let req = Request::builder()
        .method("OPTIONS")
        .uri("/api/health")
        .header("Origin", "http://localhost:5173")
        .header("Access-Control-Request-Method", "GET")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();

    // Permissive CORS should include Access-Control-Allow-Origin
    assert!(
        response
            .headers()
            .contains_key("access-control-allow-origin"),
        "CORS headers should be present on preflight responses"
    );
}
