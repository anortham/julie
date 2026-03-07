//! Tests for the HTTP server and API endpoints.

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt; // for `oneshot`
use tokio_util::sync::CancellationToken;

use crate::api;
use crate::daemon_state::DaemonState;
use crate::mcp_http;
use crate::registry::GlobalRegistry;
use crate::server::AppState;

/// Create a fresh AppState for testing.
fn test_state(julie_home: std::path::PathBuf) -> Arc<AppState> {
    Arc::new(AppState {
        start_time: Instant::now(),
        registry: tokio::sync::RwLock::new(GlobalRegistry::new()),
        julie_home,
        daemon_state: Arc::new(tokio::sync::RwLock::new(DaemonState::new())),
        cancellation_token: CancellationToken::new(),
    })
}

/// Build a test app with a fresh AppState (API routes only).
///
/// Returns (Router, TempDir) — hold the TempDir to prevent cleanup during the test.
fn test_app() -> (axum::Router, tempfile::TempDir) {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    let router = axum::Router::new()
        .nest("/api", api::routes(state));
    (router, temp_dir)
}

/// Build a test app with both API routes and MCP endpoint.
///
/// Returns (Router, TempDir) — hold the TempDir to prevent cleanup during the test.
fn test_app_with_mcp() -> (axum::Router, tempfile::TempDir) {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    let workspace_root = std::env::current_dir().unwrap();
    let mcp_service = mcp_http::create_mcp_service(workspace_root, CancellationToken::new());
    let router = axum::Router::new()
        .nest("/api", api::routes(state))
        .route_service("/mcp", mcp_service);
    (router, temp_dir)
}

// ============================================================================
// HEALTH ENDPOINT TESTS
// ============================================================================

#[tokio::test]
async fn test_health_returns_200() {
    let (app, _temp) = test_app();
    let req = Request::builder()
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_health_returns_correct_json_structure() {
    let (app, _temp) = test_app();
    let req = Request::builder()
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "ok");
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
    assert!(json["uptime_seconds"].is_number(), "uptime_seconds should be a number");
}

#[tokio::test]
async fn test_health_uptime_is_non_negative() {
    let (app, _temp) = test_app();
    let req = Request::builder()
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let uptime = json["uptime_seconds"].as_u64().unwrap();
    // Server just started, uptime should be very small
    assert!(uptime < 5, "Uptime should be less than 5 seconds in tests, got {}", uptime);
}

// ============================================================================
// PROJECTS ENDPOINT TESTS
// ============================================================================

#[tokio::test]
async fn test_list_projects_returns_empty_array() {
    let (app, _temp) = test_app();
    let req = Request::builder()
        .uri("/api/projects")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, serde_json::json!([]));
}

#[tokio::test]
async fn test_create_project_via_api() {
    let temp_dir = tempfile::tempdir().unwrap();
    let julie_home = temp_dir.path().join("julie-home");
    std::fs::create_dir_all(&julie_home).unwrap();

    // Create a fake project directory
    let project_dir = temp_dir.path().join("my-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let state = Arc::new(AppState {
        start_time: Instant::now(),
        registry: tokio::sync::RwLock::new(GlobalRegistry::new()),
        julie_home: julie_home.clone(),
        daemon_state: Arc::new(tokio::sync::RwLock::new(DaemonState::new())),
        cancellation_token: CancellationToken::new(),
    });
    let app = axum::Router::new()
        .nest("/api", api::routes(state));

    let body = serde_json::json!({ "path": project_dir.to_string_lossy() });
    let req = Request::builder()
        .method("POST")
        .uri("/api/projects")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "my-project");
    assert_eq!(json["status"], "registered");
    assert!(json["workspace_id"].is_string());

    // Verify registry was persisted to disk
    let registry_file = julie_home.join("registry.toml");
    assert!(registry_file.exists(), "Registry file should be created on disk");
}

