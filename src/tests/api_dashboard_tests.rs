//! Tests for the dashboard stats REST API endpoint.
//!
//! Tests `GET /api/dashboard/stats` — aggregated statistics from projects,
//! agent dispatches, and detected backends.

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
use crate::daemon_state::{DaemonState, LoadedWorkspace, WorkspaceLoadStatus};
use crate::registry::GlobalRegistry;
use crate::server::AppState;
use crate::workspace::JulieWorkspace;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Create a fresh AppState for dashboard tests.
fn test_state(julie_home: std::path::PathBuf) -> Arc<AppState> {
    let (indexing_sender, _rx) = tokio::sync::mpsc::channel::<IndexRequest>(1);
    let backends = vec![
        BackendInfo {
            name: "claude".to_string(),
            available: true,
            version: Some("1.2.3".to_string()),
        },
        BackendInfo {
            name: "codex".to_string(),
            available: false,
            version: None,
        },
    ];
    let registry = Arc::new(tokio::sync::RwLock::new(GlobalRegistry::new()));
    let cancellation_token = CancellationToken::new();
    Arc::new(AppState {
        start_time: Instant::now(),
        registry: registry.clone(),
        julie_home: julie_home.clone(),
        daemon_state: Arc::new(tokio::sync::RwLock::new(DaemonState::new(
            registry,
            julie_home,
            cancellation_token.clone(),
        ))),
        cancellation_token,
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

/// Register a workspace with the given ID and status.
async fn register_workspace(
    state: &Arc<AppState>,
    id: &str,
    temp_dir: &std::path::Path,
    status: WorkspaceLoadStatus,
) {
    let workspace = JulieWorkspace {
        root: temp_dir.to_path_buf(),
        julie_dir: temp_dir.join(".julie"),
        db: None,
        search_index: None,
        watcher: None,
        embedding_provider: None,
        embedding_runtime_status: None,
        config: Default::default(),
    };

    let loaded_ws = LoadedWorkspace {
        workspace,
        status,
        path: temp_dir.to_path_buf(),
    };

    let mut ds = state.daemon_state.write().await;
    ds.workspaces.insert(id.to_string(), loaded_ws);
}

/// Helper to send a GET request and return (status, json_body).
async fn get_json(app: axum::Router, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    if body.is_empty() {
        return (status, Value::Null);
    }
    let json: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, json)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_dashboard_stats_empty_state() {
    let tmp = tempfile::tempdir().unwrap();
    let state = test_state(tmp.path().to_path_buf());
    let app = test_app(state);

    let (status, json) = get_json(app, "/api/dashboard/stats").await;

    assert_eq!(status, StatusCode::OK);

    // Projects: all zeros
    assert_eq!(json["projects"]["total"], 0);
    assert_eq!(json["projects"]["ready"], 0);
    assert_eq!(json["projects"]["indexing"], 0);
    assert_eq!(json["projects"]["error"], 0);

    // Agents: empty
    assert_eq!(json["agents"]["total_dispatches"], 0);
    assert!(json["agents"]["last_dispatch"].is_null());

    // Backends: from test_state
    assert_eq!(json["backends"].as_array().unwrap().len(), 2);
    assert_eq!(json["backends"][0]["name"], "claude");
    assert_eq!(json["backends"][0]["available"], true);
    assert_eq!(json["backends"][0]["version"], "1.2.3");
    assert_eq!(json["backends"][1]["name"], "codex");
    assert_eq!(json["backends"][1]["available"], false);
    assert!(json["backends"][1]["version"].is_null());

    // Active watchers: none in empty state
    assert_eq!(json["active_watchers"], 0);
}

#[tokio::test]
async fn test_dashboard_stats_project_counts() {
    let tmp = tempfile::tempdir().unwrap();
    let state = test_state(tmp.path().to_path_buf());

    // Create temp dirs for each workspace
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let dir_c = tempfile::tempdir().unwrap();
    let dir_d = tempfile::tempdir().unwrap();
    let dir_e = tempfile::tempdir().unwrap();

    register_workspace(&state, "proj-a", dir_a.path(), WorkspaceLoadStatus::Ready).await;
    register_workspace(&state, "proj-b", dir_b.path(), WorkspaceLoadStatus::Ready).await;
    register_workspace(&state, "proj-c", dir_c.path(), WorkspaceLoadStatus::Indexing).await;
    register_workspace(
        &state,
        "proj-d",
        dir_d.path(),
        WorkspaceLoadStatus::Error("oops".to_string()),
    )
    .await;
    register_workspace(&state, "proj-e", dir_e.path(), WorkspaceLoadStatus::Registered).await;

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/dashboard/stats").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["projects"]["total"], 5);
    assert_eq!(json["projects"]["ready"], 2);
    assert_eq!(json["projects"]["indexing"], 1);
    assert_eq!(json["projects"]["error"], 1);
    assert_eq!(json["projects"]["registered"], 1);
    assert_eq!(json["projects"]["stale"], 0);
}

#[tokio::test]
async fn test_dashboard_stats_agent_dispatches() {
    let tmp = tempfile::tempdir().unwrap();
    let state = test_state(tmp.path().to_path_buf());

    // Start some dispatches
    {
        let mut dm = state.dispatch_manager.write().await;
        dm.start_dispatch("Fix bug #42".to_string(), "project-a".to_string(), "claude".to_string());
        dm.start_dispatch("Add feature".to_string(), "project-b".to_string(), "claude".to_string());
    }

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/dashboard/stats").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["agents"]["total_dispatches"], 2);
    // last_dispatch should be a non-null ISO string
    assert!(json["agents"]["last_dispatch"].is_string());
}

#[tokio::test]
async fn test_dashboard_stats_backends() {
    let tmp = tempfile::tempdir().unwrap();
    // Use a state with no backends
    let (indexing_sender, _rx) = tokio::sync::mpsc::channel::<IndexRequest>(1);
    let registry = Arc::new(tokio::sync::RwLock::new(GlobalRegistry::new()));
    let cancellation_token = CancellationToken::new();
    let julie_home = tmp.path().to_path_buf();
    let state = Arc::new(AppState {
        start_time: Instant::now(),
        registry: registry.clone(),
        julie_home: julie_home.clone(),
        daemon_state: Arc::new(tokio::sync::RwLock::new(DaemonState::new(
            registry,
            julie_home,
            cancellation_token.clone(),
        ))),
        cancellation_token,
        indexing_sender,
        dispatch_manager: Arc::new(tokio::sync::RwLock::new(DispatchManager::new())),
        backends: vec![],
    });

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/dashboard/stats").await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["backends"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_dashboard_stats_all_sections_present() {
    // Verify the response shape always has the expected top-level keys
    let tmp = tempfile::tempdir().unwrap();
    let state = test_state(tmp.path().to_path_buf());
    let app = test_app(state);

    let (status, json) = get_json(app, "/api/dashboard/stats").await;

    assert_eq!(status, StatusCode::OK);
    assert!(json.get("projects").is_some(), "missing 'projects' key");
    assert!(json.get("agents").is_some(), "missing 'agents' key");
    assert!(json.get("backends").is_some(), "missing 'backends' key");
    assert!(
        json.get("active_watchers").is_some(),
        "missing 'active_watchers' key"
    );
}
