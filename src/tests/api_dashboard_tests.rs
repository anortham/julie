//! Tests for the dashboard stats REST API endpoint.
//!
//! Tests `GET /api/dashboard/stats` — aggregated statistics from projects,
//! memories, agent dispatches, and detected backends.

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
use crate::memory::storage::format_checkpoint;
use crate::memory::{Checkpoint, CheckpointType};
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

/// Write a checkpoint file into `.memories/<date>/` under the given root.
fn write_test_checkpoint(root: &std::path::Path, checkpoint: &Checkpoint) {
    let date = &checkpoint.timestamp[..10]; // "YYYY-MM-DD"
    let date_dir = root.join(".memories").join(date);
    std::fs::create_dir_all(&date_dir).unwrap();

    let hash4 = checkpoint
        .id
        .strip_prefix("checkpoint_")
        .unwrap_or(&checkpoint.id)
        .get(..4)
        .unwrap_or("0000");
    let time_part = &checkpoint.timestamp[11..19];
    let hhmmss = time_part.replace(':', "");
    let filename = format!("{}_{}.md", hhmmss, hash4);

    let content = format_checkpoint(checkpoint);
    std::fs::write(date_dir.join(&filename), content).unwrap();
}

/// Create a simple test checkpoint.
fn make_checkpoint(id: &str, timestamp: &str, description: &str) -> Checkpoint {
    Checkpoint {
        id: id.to_string(),
        timestamp: timestamp.to_string(),
        description: description.to_string(),
        checkpoint_type: Some(CheckpointType::Checkpoint),
        context: None,
        decision: None,
        alternatives: None,
        impact: None,
        evidence: None,
        symbols: None,
        next: None,
        confidence: None,
        unknowns: None,
        tags: Some(vec!["test".to_string()]),
        git: None,
        summary: Some("Test summary".to_string()),
        plan_id: None,
    }
}

/// Write the `.active-plan` file and a corresponding plan.
fn write_active_plan(root: &std::path::Path, plan_id: &str, title: &str) {
    let memories_dir = root.join(".memories");
    std::fs::create_dir_all(&memories_dir).unwrap();
    std::fs::write(memories_dir.join(".active-plan"), plan_id).unwrap();

    // Also write the plan file so get_active_plan can find it.
    let plans_dir = memories_dir.join("plans");
    std::fs::create_dir_all(&plans_dir).unwrap();
    let content = format!(
        "---\nid: {plan_id}\ntitle: {title}\nstatus: active\ncreated: \"2026-03-07T10:00:00.000Z\"\nupdated: \"2026-03-07T10:00:00.000Z\"\ntags:\n  - test\n---\n\nPlan content.\n"
    );
    std::fs::write(plans_dir.join(format!("{plan_id}.md")), content).unwrap();
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

    // Memories: empty
    assert_eq!(json["memories"]["total_checkpoints"], 0);
    assert!(json["memories"]["active_plan"].is_null());
    assert!(json["memories"]["last_checkpoint"].is_null());

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
async fn test_dashboard_stats_memory_counts() {
    let tmp = tempfile::tempdir().unwrap();
    let state = test_state(tmp.path().to_path_buf());

    let ws_dir = tempfile::tempdir().unwrap();
    register_workspace(&state, "ws-1", ws_dir.path(), WorkspaceLoadStatus::Ready).await;

    // Write checkpoints across two dates
    let cp1 = make_checkpoint("checkpoint_aaaa1111", "2026-03-07T10:00:00Z", "First");
    let cp2 = make_checkpoint("checkpoint_bbbb2222", "2026-03-07T14:30:00Z", "Second");
    let cp3 = make_checkpoint("checkpoint_cccc3333", "2026-03-08T02:33:01Z", "Third (newest)");

    write_test_checkpoint(ws_dir.path(), &cp1);
    write_test_checkpoint(ws_dir.path(), &cp2);
    write_test_checkpoint(ws_dir.path(), &cp3);

    // Write an active plan
    write_active_plan(ws_dir.path(), "my-plan", "My Active Plan");

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/dashboard/stats").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["memories"]["total_checkpoints"], 3);
    assert_eq!(json["memories"]["active_plan"], "My Active Plan");
    assert_eq!(json["memories"]["last_checkpoint"], "2026-03-08T02:33:01Z");
}

#[tokio::test]
async fn test_dashboard_stats_no_memories_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let state = test_state(tmp.path().to_path_buf());

    // Ready workspace but no .memories/ directory
    let ws_dir = tempfile::tempdir().unwrap();
    register_workspace(&state, "ws-1", ws_dir.path(), WorkspaceLoadStatus::Ready).await;

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/dashboard/stats").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["memories"]["total_checkpoints"], 0);
    assert!(json["memories"]["active_plan"].is_null());
    assert!(json["memories"]["last_checkpoint"].is_null());
}

#[tokio::test]
async fn test_dashboard_stats_agent_dispatches() {
    let tmp = tempfile::tempdir().unwrap();
    let state = test_state(tmp.path().to_path_buf());

    // Start some dispatches
    {
        let mut dm = state.dispatch_manager.write().await;
        dm.start_dispatch("Fix bug #42".to_string(), "project-a".to_string());
        dm.start_dispatch("Add feature".to_string(), "project-b".to_string());
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
    let state = Arc::new(AppState {
        start_time: Instant::now(),
        registry: Arc::new(tokio::sync::RwLock::new(GlobalRegistry::new())),
        julie_home: tmp.path().to_path_buf(),
        daemon_state: Arc::new(tokio::sync::RwLock::new(DaemonState::new())),
        cancellation_token: CancellationToken::new(),
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
async fn test_dashboard_stats_memory_no_active_plan() {
    let tmp = tempfile::tempdir().unwrap();
    let state = test_state(tmp.path().to_path_buf());

    let ws_dir = tempfile::tempdir().unwrap();
    register_workspace(&state, "ws-1", ws_dir.path(), WorkspaceLoadStatus::Ready).await;

    // Write a checkpoint but no active plan
    let cp = make_checkpoint("checkpoint_dddd4444", "2026-03-07T09:00:00Z", "Solo checkpoint");
    write_test_checkpoint(ws_dir.path(), &cp);

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/dashboard/stats").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["memories"]["total_checkpoints"], 1);
    assert!(json["memories"]["active_plan"].is_null());
    assert_eq!(json["memories"]["last_checkpoint"], "2026-03-07T09:00:00Z");
}

#[tokio::test]
async fn test_dashboard_stats_all_sections_present() {
    // Verify the response shape always has all 4 top-level keys
    let tmp = tempfile::tempdir().unwrap();
    let state = test_state(tmp.path().to_path_buf());
    let app = test_app(state);

    let (status, json) = get_json(app, "/api/dashboard/stats").await;

    assert_eq!(status, StatusCode::OK);
    assert!(json.get("projects").is_some(), "missing 'projects' key");
    assert!(json.get("memories").is_some(), "missing 'memories' key");
    assert!(json.get("agents").is_some(), "missing 'agents' key");
    assert!(json.get("backends").is_some(), "missing 'backends' key");
}
