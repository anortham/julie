use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::lifecycle::LifecyclePhase;
use crate::daemon::session::SessionTracker;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::dashboard::state::DashboardState;
use crate::dashboard::{DashboardConfig, create_router};
use crate::database::types::FileInfo;
use crate::extractors::{Symbol, SymbolKind};
use crate::search::SearchProjection;
use crate::tools::workspace::indexing::state::{
    IndexingOperation, IndexingRepairReason, IndexingStage,
};
use crate::workspace::registry::generate_workspace_id;

fn test_state() -> DashboardState {
    DashboardState::new(
        Arc::new(SessionTracker::new()),
        None,
        Arc::new(AtomicBool::new(false)),
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        None, // no embedding service in tests
        None,
        50,
    )
}

fn test_state_with_db() -> (DashboardState, tempfile::TempDir) {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let daemon_db =
        Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).expect("open daemon.db"));
    daemon_db
        .upsert_workspace("ready-a", "/proj/a", "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats("ready-a", 10, 1, None, None, None)
        .unwrap();

    let state = DashboardState::new(
        Arc::new(SessionTracker::new()),
        Some(daemon_db),
        Arc::new(AtomicBool::new(false)),
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        None,
        None,
        50,
    );

    (state, temp_dir)
}

fn make_file(path: &str, content: &str) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: "rust".to_string(),
        hash: format!("hash_{path}"),
        size: content.len() as i64,
        last_modified: 1000,
        last_indexed: 0,
        symbol_count: 1,
        line_count: content.lines().count() as i32,
        content: Some(content.to_string()),
    }
}

fn make_symbol(id: &str, name: &str, file_path: &str) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 1,
        end_column: 24,
        start_byte: 0,
        end_byte: 24,
        signature: Some(format!("fn {}()", name)),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some(format!("fn {}() {{}}", name)),
        content_type: None,
    }
}

async fn state_with_projection_lag() -> (DashboardState, tempfile::TempDir, String) {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp_dir.path().join("workspace");
    std::fs::create_dir_all(&workspace_root).expect("workspace dir");
    let workspace_id =
        generate_workspace_id(&workspace_root.to_string_lossy()).expect("workspace id");

    let daemon_db =
        Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).expect("open daemon.db"));
    daemon_db
        .upsert_workspace(&workspace_id, &workspace_root.to_string_lossy(), "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats(&workspace_id, 2, 1, None, None, None)
        .unwrap();

    let pool = Arc::new(WorkspacePool::new(
        temp_dir.path().join("indexes"),
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));
    let workspace = pool
        .get_or_init(&workspace_id, workspace_root.clone())
        .await
        .expect("workspace init");

    {
        let mut db = workspace
            .db
            .as_ref()
            .expect("workspace db")
            .lock()
            .expect("db lock");
        db.bulk_store_fresh_atomic(
            &[make_file("src/lib.rs", "fn first_symbol() {}\n")],
            &[make_symbol("sym_1", "first_symbol", "src/lib.rs")],
            &[],
            &[],
            &[],
            &workspace_id,
        )
        .unwrap();

        let search_index = workspace
            .search_index
            .as_ref()
            .expect("search index")
            .lock()
            .expect("index lock");
        SearchProjection::tantivy(&workspace_id)
            .ensure_current_from_database(&mut db, &search_index)
            .unwrap();
        drop(search_index);

        db.incremental_update_atomic(
            &["src/lib.rs".to_string()],
            &[make_file("src/lib.rs", "fn second_symbol() {}\n")],
            &[make_symbol("sym_2", "second_symbol", "src/lib.rs")],
            &[],
            &[],
            &[],
            &workspace_id,
        )
        .unwrap();
    }

    (
        DashboardState::new(
            Arc::new(SessionTracker::new()),
            Some(daemon_db),
            Arc::new(AtomicBool::new(false)),
            Arc::new(RwLock::new(LifecyclePhase::Ready)),
            Instant::now(),
            None,
            Some(pool),
            50,
        ),
        temp_dir,
        workspace_id,
    )
}

#[tokio::test]
async fn test_all_dashboard_pages_return_200() {
    let state = test_state();
    let config = DashboardConfig::default();

    for path in ["/", "/projects", "/metrics", "/search"] {
        let app = create_router(state.clone(), config.clone()).unwrap();
        let response = app
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(
            response.status().as_u16(),
            200,
            "GET {} returned {}",
            path,
            response.status()
        );
    }
}

