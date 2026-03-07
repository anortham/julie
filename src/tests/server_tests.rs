//! Tests for the HTTP server and API endpoints.

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt; // for `oneshot`

use crate::api;
use crate::server::AppState;

/// Build a test app with a fresh AppState.
fn test_app() -> axum::Router {
    let state = Arc::new(AppState {
        start_time: Instant::now(),
    });
    axum::Router::new()
        .nest("/api", api::routes(state))
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
// PROJECTS ENDPOINT TESTS (STUBS)
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
async fn test_create_project_returns_501() {
    let app = test_app();
    let req = Request::builder()
        .method("POST")
        .uri("/api/projects")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
}

#[tokio::test]
async fn test_delete_project_returns_501() {
    let app = test_app();
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/projects/some-id")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
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
    let result = crate::server::start_server(port, std::future::pending()).await;
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