#[tokio::test]
async fn test_create_project_bad_path() {
    let (app, _temp) = test_app();
    let body = serde_json::json!({ "path": "/nonexistent/path/xyzzy" });
    let req = Request::builder()
        .method("POST")
        .uri("/api/projects")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_create_project_conflict() {
    let temp_dir = tempfile::tempdir().unwrap();
    let julie_home = temp_dir.path().join("julie-home");
    std::fs::create_dir_all(&julie_home).unwrap();

    let project_dir = temp_dir.path().join("my-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let state = Arc::new(AppState {
        start_time: Instant::now(),
        registry: tokio::sync::RwLock::new(GlobalRegistry::new()),
        julie_home: julie_home.clone(),
        daemon_state: Arc::new(tokio::sync::RwLock::new(DaemonState::new())),
        cancellation_token: CancellationToken::new(),
    });

    // Register once via the registry directly
    {
        let mut reg = state.registry.write().await;
        reg.register_project(&project_dir).unwrap();
    }

    let app = axum::Router::new()
        .nest("/api", api::routes(state));

    // Try to register again via API
    let body = serde_json::json!({ "path": project_dir.to_string_lossy() });
    let req = Request::builder()
        .method("POST")
        .uri("/api/projects")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_delete_project_not_found() {
    let (app, _temp) = test_app();
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/projects/nonexistent-id")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_project_success() {
    let temp_dir = tempfile::tempdir().unwrap();
    let julie_home = temp_dir.path().join("julie-home");
    std::fs::create_dir_all(&julie_home).unwrap();

    let project_dir = temp_dir.path().join("my-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let workspace_id = registry.register_project(&project_dir).unwrap().workspace_id().to_string();

    let state = Arc::new(AppState {
        start_time: Instant::now(),
        registry: tokio::sync::RwLock::new(registry),
        julie_home,
        daemon_state: Arc::new(tokio::sync::RwLock::new(DaemonState::new())),
        cancellation_token: CancellationToken::new(),
    });
    let app = axum::Router::new()
        .nest("/api", api::routes(state));

    let req = Request::builder()
        .method("DELETE")
        .uri(&format!("/api/projects/{}", workspace_id))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

// ============================================================================
// 404 FOR UNKNOWN ROUTES
// ============================================================================

#[tokio::test]
async fn test_unknown_route_returns_404() {
    let (app, _temp) = test_app();
    let req = Request::builder()
        .uri("/api/nonexistent")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ============================================================================
// PORT BINDING ERROR MESSAGE TEST
// ============================================================================

#[tokio::test]
async fn test_port_conflict_gives_clear_error_message() {
    // Bind to 0.0.0.0 (same as start_server uses) so the port is actually occupied
    let listener = tokio::net::TcpListener::bind("0.0.0.0:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    // Try to start the server on the same port -- should fail with a clear message
    let workspace_root = std::env::current_dir().unwrap();
    let temp_dir = tempfile::tempdir().unwrap();
    let result = crate::server::start_server(
        port,
        workspace_root,
        std::future::pending(),
        GlobalRegistry::new(),
        temp_dir.path().to_path_buf(),
    )
    .await;
    assert!(result.is_err());

    let err_msg = format!("{:#}", result.unwrap_err());
    assert!(
        err_msg.contains(&format!("Port {}", port)),
        "Error should mention the port number, got: {}",
        err_msg
    );
    assert!(
        err_msg.contains("--port") || err_msg.contains("JULIE_PORT"),
        "Error should suggest --port or JULIE_PORT, got: {}",
        err_msg
    );
}

// ============================================================================
// MCP STREAMABLE HTTP ENDPOINT TESTS
// ============================================================================

#[tokio::test]
async fn test_mcp_endpoint_rejects_get_without_session() {
    // GET to /mcp without a session ID should be rejected (requires session in stateful mode)
    let (app, _temp) = test_app_with_mcp();
    let req = Request::builder()
        .method("GET")
        .uri("/mcp")
        .header("Accept", "text/event-stream")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    // Should be 401 Unauthorized (no session ID provided)
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_mcp_endpoint_rejects_post_without_accept_header() {
    // POST to /mcp without proper Accept header should be rejected
    let (app, _temp) = test_app_with_mcp();
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("Content-Type", "application/json")
        .body(Body::from("{}"))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    // Should be 406 Not Acceptable (missing Accept: application/json, text/event-stream)
    assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
}

#[tokio::test]
async fn test_mcp_endpoint_rejects_post_without_content_type() {
    // POST to /mcp without Content-Type: application/json should be rejected
    let (app, _temp) = test_app_with_mcp();
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("Accept", "application/json, text/event-stream")
        .body(Body::from("{}"))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    // Should be 415 Unsupported Media Type
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn test_mcp_endpoint_initialize_returns_sse_response() {
    // POST a valid MCP initialize request -- should get an SSE response with session ID
    let (app, _temp) = test_app_with_mcp();

    let init_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "TestClient",
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

    // Should return 200 OK with SSE content type
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "MCP initialize should return 200, got {}",
        response.status()
    );

    // Should have a session ID header in stateful mode
    assert!(
        response.headers().contains_key("mcp-session-id"),
        "Response should contain mcp-session-id header"
    );

    // Content type should be text/event-stream (SSE)
    let content_type = response.headers().get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("text/event-stream"),
        "Content-Type should be text/event-stream, got: {}",
        content_type
    );
}

#[tokio::test]
async fn test_mcp_endpoint_rejects_method_not_allowed() {
    // PUT to /mcp should be rejected
    let (app, _temp) = test_app_with_mcp();
    let req = Request::builder()
        .method("PUT")
        .uri("/mcp")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_api_routes_still_work_with_mcp_mounted() {
    // Verify that API routes still work when MCP is mounted alongside
    let (app, _temp) = test_app_with_mcp();
    let req = Request::builder()
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// ============================================================================
// DAEMON STATE WORKSPACE LOADING TESTS
// ============================================================================

#[tokio::test]
async fn test_daemon_state_empty_registry_loads_nothing() {
    let mut state = DaemonState::new();
    let registry = GlobalRegistry::new();
    let ct = CancellationToken::new();

    state.load_registered_projects(&registry, &ct).await;

    assert!(state.workspaces.is_empty());
    assert!(state.mcp_services.is_empty());
}

#[tokio::test]
async fn test_daemon_state_project_without_julie_dir_is_registered() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = temp_dir.path().join("no-index-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let workspace_id = registry.register_project(&project_dir).unwrap().workspace_id().to_string();

    let mut state = DaemonState::new();
    let ct = CancellationToken::new();
    state.load_registered_projects(&registry, &ct).await;

    assert!(state.workspaces.contains_key(&workspace_id));
    let loaded = &state.workspaces[&workspace_id];
    assert_eq!(loaded.status, crate::daemon_state::WorkspaceLoadStatus::Registered);
    // Registered projects don't get MCP services (no .julie dir)
    assert!(!state.mcp_services.contains_key(&workspace_id));
}

#[tokio::test]
async fn test_daemon_state_register_workspace_creates_mcp_service() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = temp_dir.path().join("new-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut state = DaemonState::new();
    let ct = CancellationToken::new();

    state.register_workspace("test-ws-123".to_string(), project_dir.clone(), &ct);

    assert!(state.workspaces.contains_key("test-ws-123"));
    assert!(state.mcp_services.contains_key("test-ws-123"));
    assert_eq!(
        state.workspaces["test-ws-123"].status,
        crate::daemon_state::WorkspaceLoadStatus::Registered,
    );
}

#[tokio::test]
async fn test_daemon_state_remove_workspace_cleans_up() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = temp_dir.path().join("project-to-remove");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut state = DaemonState::new();
    let ct = CancellationToken::new();

    state.register_workspace("ws-remove".to_string(), project_dir.clone(), &ct);
    assert!(state.workspaces.contains_key("ws-remove"));
    assert!(state.mcp_services.contains_key("ws-remove"));

    state.remove_workspace("ws-remove");
    assert!(!state.workspaces.contains_key("ws-remove"));
    assert!(!state.mcp_services.contains_key("ws-remove"));
}

#[tokio::test]
async fn test_daemon_state_project_status_for_missing_returns_registered() {
    let state = DaemonState::new();
    let status = state.project_status_for("nonexistent");
    assert_eq!(status, crate::registry::ProjectStatus::Registered);
}

#[tokio::test]
async fn test_list_projects_reflects_daemon_state_status() {
    // When daemon state says a project is "registered" (no .julie dir),
    // the GET /api/projects endpoint should reflect that.
    let temp_dir = tempfile::tempdir().unwrap();
    let julie_home = temp_dir.path().join("julie-home");
    std::fs::create_dir_all(&julie_home).unwrap();

    let project_dir = temp_dir.path().join("my-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let workspace_id = registry.register_project(&project_dir).unwrap().workspace_id().to_string();

    // Load daemon state from registry (project has no .julie dir -> Registered)
    let mut daemon_state = DaemonState::new();
    let ct = CancellationToken::new();
    daemon_state.load_registered_projects(&registry, &ct).await;

    let state = Arc::new(AppState {
        start_time: Instant::now(),
        registry: tokio::sync::RwLock::new(registry),
        julie_home,
        daemon_state: Arc::new(tokio::sync::RwLock::new(daemon_state)),
        cancellation_token: ct,
    });
    let app = axum::Router::new().nest("/api", api::routes(state));

    let req = Request::builder()
        .uri("/api/projects")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json.as_array().unwrap().len(), 1);
    assert_eq!(json[0]["workspace_id"], workspace_id);
    assert_eq!(json[0]["status"], "registered");
}

#[tokio::test]
async fn test_create_project_updates_daemon_state() {
    // When POST /api/projects registers a new project, the daemon state
    // should also be updated with the new workspace + MCP service.
    let temp_dir = tempfile::tempdir().unwrap();
    let julie_home = temp_dir.path().join("julie-home");
    std::fs::create_dir_all(&julie_home).unwrap();

    let project_dir = temp_dir.path().join("new-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let ct = CancellationToken::new();
    let daemon_state = Arc::new(tokio::sync::RwLock::new(DaemonState::new()));

    let state = Arc::new(AppState {
        start_time: Instant::now(),
        registry: tokio::sync::RwLock::new(GlobalRegistry::new()),
        julie_home,
        daemon_state: daemon_state.clone(),
        cancellation_token: ct,
    });
    let app = axum::Router::new().nest("/api", api::routes(state));

    let body = serde_json::json!({ "path": project_dir.to_string_lossy() });
    let req = Request::builder()
        .method("POST")
        .uri("/api/projects")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let workspace_id = json["workspace_id"].as_str().unwrap();

    // Verify daemon state was updated
    let ds = daemon_state.read().await;
    assert!(
        ds.workspaces.contains_key(workspace_id),
        "Daemon state should contain the new workspace"
    );
    assert!(
        ds.mcp_services.contains_key(workspace_id),
        "Daemon state should have an MCP service for the new workspace"
    );
}

#[tokio::test]
async fn test_delete_project_cleans_up_daemon_state() {
    // When DELETE /api/projects/:id removes a project, the daemon state
    // should also be cleaned up.
    let temp_dir = tempfile::tempdir().unwrap();
    let julie_home = temp_dir.path().join("julie-home");
    std::fs::create_dir_all(&julie_home).unwrap();

    let project_dir = temp_dir.path().join("project-to-delete");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let workspace_id = registry.register_project(&project_dir).unwrap().workspace_id().to_string();

    let ct = CancellationToken::new();
    let mut daemon_state = DaemonState::new();
    daemon_state.register_workspace(workspace_id.clone(), project_dir, &ct);
    let daemon_state = Arc::new(tokio::sync::RwLock::new(daemon_state));

    let state = Arc::new(AppState {
        start_time: Instant::now(),
        registry: tokio::sync::RwLock::new(registry),
        julie_home,
        daemon_state: daemon_state.clone(),
        cancellation_token: ct,
    });
    let app = axum::Router::new().nest("/api", api::routes(state));

    let req = Request::builder()
        .method("DELETE")
        .uri(&format!("/api/projects/{}", workspace_id))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify daemon state was cleaned up
    let ds = daemon_state.read().await;
    assert!(
        !ds.workspaces.contains_key(&workspace_id),
        "Daemon state should not contain the removed workspace"
    );
    assert!(
        !ds.mcp_services.contains_key(&workspace_id),
        "Daemon state should not have MCP service for the removed workspace"
    );
}

// ============================================================================
// PER-WORKSPACE MCP ENDPOINT TESTS
// ============================================================================

#[tokio::test]
async fn test_workspace_mcp_endpoint_returns_404_for_unknown_workspace() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());

    let app = axum::Router::new()
        .route(
            "/mcp/{workspace_id}",
            axum::routing::any(mcp_http::workspace_mcp_handler),
        )
        .with_state(state);

    let req = Request::builder()
        .method("POST")
        .uri("/mcp/nonexistent-workspace-id")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(Body::from("{}"))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_workspace_mcp_endpoint_routes_to_registered_workspace() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = temp_dir.path().join("my-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let ct = CancellationToken::new();
    let mut daemon_state = DaemonState::new();
    daemon_state.register_workspace(
        "test-workspace".to_string(),
        project_dir,
        &ct,
    );

    let state = Arc::new(AppState {
        start_time: Instant::now(),
        registry: tokio::sync::RwLock::new(GlobalRegistry::new()),
        julie_home: temp_dir.path().to_path_buf(),
        daemon_state: Arc::new(tokio::sync::RwLock::new(daemon_state)),
        cancellation_token: ct,
    });

    let app = axum::Router::new()
        .route(
            "/mcp/{workspace_id}",
            axum::routing::any(mcp_http::workspace_mcp_handler),
        )
        .with_state(state);

    // Send an MCP initialize request to the workspace endpoint
    let init_request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "TestClient",
                "version": "1.0.0"
            }
        }
    });

    let req = Request::builder()
        .method("POST")
        .uri("/mcp/test-workspace")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(Body::from(serde_json::to_string(&init_request).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();

    // Should return 200 OK with SSE content type (MCP session created)
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Workspace MCP initialize should return 200, got {}",
        response.status()
    );

    assert!(
        response.headers().contains_key("mcp-session-id"),
        "Response should contain mcp-session-id header"
    );
}
