//! Tests for the background indexing pipeline.
//!
//! Tests cover:
//! - IndexRequest construction and channel submission
//! - Status transitions: Registered -> Indexing -> Ready/Error
//! - API endpoint: POST /api/projects/:id/index (trigger)
//! - API endpoint: GET /api/projects/:id/status
//! - Sequential processing (queue behavior)
//! - Re-index (force mode)

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

use crate::api;
use crate::daemon_indexer::{IndexRequest, IndexingSender};
use crate::daemon_state::{DaemonState, WorkspaceLoadStatus};
use crate::registry::{GlobalRegistry, ProjectStatus};
use crate::server::AppState;

/// Create an AppState with a real indexing channel for testing.
///
/// Returns (Arc<AppState>, Receiver) -- hold the receiver to keep the channel open.
fn test_state_with_indexer(
    julie_home: std::path::PathBuf,
) -> (Arc<AppState>, tokio::sync::mpsc::Receiver<IndexRequest>) {
    let (tx, rx) = tokio::sync::mpsc::channel::<IndexRequest>(64);
    let registry = Arc::new(tokio::sync::RwLock::new(GlobalRegistry::new()));
    let cancellation_token = CancellationToken::new();
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
        indexing_sender: tx,
        dispatch_manager: Arc::new(tokio::sync::RwLock::new(crate::agent::dispatch::DispatchManager::new())),
        backends: vec![],
    });
    (state, rx)
}

/// Create an AppState with a pre-populated registry for testing.
///
/// Returns (Arc<AppState>, Receiver) -- hold the receiver to keep the channel open.
fn test_state_with_registry(
    julie_home: std::path::PathBuf,
    registry: GlobalRegistry,
) -> (Arc<AppState>, tokio::sync::mpsc::Receiver<IndexRequest>) {
    let (tx, rx) = tokio::sync::mpsc::channel::<IndexRequest>(64);
    let registry_rw = Arc::new(tokio::sync::RwLock::new(registry));
    let cancellation_token = CancellationToken::new();
    let state = Arc::new(AppState {
        start_time: Instant::now(),
        registry: registry_rw.clone(),
        julie_home: julie_home.clone(),
        daemon_state: Arc::new(tokio::sync::RwLock::new(DaemonState::new(
            registry_rw,
            julie_home,
            cancellation_token.clone(),
        ))),
        cancellation_token,
        indexing_sender: tx,
        dispatch_manager: Arc::new(tokio::sync::RwLock::new(crate::agent::dispatch::DispatchManager::new())),
        backends: vec![],
    });
    (state, rx)
}

// ============================================================================
// INDEX REQUEST CONSTRUCTION
// ============================================================================

#[test]
fn test_index_request_construction() {
    let request = IndexRequest {
        workspace_id: "test-ws-123".to_string(),
        project_path: std::env::temp_dir().join("project"),
        force: false,
    };
    assert_eq!(request.workspace_id, "test-ws-123");
    assert!(!request.force);
}

#[test]
fn test_index_request_clone() {
    let request = IndexRequest {
        workspace_id: "ws-1".to_string(),
        project_path: std::env::temp_dir().join("project"),
        force: true,
    };
    let cloned = request.clone();
    assert_eq!(cloned.workspace_id, "ws-1");
    assert!(cloned.force);
}

// ============================================================================
// CHANNEL SUBMISSION
// ============================================================================

#[tokio::test]
async fn test_index_request_can_be_sent_through_channel() {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<IndexRequest>(10);

    let request = IndexRequest {
        workspace_id: "ws-channel-test".to_string(),
        project_path: std::env::temp_dir().join("project"),
        force: false,
    };

    tx.send(request).await.unwrap();

    let received = rx.recv().await.unwrap();
    assert_eq!(received.workspace_id, "ws-channel-test");
}

#[tokio::test]
async fn test_multiple_requests_queued_in_order() {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<IndexRequest>(10);

    for i in 0..5 {
        tx.send(IndexRequest {
            workspace_id: format!("ws-{}", i),
            project_path: std::env::temp_dir().join(format!("project-{}", i)),
            force: false,
        })
        .await
        .unwrap();
    }

    for i in 0..5 {
        let received = rx.recv().await.unwrap();
        assert_eq!(received.workspace_id, format!("ws-{}", i));
    }
}

