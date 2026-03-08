//! Tests for the memories and plans REST API endpoints.
//!
//! Tests cover:
//! - `GET /api/memories` — list/search checkpoints
//! - `GET /api/memories/:id` — get single checkpoint by ID prefix
//! - `GET /api/plans` — list plans
//! - `GET /api/plans/:id` — get single plan
//! - `GET /api/plans/active` — get active plan (or 404)

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt; // for `oneshot`

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

/// Create a fresh AppState for memory API tests.
fn test_state(julie_home: std::path::PathBuf) -> Arc<AppState> {
    let (indexing_sender, _rx) = tokio::sync::mpsc::channel::<IndexRequest>(1);
    Arc::new(AppState {
        start_time: Instant::now(),
        registry: Arc::new(tokio::sync::RwLock::new(GlobalRegistry::new())),
        julie_home,
        daemon_state: Arc::new(tokio::sync::RwLock::new(DaemonState::new())),
        cancellation_token: CancellationToken::new(),
        indexing_sender,
    })
}

/// Build a test app with API routes.
fn test_app(state: Arc<AppState>) -> axum::Router {
    axum::Router::new().nest("/api", api::routes(state))
}

/// Register a workspace whose `path` points at the given temp dir.
async fn register_workspace(state: &Arc<AppState>, temp_dir: &std::path::Path) -> String {
    let ws_id = "mem-test-ws".to_string();

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
        status: WorkspaceLoadStatus::Ready,
        path: temp_dir.to_path_buf(),
    };

    let mut ds = state.daemon_state.write().await;
    ds.workspaces.insert(ws_id.clone(), loaded_ws);

    ws_id
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
        summary: Some("Test checkpoint summary".to_string()),
        plan_id: None,
    }
}

/// Write a plan file into `.memories/plans/` under the given root.
fn write_test_plan(root: &std::path::Path, id: &str, title: &str, status: &str) {
    let plans_dir = root.join(".memories").join("plans");
    std::fs::create_dir_all(&plans_dir).unwrap();

    let content = format!(
        "---\nid: {id}\ntitle: {title}\nstatus: {status}\ncreated: \"2026-03-07T10:00:00.000Z\"\nupdated: \"2026-03-07T10:00:00.000Z\"\ntags:\n  - test\n---\n\nPlan content for {title}.\n"
    );
    std::fs::write(plans_dir.join(format!("{}.md", id)), content).unwrap();
}

/// Write the `.active-plan` file.
fn write_active_plan(root: &std::path::Path, plan_id: &str) {
    let memories_dir = root.join(".memories");
    std::fs::create_dir_all(&memories_dir).unwrap();
    std::fs::write(memories_dir.join(".active-plan"), plan_id).unwrap();
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

// ===========================================================================
// GET /api/memories — list checkpoints
// ===========================================================================

#[tokio::test]
async fn test_list_memories_returns_checkpoints() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    register_workspace(&state, temp_dir.path()).await;

    // Write a checkpoint
    let cp = make_checkpoint(
        "checkpoint_abcd1234",
        "2026-03-07T14:30:00.000Z",
        "Added auth middleware",
    );
    write_test_checkpoint(temp_dir.path(), &cp);

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/memories").await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["checkpoints"].is_array());
    assert!(json["checkpoints"].as_array().unwrap().len() > 0);

    let first = &json["checkpoints"][0];
    assert_eq!(first["id"], "checkpoint_abcd1234");
    assert!(first["timestamp"].is_string());
}

