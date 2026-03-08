//! Tests for the agent dispatch REST API endpoints.
//!
//! Tests `POST /api/agents/dispatch`, `GET /api/agents/:id`,
//! `GET /api/agents/:id/stream`, `GET /api/agents/history`,
//! and `GET /api/agents/backends`.

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt; // for `oneshot`

use crate::agent::backend::BackendInfo;
use crate::agent::dispatch::DispatchManager;
use crate::api;
use crate::daemon_indexer::IndexRequest;
use crate::daemon_state::DaemonState;
use crate::registry::GlobalRegistry;
use crate::server::AppState;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Create a fresh AppState for agent API tests.
fn test_state(julie_home: std::path::PathBuf) -> Arc<AppState> {
    let (indexing_sender, _rx) = tokio::sync::mpsc::channel::<IndexRequest>(1);
    let backends = vec![
        BackendInfo {
            name: "claude".to_string(),
            available: true,
            version: Some("1.0.0".to_string()),
        },
        BackendInfo {
            name: "test-backend".to_string(),
            available: false,
            version: None,
        },
    ];
    Arc::new(AppState {
        start_time: Instant::now(),
        registry: Arc::new(tokio::sync::RwLock::new(GlobalRegistry::new())),
        julie_home,
        daemon_state: Arc::new(tokio::sync::RwLock::new(DaemonState::new())),
        cancellation_token: CancellationToken::new(),
        indexing_sender,
        dispatch_manager: Arc::new(tokio::sync::RwLock::new(
            DispatchManager::with_backends(backends.clone()),
        )),
        backends,
    })
}

/// Build a test app with API routes.
fn test_app(state: Arc<AppState>) -> axum::Router {
    axum::Router::new().nest("/api", api::routes(state))
}

// ---------------------------------------------------------------------------
// GET /api/agents/backends
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_backends_returns_detected_backends() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    let app = test_app(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/agents/backends")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["backends"].is_array());
    let backends = json["backends"].as_array().unwrap();
    assert_eq!(backends.len(), 2);

    // First backend should be claude (available)
    assert_eq!(backends[0]["name"], "claude");
    assert_eq!(backends[0]["available"], true);
    assert_eq!(backends[0]["version"], "1.0.0");

    // Second backend should be test-backend (unavailable)
    assert_eq!(backends[1]["name"], "test-backend");
    assert_eq!(backends[1]["available"], false);
    assert!(backends[1]["version"].is_null());
}

// ---------------------------------------------------------------------------
// GET /api/agents/history
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_dispatches_empty() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    let app = test_app(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/agents/history")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["dispatches"].is_array());
    assert_eq!(json["dispatches"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_list_dispatches_with_entries() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());

    // Manually insert some dispatches
    {
        let mut dm = state.dispatch_manager.write().await;
        dm.start_dispatch("task one".to_string(), "project-a".to_string());
        dm.start_dispatch("task two".to_string(), "project-b".to_string());
    }

    let app = test_app(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/agents/history")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let dispatches = json["dispatches"].as_array().unwrap();
    assert_eq!(dispatches.len(), 2);

    // Should have standard fields
    let first = &dispatches[0];
    assert!(first["id"].is_string());
    assert!(first["task"].is_string());
    assert!(first["project"].is_string());
    assert!(first["status"].is_string());
    assert!(first["started_at"].is_string());
}

#[tokio::test]
async fn test_list_dispatches_with_limit() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());

    {
        let mut dm = state.dispatch_manager.write().await;
        dm.start_dispatch("task one".to_string(), "project-a".to_string());
        dm.start_dispatch("task two".to_string(), "project-b".to_string());
        dm.start_dispatch("task three".to_string(), "project-a".to_string());
    }

    let app = test_app(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/agents/history?limit=2")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let dispatches = json["dispatches"].as_array().unwrap();
    assert_eq!(dispatches.len(), 2);
}

#[tokio::test]
async fn test_list_dispatches_with_project_filter() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());

    {
        let mut dm = state.dispatch_manager.write().await;
        dm.start_dispatch("task one".to_string(), "project-a".to_string());
        dm.start_dispatch("task two".to_string(), "project-b".to_string());
        dm.start_dispatch("task three".to_string(), "project-a".to_string());
    }

    let app = test_app(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/agents/history?project=project-a")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let dispatches = json["dispatches"].as_array().unwrap();
    assert_eq!(dispatches.len(), 2);
    for d in dispatches {
        assert_eq!(d["project"], "project-a");
    }
}

// ---------------------------------------------------------------------------
// GET /api/agents/:id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_dispatch_returns_detail() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());

    let dispatch_id = {
        let mut dm = state.dispatch_manager.write().await;
        let id = dm.start_dispatch("test task".to_string(), "my-project".to_string());
        dm.append_output(&id, "line 1\n");
        dm.append_output(&id, "line 2\n");
        id
    };

    let app = test_app(state);

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/api/agents/{}", dispatch_id))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["id"], dispatch_id);
    assert_eq!(json["task"], "test task");
    assert_eq!(json["project"], "my-project");
    assert_eq!(json["status"], "running");
    assert!(json["started_at"].is_string());
    assert_eq!(json["output"], "line 1\nline 2\n");
}

#[tokio::test]
async fn test_get_dispatch_completed() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());

    let dispatch_id = {
        let mut dm = state.dispatch_manager.write().await;
        let id = dm.start_dispatch("done task".to_string(), "proj".to_string());
        dm.append_output(&id, "result\n");
        dm.complete_dispatch(&id);
        id
    };

    let app = test_app(state);

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/api/agents/{}", dispatch_id))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "completed");
    assert!(json["completed_at"].is_string());
}

#[tokio::test]
async fn test_get_dispatch_not_found() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    let app = test_app(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/agents/dispatch_nonexistent")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// POST /api/agents/dispatch — dispatch (without actually spawning claude)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_dispatch_missing_task_returns_400() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    let app = test_app(state);

    let body = serde_json::json!({});

    let req = Request::builder()
        .method("POST")
        .uri("/api/agents/dispatch")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    // Missing required field "task" should fail deserialization
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_dispatch_no_workspace_returns_404() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    // No workspace registered
    let app = test_app(state);

    let body = serde_json::json!({
        "task": "Fix the bug",
        "project": "nonexistent"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/agents/dispatch")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// GET /api/agents/:id/stream — SSE streaming
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_stream_dispatch_not_found() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    let app = test_app(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/agents/dispatch_nonexistent/stream")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_stream_dispatch_receives_events() {
    use futures::StreamExt;

    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());

    // Create a dispatch and get subscriber before sending output
    let dispatch_id = {
        let mut dm = state.dispatch_manager.write().await;
        dm.start_dispatch("streaming task".to_string(), "proj".to_string())
    };

    // Spawn a task that will send output after a small delay
    let dm_clone = state.dispatch_manager.clone();
    let id_clone = dispatch_id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let mut dm = dm_clone.write().await;
        dm.append_output(&id_clone, "hello world\n");
        dm.complete_dispatch(&id_clone);
        // Drop the sender by dropping the dispatch manager write lock
    });

    let app = test_app(state);

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/api/agents/{}/stream", dispatch_id))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Read the SSE body
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    // SSE events should contain our data
    assert!(
        body_str.contains("hello world"),
        "SSE body should contain 'hello world', got: {}",
        body_str
    );
}