#[tokio::test]
async fn test_metrics_page_renders_aggregated_tool_history() {
    let (state, _tmp) = test_state_with_db();
    let daemon_db = state.daemon_db().expect("daemon db").clone();

    daemon_db
        .upsert_workspace("ready-b", "/proj/b", "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats("ready-b", 8, 1, None, None, None)
        .unwrap();

    daemon_db
        .insert_tool_call(
            "ready-a",
            "session-a",
            "fast_search",
            12.0,
            Some(3),
            Some(12_000),
            Some(800),
            true,
            None,
        )
        .unwrap();
    daemon_db
        .insert_tool_call(
            "ready-b",
            "session-b",
            "deep_dive",
            25.0,
            Some(1),
            Some(8_000),
            Some(1_200),
            false,
            None,
        )
        .unwrap();

    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics?days=7")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("All Workspaces"));
    assert!(html.contains("Success Rate"));
    assert!(html.contains("Context Saved"));
    assert!(html.contains("fast_search"));
    assert!(html.contains("deep_dive"));
}

#[tokio::test]
async fn test_metrics_table_filters_to_selected_workspace() {
    let (state, _tmp) = test_state_with_db();
    let daemon_db = state.daemon_db().expect("daemon db").clone();

    daemon_db
        .upsert_workspace("ready-b", "/proj/b", "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats("ready-b", 8, 1, None, None, None)
        .unwrap();

    daemon_db
        .insert_tool_call(
            "ready-a",
            "session-a",
            "fast_search",
            12.0,
            Some(3),
            Some(12_000),
            Some(800),
            true,
            None,
        )
        .unwrap();
    daemon_db
        .insert_tool_call(
            "ready-b",
            "session-b",
            "deep_dive",
            25.0,
            Some(1),
            Some(8_000),
            Some(1_200),
            true,
            None,
        )
        .unwrap();

    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics/table?days=7&workspace=ready-a")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("fast_search"));
    assert!(!html.contains("deep_dive"));
}

#[tokio::test]
async fn test_dashboard_static_files_served() {
    let state = test_state();
    let config = DashboardConfig::default();

    // Test app.css
    let app = create_router(state.clone(), config.clone()).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/static/app.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status().as_u16(),
        200,
        "GET /static/app.css returned {}",
        response.status()
    );
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("text/css"),
        "expected text/css content-type for app.css, got: {content_type}"
    );

    // Test app.js
    let app = create_router(state, config).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/static/app.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status().as_u16(),
        200,
        "GET /static/app.js returned {}",
        response.status()
    );
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("javascript"),
        "expected javascript content-type for app.js, got: {content_type}"
    );
}

#[tokio::test]
async fn test_dashboard_404_for_missing_static() {
    let state = test_state();
    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/static/nonexistent.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status().as_u16(),
        404,
        "GET /static/nonexistent.js should return 404, got {}",
        response.status()
    );
}

#[tokio::test]
async fn test_dashboard_post_search_returns_200() {
    let state = test_state();
    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/search")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("query=test&search_target=definitions"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn test_project_detail_returns_404_without_daemon_db() {
    // With no daemon_db, the detail endpoint should return 404
    // (daemon_db is None, so get_workspace returns NotFound)
    let state = test_state();
    let config = DashboardConfig::default();
    let router = create_router(state, config).unwrap();

    let request = Request::builder()
        .uri("/projects/test_workspace/detail")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    // 404 because daemon_db is None
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_status_live_exposes_nested_health_snapshot() {
    let (state, _temp_dir) = test_state_with_db();
    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/status/live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["health"]["overall"], "ready");
    assert_eq!(json["health"]["control_plane"]["active_sessions"], 0);
    assert_eq!(json["health"]["control_plane"]["daemon_phase"], "ready");
    assert_eq!(
        json["health"]["control_plane"]["session_phases"]["connecting"],
        0
    );
    assert_eq!(json["health"]["data_plane"]["workspace_count"], 1);
    assert_eq!(json["health"]["data_plane"]["ready_workspace_count"], 1);
    assert_eq!(json["health"]["runtime_plane"]["configured"], false);
    assert_eq!(
        json["health"]["runtime_plane"]["embeddings"]["query_fallback"],
        "keyword-only"
    );
}