#[tokio::test]
async fn test_list_memories_empty_when_no_checkpoints() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    register_workspace(&state, temp_dir.path()).await;

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/memories").await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["checkpoints"].is_array());
    assert_eq!(json["checkpoints"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_list_memories_with_limit() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    register_workspace(&state, temp_dir.path()).await;

    // Write 3 checkpoints
    for i in 0..3 {
        let cp = make_checkpoint(
            &format!("checkpoint_{:04x}abcd", i),
            &format!("2026-03-07T14:{:02}:00.000Z", 30 + i),
            &format!("Checkpoint {}", i),
        );
        write_test_checkpoint(temp_dir.path(), &cp);
    }

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/memories?limit=1").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["checkpoints"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_list_memories_with_since_filter() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    register_workspace(&state, temp_dir.path()).await;

    // Write a checkpoint with a specific date
    let cp = make_checkpoint(
        "checkpoint_abcd1234",
        "2026-03-07T14:30:00.000Z",
        "Recent checkpoint",
    );
    write_test_checkpoint(temp_dir.path(), &cp);

    let app = test_app(state);
    // "3d" = last 3 days — should include the checkpoint
    let (status, json) = get_json(app, "/api/memories?since=3d").await;

    assert_eq!(status, StatusCode::OK);
    // The recall function should process without error
    assert!(json["checkpoints"].is_array());
}

#[tokio::test]
async fn test_list_memories_with_plan_id_filter() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    register_workspace(&state, temp_dir.path()).await;

    // Write checkpoint with a planId
    let mut cp = make_checkpoint(
        "checkpoint_abcd1234",
        "2026-03-07T14:30:00.000Z",
        "Checkpoint with plan",
    );
    cp.plan_id = Some("my-plan".to_string());
    write_test_checkpoint(temp_dir.path(), &cp);

    // Write checkpoint without planId
    let cp2 = make_checkpoint(
        "checkpoint_ef011234",
        "2026-03-07T14:31:00.000Z",
        "Checkpoint without plan",
    );
    write_test_checkpoint(temp_dir.path(), &cp2);

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/memories?planId=my-plan").await;

    assert_eq!(status, StatusCode::OK);
    let checkpoints = json["checkpoints"].as_array().unwrap();
    // Only the checkpoint with planId should be returned
    assert_eq!(checkpoints.len(), 1);
    assert_eq!(checkpoints[0]["id"], "checkpoint_abcd1234");
}

#[tokio::test]
async fn test_list_memories_no_workspace_returns_404() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    // No workspace registered

    let app = test_app(state);
    let (status, _json) = get_json(app, "/api/memories").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ===========================================================================
// GET /api/memories/:id — single checkpoint by ID
// ===========================================================================

#[tokio::test]
async fn test_get_memory_by_id() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    register_workspace(&state, temp_dir.path()).await;

    let cp = make_checkpoint(
        "checkpoint_abcd1234",
        "2026-03-07T14:30:00.000Z",
        "Found the auth bug",
    );
    write_test_checkpoint(temp_dir.path(), &cp);

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/memories/checkpoint_abcd1234").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["id"], "checkpoint_abcd1234");
    assert_eq!(json["description"], "Found the auth bug");
}

#[tokio::test]
async fn test_get_memory_by_id_prefix() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    register_workspace(&state, temp_dir.path()).await;

    let cp = make_checkpoint(
        "checkpoint_abcd1234",
        "2026-03-07T14:30:00.000Z",
        "Found the auth bug",
    );
    write_test_checkpoint(temp_dir.path(), &cp);

    let app = test_app(state);
    // Should match on prefix "abcd"
    let (status, json) = get_json(app, "/api/memories/abcd").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["id"], "checkpoint_abcd1234");
}

#[tokio::test]
async fn test_get_memory_not_found() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    register_workspace(&state, temp_dir.path()).await;

    let app = test_app(state);
    let (status, _json) = get_json(app, "/api/memories/nonexistent").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ===========================================================================
// GET /api/plans — list plans
// ===========================================================================

