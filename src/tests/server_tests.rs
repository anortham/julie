//! Tests for the HTTP server and API endpoints.

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt; // for `oneshot`
use tokio_util::sync::CancellationToken;

use crate::api;
use crate::mcp_http;
use crate::registry::GlobalRegistry;
use crate::server::AppState;

/// Build a test app with a fresh AppState (API routes only).
fn test_app() -> axum::Router {
    let temp_dir = std::env::temp_dir().join(format!("julie-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let state = Arc::new(AppState {
        start_time: Instant::now(),
        registry: tokio::sync::RwLock::new(GlobalRegistry::new()),
        julie_home: temp_dir,
    });
    axum::Router::new()
        .nest("/api", api::routes(state))
}

/// Build a test app with both API routes and MCP endpoint.
fn test_app_with_mcp() -> axum::Router {
    let temp_dir = std::env::temp_dir().join(format!("julie-test-mcp-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let state = Arc::new(AppState {
        start_time: Instant::now(),
        registry: tokio::sync::RwLock::new(GlobalRegistry::new()),
        julie_home: temp_dir,
    });
    let workspace_root = std::env::current_dir().unwrap();
    let mcp_service = mcp_http::create_mcp_service(workspace_root, CancellationToken::new());
    axum::Router::new()
        .nest("/api", api::routes(state))
        .route_service("/mcp", mcp_service)
}

// ============================================================================
// HEALTH ENDPOINT TESTS
// ============================================================================

#[tokio::test]
async fn test_health_returns_200() {
    let app = test_app();
    let req = Request::builder()
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_health_returns_correct_json_structure() {
    let app = test_app();
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
    let app = test_app();
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
    let app = test_app();
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
    let app = test_app();
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
    let app = test_app();
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
    let workspace_id = registry.register_project(&project_dir).unwrap();

    let state = Arc::new(AppState {
        start_time: Instant::now(),
        registry: tokio::sync::RwLock::new(registry),
        julie_home,
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
    let app = test_app();
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

    // Try to start the server on the same port — should fail with a clear message
    let workspace_root = std::env::current_dir().unwrap();
    let julie_home = std::env::temp_dir().join("julie-test-port-conflict");
    let result = crate::server::start_server(
        port,
        workspace_root,
        std::future::pending(),
        GlobalRegistry::new(),
        julie_home,
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
    let app = test_app_with_mcp();
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
    let app = test_app_with_mcp();
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
    let app = test_app_with_mcp();
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
    // POST a valid MCP initialize request — should get an SSE response with session ID
    let app = test_app_with_mcp();

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
    let app = test_app_with_mcp();
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
    let app = test_app_with_mcp();
    let req = Request::builder()
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