#[tokio::test]
async fn test_status_live_exposes_indexing_health_snapshot() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp_dir.path().join("workspace");
    std::fs::create_dir_all(&workspace_root).expect("workspace dir");
    let workspace_id =
        generate_workspace_id(&workspace_root.to_string_lossy()).expect("workspace id");

    let daemon_db =
        Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).expect("open daemon.db"));
    daemon_db
        .upsert_workspace(&workspace_id, &workspace_root.to_string_lossy(), "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats(&workspace_id, 10, 1, None, None, None)
        .unwrap();

    let pool = Arc::new(WorkspacePool::new(
        temp_dir.path().join("indexes"),
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));
    let workspace = pool
        .get_or_init(&workspace_id, workspace_root.clone())
        .await
        .expect("workspace init");
    {
        let mut indexing = workspace.indexing_runtime.write().unwrap();
        indexing.begin_operation(IndexingOperation::Incremental);
        indexing.transition_stage(IndexingStage::Projecting);
        indexing.set_catchup_active(true);
        indexing.set_watcher_paused(true);
        indexing.set_dirty_projection_count(2);
        indexing.record_repair_reason(IndexingRepairReason::ProjectionFailure);
    }

    let state = DashboardState::new(
        Arc::new(SessionTracker::new()),
        Some(daemon_db),
        Arc::new(AtomicBool::new(false)),
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        None,
        Some(pool),
        50,
    );
    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/status/live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        json["health"]["data_plane"]["indexing"]["active_operation"],
        "catch_up"
    );
    assert_eq!(
        json["health"]["data_plane"]["indexing"]["stage"],
        "projecting"
    );
    assert_eq!(
        json["health"]["data_plane"]["indexing"]["catchup_active"],
        true
    );
    assert_eq!(
        json["health"]["data_plane"]["indexing"]["watcher_paused"],
        true
    );
    assert_eq!(
        json["health"]["data_plane"]["indexing"]["dirty_projection_count"],
        2
    );
    assert_eq!(
        json["health"]["data_plane"]["indexing"]["repair_needed"],
        true
    );
    assert_eq!(
        json["health"]["data_plane"]["indexing"]["repair_reasons"][0],
        "projection_failure"
    );
}

#[tokio::test]
async fn test_status_live_exposes_projection_freshness_snapshot() {
    let (state, _temp_dir, workspace_id) = state_with_projection_lag().await;
    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/status/live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(
        json["health"]["data_plane"]["search_projection"]["workspace_id"],
        workspace_id
    );
    assert_eq!(
        json["health"]["data_plane"]["search_projection"]["state"],
        "ready"
    );
    assert_eq!(
        json["health"]["data_plane"]["search_projection"]["freshness"],
        "lagging"
    );
    assert_eq!(
        json["health"]["data_plane"]["search_projection"]["canonical_revision"],
        2
    );
    assert_eq!(
        json["health"]["data_plane"]["search_projection"]["projected_revision"],
        1
    );
    assert_eq!(
        json["health"]["data_plane"]["search_projection"]["revision_lag"],
        1
    );
    assert_eq!(
        json["health"]["data_plane"]["search_projection"]["repair_needed"],
        true
    );
}

#[tokio::test]
async fn test_status_page_renders_health_sections() {
    let (state, _temp_dir, _workspace_id) = state_with_projection_lag().await;
    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("Control Plane"));
    assert!(html.contains("Data Plane"));
    assert!(html.contains("Runtime Plane"));
    assert!(html.contains("Overall Health"));
    assert!(html.contains("Daemon Phase"));
    assert!(html.contains("Restart Required"));
    assert!(html.contains("Session Phases"));
    assert!(html.contains("Projection Freshness"));
    assert!(html.contains("Canonical / Projected Revision"));
    assert!(html.contains("Projection Revision Lag"));
    assert!(html.contains("Indexing Operation"));
    assert!(html.contains("Dirty Projection Entries"));
    assert!(html.contains("Repair Reasons"));
    assert!(html.contains("formatUpperValue(d.health.runtime_plane.embeddings.state)"));
    assert!(
        !html.contains("formatUpper(d.health.runtime_plane.embeddings.state)"),
        "runtime state poller should call the defined formatter helper"
    );
}