#[tokio::test]
async fn test_list_plans_returns_plans() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    register_workspace(&state, temp_dir.path()).await;

    write_test_plan(temp_dir.path(), "plan-alpha", "Alpha Plan", "active");
    write_test_plan(temp_dir.path(), "plan-beta", "Beta Plan", "completed");

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/plans").await;

    assert_eq!(status, StatusCode::OK);
    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_list_plans_with_status_filter() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    register_workspace(&state, temp_dir.path()).await;

    write_test_plan(temp_dir.path(), "plan-alpha", "Alpha Plan", "active");
    write_test_plan(temp_dir.path(), "plan-beta", "Beta Plan", "completed");

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/plans?status=active").await;

    assert_eq!(status, StatusCode::OK);
    assert!(json.is_array());
    let plans = json.as_array().unwrap();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0]["id"], "plan-alpha");
}

#[tokio::test]
async fn test_list_plans_empty() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    register_workspace(&state, temp_dir.path()).await;

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/plans").await;

    assert_eq!(status, StatusCode::OK);
    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 0);
}

// ===========================================================================
// GET /api/plans/:id — single plan
// ===========================================================================

#[tokio::test]
async fn test_get_plan_by_id() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    register_workspace(&state, temp_dir.path()).await;

    write_test_plan(temp_dir.path(), "plan-alpha", "Alpha Plan", "active");

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/plans/plan-alpha").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["id"], "plan-alpha");
    assert_eq!(json["title"], "Alpha Plan");
    assert_eq!(json["status"], "active");
}

#[tokio::test]
async fn test_get_plan_not_found() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    register_workspace(&state, temp_dir.path()).await;

    let app = test_app(state);
    let (status, _json) = get_json(app, "/api/plans/nonexistent").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ===========================================================================
// GET /api/plans/active — active plan
// ===========================================================================

#[tokio::test]
async fn test_get_active_plan() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    register_workspace(&state, temp_dir.path()).await;

    write_test_plan(temp_dir.path(), "plan-alpha", "Alpha Plan", "active");
    write_active_plan(temp_dir.path(), "plan-alpha");

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/plans/active").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["id"], "plan-alpha");
    assert_eq!(json["title"], "Alpha Plan");
}

#[tokio::test]
async fn test_get_active_plan_returns_404_when_none() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    register_workspace(&state, temp_dir.path()).await;

    // No active plan set
    let app = test_app(state);
    let (status, _json) = get_json(app, "/api/plans/active").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_active_plan_no_workspace_returns_404() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    // No workspace registered

    let app = test_app(state);
    let (status, _json) = get_json(app, "/api/plans/active").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ===========================================================================
// GET /api/memories?search=... — Tantivy search mode
// ===========================================================================

#[tokio::test]
async fn test_list_memories_search_mode() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    register_workspace(&state, temp_dir.path()).await;

    // Write a checkpoint about auth
    let cp = make_checkpoint(
        "checkpoint_abcd1234",
        "2026-03-07T14:30:00.000Z",
        "Added authentication middleware for the API gateway",
    );
    write_test_checkpoint(temp_dir.path(), &cp);

    let app = test_app(state);
    // search param triggers Tantivy search mode in recall()
    let (status, json) = get_json(app, "/api/memories?search=auth").await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["checkpoints"].is_array());
    // The search uses Tantivy which may or may not find results depending on
    // whether the index was built. We just verify the endpoint works without error.
}

// ===========================================================================
// Project param routing
// ===========================================================================

#[tokio::test]
async fn test_memories_with_project_param() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    let ws_id = register_workspace(&state, temp_dir.path()).await;

    let cp = make_checkpoint(
        "checkpoint_abcd1234",
        "2026-03-07T14:30:00.000Z",
        "Test checkpoint",
    );
    write_test_checkpoint(temp_dir.path(), &cp);

    let app = test_app(state);
    let uri = format!("/api/memories?project={}", ws_id);
    let (status, json) = get_json(app, &uri).await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["checkpoints"].as_array().unwrap().len() > 0);
}

#[tokio::test]
async fn test_memories_unknown_project_returns_404() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    register_workspace(&state, temp_dir.path()).await;

    let app = test_app(state);
    let (status, _json) = get_json(app, "/api/memories?project=nonexistent").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}