// ============================================================================
// STATUS TRANSITIONS (unit tests via registry directly)
// ============================================================================

#[test]
fn test_registry_status_transition_to_indexing() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let ws_id = registry.register_project(&project_dir).unwrap().workspace_id().to_string();

    assert_eq!(registry.get_project(&ws_id).unwrap().status, ProjectStatus::Registered);

    registry.mark_indexing(&ws_id);
    assert_eq!(registry.get_project(&ws_id).unwrap().status, ProjectStatus::Indexing);
}

#[test]
fn test_registry_status_transition_to_ready() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let ws_id = registry.register_project(&project_dir).unwrap().workspace_id().to_string();

    registry.mark_indexing(&ws_id);
    registry.mark_ready(&ws_id, 500, 50);

    let entry = registry.get_project(&ws_id).unwrap();
    assert_eq!(entry.status, ProjectStatus::Ready);
    assert_eq!(entry.symbol_count, Some(500));
    assert_eq!(entry.file_count, Some(50));
    assert!(entry.last_indexed.is_some());
}

#[test]
fn test_registry_status_transition_to_error() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let ws_id = registry.register_project(&project_dir).unwrap().workspace_id().to_string();

    registry.mark_indexing(&ws_id);
    registry.mark_error(&ws_id, "something went wrong".to_string());

    let entry = registry.get_project(&ws_id).unwrap();
    assert_eq!(entry.status, ProjectStatus::Error("something went wrong".to_string()));
}

// ============================================================================
// DAEMON STATE STATUS MAPPING
// ============================================================================

#[test]
fn test_daemon_state_indexing_status_maps_to_project_status() {
    let temp = tempfile::tempdir().unwrap();
    let registry = Arc::new(tokio::sync::RwLock::new(GlobalRegistry::new()));
    let ct = CancellationToken::new();
    let julie_home = temp.path().join("julie-home");
    let daemon_state_arc = Arc::new(tokio::sync::RwLock::new(DaemonState::new(
        registry.clone(), julie_home.clone(), ct.clone(),
    )));
    let mut state = DaemonState::new(registry, julie_home, ct);
    let project_path = temp.path().join("project");

    state.register_workspace("ws-1".to_string(), project_path, daemon_state_arc);

    // Manually set status to Indexing
    if let Some(loaded) = state.workspaces.get_mut("ws-1") {
        loaded.status = WorkspaceLoadStatus::Indexing;
    }

    let status = state.project_status_for("ws-1");
    assert_eq!(status, ProjectStatus::Indexing);
}

// ============================================================================
// WORKSPACES NEEDING INDEXING
// ============================================================================

#[test]
fn test_workspaces_needing_indexing_returns_registered_and_stale() {
    let temp = tempfile::tempdir().unwrap();
    let registry = Arc::new(tokio::sync::RwLock::new(GlobalRegistry::new()));
    let ct = CancellationToken::new();
    let julie_home = temp.path().join("julie-home");
    let daemon_state_arc = Arc::new(tokio::sync::RwLock::new(DaemonState::new(
        registry.clone(), julie_home.clone(), ct.clone(),
    )));
    let mut state = DaemonState::new(registry, julie_home, ct);

    // Register three workspaces with different statuses
    let path_a = temp.path().join("project-a");
    let path_b = temp.path().join("project-b");
    let path_c = temp.path().join("project-c");

    state.register_workspace("ws-a".to_string(), path_a.clone(), daemon_state_arc.clone());
    state.register_workspace("ws-b".to_string(), path_b.clone(), daemon_state_arc.clone());
    state.register_workspace("ws-c".to_string(), path_c.clone(), daemon_state_arc.clone());

    // ws-a: Registered (default for no .julie dir)
    // ws-b: set to Stale
    if let Some(loaded) = state.workspaces.get_mut("ws-b") {
        loaded.status = WorkspaceLoadStatus::Stale;
    }
    // ws-c: set to Ready (should NOT be returned)
    if let Some(loaded) = state.workspaces.get_mut("ws-c") {
        loaded.status = WorkspaceLoadStatus::Ready;
    }

    let needing = state.workspaces_needing_indexing();
    let mut ids: Vec<&str> = needing.iter().map(|(id, _)| id.as_str()).collect();
    ids.sort();

    assert_eq!(ids.len(), 2, "Should return Registered + Stale, not Ready");
    assert!(ids.contains(&"ws-a"), "Registered workspace should be included");
    assert!(ids.contains(&"ws-b"), "Stale workspace should be included");
}

#[test]
fn test_workspaces_needing_indexing_excludes_error_and_indexing() {
    let temp = tempfile::tempdir().unwrap();
    let registry = Arc::new(tokio::sync::RwLock::new(GlobalRegistry::new()));
    let ct = CancellationToken::new();
    let julie_home = temp.path().join("julie-home");
    let daemon_state_arc = Arc::new(tokio::sync::RwLock::new(DaemonState::new(
        registry.clone(), julie_home.clone(), ct.clone(),
    )));
    let mut state = DaemonState::new(registry, julie_home, ct);

    let path_a = temp.path().join("project-a");
    let path_b = temp.path().join("project-b");

    state.register_workspace("ws-a".to_string(), path_a.clone(), daemon_state_arc.clone());
    state.register_workspace("ws-b".to_string(), path_b.clone(), daemon_state_arc.clone());

    // ws-a: set to Error
    if let Some(loaded) = state.workspaces.get_mut("ws-a") {
        loaded.status = WorkspaceLoadStatus::Error("bad".to_string());
    }
    // ws-b: set to Indexing
    if let Some(loaded) = state.workspaces.get_mut("ws-b") {
        loaded.status = WorkspaceLoadStatus::Indexing;
    }

    let needing = state.workspaces_needing_indexing();
    assert!(needing.is_empty(), "Error and Indexing should not be returned");
}

#[test]
fn test_workspaces_needing_indexing_empty_state() {
    let temp = tempfile::tempdir().unwrap();
    let registry = Arc::new(tokio::sync::RwLock::new(GlobalRegistry::new()));
    let ct = CancellationToken::new();
    let julie_home = temp.path().join("julie-home");
    let state = DaemonState::new(registry, julie_home, ct);

    let needing = state.workspaces_needing_indexing();
    assert!(needing.is_empty(), "No workspaces should need indexing when state is empty");
}

#[test]
fn test_workspaces_needing_indexing_returns_correct_paths() {
    let temp = tempfile::tempdir().unwrap();
    let registry = Arc::new(tokio::sync::RwLock::new(GlobalRegistry::new()));
    let ct = CancellationToken::new();
    let julie_home = temp.path().join("julie-home");
    let daemon_state_arc = Arc::new(tokio::sync::RwLock::new(DaemonState::new(
        registry.clone(), julie_home.clone(), ct.clone(),
    )));
    let mut state = DaemonState::new(registry, julie_home, ct);

    let path_a = temp.path().join("project-a");
    state.register_workspace("ws-a".to_string(), path_a.clone(), daemon_state_arc);

    let needing = state.workspaces_needing_indexing();
    assert_eq!(needing.len(), 1);
    assert_eq!(needing[0].0, "ws-a");
    assert_eq!(needing[0].1, path_a);
}

// ============================================================================
// API ENDPOINT TESTS: POST /api/projects/:id/index
// ============================================================================

#[tokio::test]
async fn test_trigger_index_returns_404_for_unknown_project() {
    let temp = tempfile::tempdir().unwrap();
    let (state, _rx) = test_state_with_indexer(temp.path().to_path_buf());
    let app = axum::Router::new().nest("/api", api::routes(state));

    let req = Request::builder()
        .method("POST")
        .uri("/api/projects/nonexistent-ws/index")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_trigger_index_returns_202_for_registered_project() {
    let temp = tempfile::tempdir().unwrap();
    let julie_home = temp.path().join("julie-home");
    std::fs::create_dir_all(&julie_home).unwrap();

    let project_dir = temp.path().join("my-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let ws_id = registry.register_project(&project_dir).unwrap().workspace_id().to_string();

    let (state, _rx) = test_state_with_registry(julie_home, registry);
    let app = axum::Router::new().nest("/api", api::routes(state));

    let req = Request::builder()
        .method("POST")
        .uri(&format!("/api/projects/{}/index", ws_id))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["workspace_id"], ws_id);
    assert_eq!(json["status"], "indexing");
    assert_eq!(json["message"], "Indexing queued");
}

#[tokio::test]
async fn test_trigger_index_with_force_flag() {
    let temp = tempfile::tempdir().unwrap();
    let julie_home = temp.path().join("julie-home");
    std::fs::create_dir_all(&julie_home).unwrap();

    let project_dir = temp.path().join("my-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let ws_id = registry.register_project(&project_dir).unwrap().workspace_id().to_string();

    let (state, _rx) = test_state_with_registry(julie_home, registry);
    let app = axum::Router::new().nest("/api", api::routes(state));

    let body = serde_json::json!({ "force": true });
    let req = Request::builder()
        .method("POST")
        .uri(&format!("/api/projects/{}/index", ws_id))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["message"], "Force re-indexing queued");
}

// ============================================================================
// API ENDPOINT TESTS: GET /api/projects/:id/status
// ============================================================================

#[tokio::test]
async fn test_get_status_returns_404_for_unknown_project() {
    let temp = tempfile::tempdir().unwrap();
    let (state, _rx) = test_state_with_indexer(temp.path().to_path_buf());
    let app = axum::Router::new().nest("/api", api::routes(state));

    let req = Request::builder()
        .uri("/api/projects/nonexistent-ws/status")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_status_returns_registered_for_new_project() {
    let temp = tempfile::tempdir().unwrap();
    let julie_home = temp.path().join("julie-home");
    std::fs::create_dir_all(&julie_home).unwrap();

    let project_dir = temp.path().join("my-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let ws_id = registry.register_project(&project_dir).unwrap().workspace_id().to_string();

    let (state, _rx) = test_state_with_registry(julie_home, registry);
    let app = axum::Router::new().nest("/api", api::routes(state));

    let req = Request::builder()
        .uri(&format!("/api/projects/{}/status", ws_id))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["workspace_id"], ws_id);
    assert_eq!(json["status"], "registered");
}

#[tokio::test]
async fn test_get_status_reflects_indexing_state() {
    let temp = tempfile::tempdir().unwrap();
    let julie_home = temp.path().join("julie-home");
    std::fs::create_dir_all(&julie_home).unwrap();

    let project_dir = temp.path().join("my-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let ws_id = registry.register_project(&project_dir).unwrap().workspace_id().to_string();

    let ct = CancellationToken::new();
    let registry_rw = Arc::new(tokio::sync::RwLock::new(registry));
    let daemon_state = Arc::new(tokio::sync::RwLock::new(DaemonState::new(
        registry_rw.clone(),
        julie_home.clone(),
        ct.clone(),
    )));
    {
        let mut ds = daemon_state.write().await;
        ds.register_workspace(ws_id.clone(), project_dir, daemon_state.clone());

        // Manually set status to Indexing
        if let Some(loaded) = ds.workspaces.get_mut(&ws_id) {
            loaded.status = WorkspaceLoadStatus::Indexing;
        }
    }

    let (tx, _rx) = tokio::sync::mpsc::channel::<IndexRequest>(1);
    let state = Arc::new(AppState {
        start_time: Instant::now(),
        registry: registry_rw,
        julie_home,
        daemon_state,
        cancellation_token: ct,
        indexing_sender: tx,
        dispatch_manager: Arc::new(tokio::sync::RwLock::new(crate::agent::dispatch::DispatchManager::new())),
        backends: vec![],
    });
    let app = axum::Router::new().nest("/api", api::routes(state));

    let req = Request::builder()
        .uri(&format!("/api/projects/{}/status", ws_id))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "indexing");
}

// ============================================================================
// INDEXING WORKER: STATUS UPDATES
// ============================================================================

#[tokio::test]
async fn test_worker_marks_indexing_then_processes() {
    // This test verifies that when we send a request through the real worker,
    // the registry gets updated. We use a non-existent path to trigger an error,
    // which should result in an Error status.
    let temp = tempfile::tempdir().unwrap();
    let julie_home = temp.path().join("julie-home");
    std::fs::create_dir_all(&julie_home).unwrap();

    // Create a project dir that exists for registration but will fail indexing
    // because it has no source files and the workspace init might fail.
    let project_dir = temp.path().join("empty-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let ws_id = registry
        .register_project(&project_dir)
        .unwrap()
        .workspace_id()
        .to_string();

    let registry_arc = Arc::new(tokio::sync::RwLock::new(registry));
    let ct = CancellationToken::new();
    let daemon_state_arc = Arc::new(tokio::sync::RwLock::new(DaemonState::new(
        registry_arc.clone(),
        julie_home.clone(),
        ct.clone(),
    )));

    let tx = crate::daemon_indexer::spawn_indexing_worker(
        registry_arc.clone(),
        daemon_state_arc.clone(),
        julie_home.clone(),
        ct,
    );

    // Send an indexing request
    tx.send(IndexRequest {
        workspace_id: ws_id.clone(),
        project_path: project_dir,
        force: false,
    })
    .await
    .unwrap();

    // Give the worker some time to process
    // The indexing of an empty project should either succeed quickly
    // or fail quickly. Either way, the status should change from Registered.
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    let reg = registry_arc.read().await;
    let entry = reg.get_project(&ws_id).unwrap();

    // The status should be either Ready or Error (not Registered or Indexing)
    match &entry.status {
        ProjectStatus::Ready => {
            // Empty project indexed successfully (0 symbols, 0 files)
            assert!(entry.last_indexed.is_some());
        }
        ProjectStatus::Error(msg) => {
            // Indexing failed with some error message
            assert!(!msg.is_empty(), "Error message should not be empty");
        }
        other => {
            panic!(
                "Expected Ready or Error after indexing, got {:?}",
                other
            );
        }
    }
}

#[tokio::test]
async fn test_worker_updates_daemon_state_on_success() {
    // Index a project with a real source file and verify daemon state is updated
    let temp = tempfile::tempdir().unwrap();
    let julie_home = temp.path().join("julie-home");
    std::fs::create_dir_all(&julie_home).unwrap();

    let project_dir = temp.path().join("real-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    // Create a simple Rust source file so indexing has something to work with
    std::fs::write(
        project_dir.join("main.rs"),
        "fn main() {\n    println!(\"hello\");\n}\n",
    )
    .unwrap();

    let mut registry = GlobalRegistry::new();
    let ws_id = registry
        .register_project(&project_dir)
        .unwrap()
        .workspace_id()
        .to_string();

    let registry_arc = Arc::new(tokio::sync::RwLock::new(registry));
    let ct = CancellationToken::new();
    let daemon_state_arc = Arc::new(tokio::sync::RwLock::new(DaemonState::new(
        registry_arc.clone(),
        julie_home.clone(),
        ct.clone(),
    )));

    let tx = crate::daemon_indexer::spawn_indexing_worker(
        registry_arc.clone(),
        daemon_state_arc.clone(),
        julie_home.clone(),
        ct.clone(),
    );

    tx.send(IndexRequest {
        workspace_id: ws_id.clone(),
        project_path: project_dir.clone(),
        force: false,
    })
    .await
    .unwrap();

    // Wait for indexing to complete
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    // Check registry status
    let reg = registry_arc.read().await;
    let entry = reg.get_project(&ws_id).unwrap();

    // The project should be Ready (it has a valid source file)
    assert_eq!(
        entry.status,
        ProjectStatus::Ready,
        "Project should be Ready after indexing, got {:?}",
        entry.status
    );
    assert!(entry.symbol_count.unwrap_or(0) > 0, "Should have indexed at least one symbol");
    assert!(entry.file_count.unwrap_or(0) > 0, "Should have indexed at least one file");
    drop(reg);

    // Check daemon state has loaded workspace
    let ds = daemon_state_arc.read().await;
    assert!(
        ds.workspaces.contains_key(&ws_id),
        "Daemon state should contain the workspace after indexing"
    );
    let loaded = &ds.workspaces[&ws_id];
    assert_eq!(
        loaded.status,
        WorkspaceLoadStatus::Ready,
        "Loaded workspace should be Ready"
    );
    assert!(
        loaded.workspace.db.is_some(),
        "Loaded workspace should have a database"
    );
    assert!(
        loaded.workspace.search_index.is_some(),
        "Loaded workspace should have a search index"
    );

    // Check that an MCP service was created
    assert!(
        ds.mcp_services.contains_key(&ws_id),
        "Daemon state should have an MCP service for the indexed workspace"
    );
}

// ============================================================================
// SEQUENTIAL PROCESSING
// ============================================================================

#[tokio::test]
async fn test_worker_processes_multiple_requests_sequentially() {
    let temp = tempfile::tempdir().unwrap();
    let julie_home = temp.path().join("julie-home");
    std::fs::create_dir_all(&julie_home).unwrap();

    // Create two project directories with source files
    let mut workspace_ids = Vec::new();
    for i in 0..2 {
        let project_dir = temp.path().join(format!("project-{}", i));
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(
            project_dir.join("lib.rs"),
            &format!("pub fn func_{}() {{}}\n", i),
        )
        .unwrap();

        let mut registry = GlobalRegistry::new();
        // We need a shared registry, so register both upfront
        workspace_ids.push((
            project_dir,
            format!("project-{}", i),
        ));
        drop(registry);
    }

    // Register all projects in a shared registry
    let mut registry = GlobalRegistry::new();
    let mut ws_ids = Vec::new();
    for (dir, _name) in &workspace_ids {
        let ws_id = registry.register_project(dir).unwrap().workspace_id().to_string();
        ws_ids.push(ws_id);
    }

    let registry_arc = Arc::new(tokio::sync::RwLock::new(registry));
    let ct = CancellationToken::new();
    let daemon_state_arc = Arc::new(tokio::sync::RwLock::new(DaemonState::new(
        registry_arc.clone(),
        julie_home.clone(),
        ct.clone(),
    )));

    let tx = crate::daemon_indexer::spawn_indexing_worker(
        registry_arc.clone(),
        daemon_state_arc.clone(),
        julie_home,
        ct,
    );

    // Queue both requests
    for (i, (dir, _)) in workspace_ids.iter().enumerate() {
        tx.send(IndexRequest {
            workspace_id: ws_ids[i].clone(),
            project_path: dir.clone(),
            force: false,
        })
        .await
        .unwrap();
    }

    // Wait for both to complete
    tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;

    // Both should be ready
    let reg = registry_arc.read().await;
    for ws_id in &ws_ids {
        let entry = reg.get_project(ws_id).unwrap();
        assert!(
            matches!(entry.status, ProjectStatus::Ready),
            "Project {} should be Ready, got {:?}",
            ws_id,
            entry.status
        );
    }
}

// ============================================================================
// RE-INDEX (FORCE MODE)
// ============================================================================

#[tokio::test]
async fn test_worker_handles_force_reindex() {
    let temp = tempfile::tempdir().unwrap();
    let julie_home = temp.path().join("julie-home");
    std::fs::create_dir_all(&julie_home).unwrap();

    let project_dir = temp.path().join("reindex-project");
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(
        project_dir.join("lib.rs"),
        "pub fn original() {}\n",
    )
    .unwrap();

    let mut registry = GlobalRegistry::new();
    let ws_id = registry
        .register_project(&project_dir)
        .unwrap()
        .workspace_id()
        .to_string();

    let registry_arc = Arc::new(tokio::sync::RwLock::new(registry));
    let ct = CancellationToken::new();
    let daemon_state_arc = Arc::new(tokio::sync::RwLock::new(DaemonState::new(
        registry_arc.clone(),
        julie_home.clone(),
        ct.clone(),
    )));

    let tx = crate::daemon_indexer::spawn_indexing_worker(
        registry_arc.clone(),
        daemon_state_arc.clone(),
        julie_home,
        ct,
    );

    // First index
    tx.send(IndexRequest {
        workspace_id: ws_id.clone(),
        project_path: project_dir.clone(),
        force: false,
    })
    .await
    .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    // Verify first index completed
    {
        let reg = registry_arc.read().await;
        let entry = reg.get_project(&ws_id).unwrap();
        assert_eq!(entry.status, ProjectStatus::Ready);
    }

    // Add more source files
    std::fs::write(
        project_dir.join("extra.rs"),
        "pub fn extra_func() {}\npub fn another() {}\n",
    )
    .unwrap();

    // Force re-index
    tx.send(IndexRequest {
        workspace_id: ws_id.clone(),
        project_path: project_dir.clone(),
        force: true,
    })
    .await
    .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    // Verify re-index completed
    let reg = registry_arc.read().await;
    let entry = reg.get_project(&ws_id).unwrap();
    assert_eq!(
        entry.status,
        ProjectStatus::Ready,
        "Project should be Ready after re-index"
    );
    // After re-indexing with the extra file, we should have more symbols
    assert!(
        entry.symbol_count.unwrap_or(0) > 1,
        "Should have more symbols after adding extra.rs, got {:?}",
        entry.symbol_count
    );
}
