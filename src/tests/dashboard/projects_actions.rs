use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::lifecycle::{LifecyclePhase, ShutdownCause};
use crate::daemon::session::SessionTracker;
use crate::daemon::watcher_pool::WatcherPool;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::dashboard::routes::projects_actions::{
    cleanup_dashboard_anchor, dashboard_handler, disconnect_dashboard_attached_workspaces,
};
use crate::dashboard::state::DashboardState;
use crate::dashboard::{DashboardConfig, create_router};
use crate::workspace::registry::generate_workspace_id;

async fn body_to_string(body: Body) -> String {
    let bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .expect("body bytes");
    String::from_utf8(bytes.to_vec()).expect("utf8 body")
}

fn action_ready_state() -> (
    DashboardState,
    Arc<DaemonDatabase>,
    Arc<WorkspacePool>,
    tempfile::TempDir,
) {
    action_state_with_phase(LifecyclePhase::Ready, false)
}

fn action_state_with_phase(
    phase: LifecyclePhase,
    restart_pending: bool,
) -> (
    DashboardState,
    Arc<DaemonDatabase>,
    Arc<WorkspacePool>,
    tempfile::TempDir,
) {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let daemon_db =
        Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).expect("open daemon"));
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));
    let workspace_pool = Arc::new(WorkspacePool::new(
        temp_dir.path().join("indexes"),
        Some(Arc::clone(&daemon_db)),
    ));
    let sessions = Arc::new(SessionTracker::new());
    let state = DashboardState::new_with_watcher_pool(
        sessions,
        Some(Arc::clone(&daemon_db)),
        Arc::new(AtomicBool::new(restart_pending)),
        Arc::new(RwLock::new(phase)),
        Instant::now(),
        None,
        Some(watcher_pool),
        Some(Arc::clone(&workspace_pool)),
        50,
    );

    (state, daemon_db, workspace_pool, temp_dir)
}

fn action_state_without_daemon() -> (DashboardState, tempfile::TempDir) {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let state = DashboardState::new(
        Arc::new(SessionTracker::new()),
        None,
        Arc::new(AtomicBool::new(false)),
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        None,
        None,
        50,
    );
    (state, temp_dir)
}

fn write_workspace_source(path: &std::path::Path) {
    std::fs::create_dir_all(path.join("src")).expect("src dir");
    std::fs::write(
        path.join("src/lib.rs"),
        "pub fn dashboard_action_target() -> &'static str { \"ok\" }\n",
    )
    .expect("write source");
}

#[tokio::test]
async fn test_projects_page_shows_workspace_controls_and_cleanup_log() {
    let (state, daemon_db, _workspace_pool, temp_dir) = action_ready_state();
    let current_path = temp_dir.path().join("current-workspace");
    let active_path = temp_dir.path().join("active-workspace");
    let known_path = temp_dir.path().join("known-workspace");
    let stale_path = temp_dir.path().join("stale-workspace");
    let blocked_path = temp_dir.path().join("blocked-workspace");
    std::fs::create_dir_all(&current_path).expect("current path");
    std::fs::create_dir_all(&active_path).expect("active path");
    std::fs::create_dir_all(&known_path).expect("known path");
    std::fs::create_dir_all(&stale_path).expect("stale path");
    std::fs::create_dir_all(&blocked_path).expect("blocked path");

    daemon_db
        .upsert_workspace("current_ws", &current_path.to_string_lossy(), "ready")
        .unwrap();
    daemon_db
        .upsert_workspace("active_ws", &active_path.to_string_lossy(), "ready")
        .unwrap();
    daemon_db
        .upsert_workspace("known_ws", &known_path.to_string_lossy(), "ready")
        .unwrap();
    daemon_db
        .upsert_workspace("stale_ws", &stale_path.to_string_lossy(), "ready")
        .unwrap();
    daemon_db
        .upsert_workspace("blocked_ws", &blocked_path.to_string_lossy(), "ready")
        .unwrap();
    daemon_db.increment_session_count("current_ws").unwrap();
    daemon_db.increment_session_count("active_ws").unwrap();
    daemon_db.increment_session_count("blocked_ws").unwrap();
    daemon_db
        .insert_cleanup_event("gone_ws", "/tmp/gone", "auto_prune", "missing_path")
        .unwrap();
    std::fs::remove_dir_all(&stale_path).expect("remove stale path");
    std::fs::remove_dir_all(&blocked_path).expect("remove blocked path");

    let current_session = state.sessions().add_session();
    assert!(
        state
            .sessions()
            .set_current_workspace(&current_session, Some("current_ws".to_string())),
        "session should accept current workspace tracking"
    );

    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let html = body_to_string(response.into_body()).await;
    assert!(html.contains("Add Workspace"));
    assert!(html.contains("Recent Cleanup"));
    assert!(html.contains("name=\"csrf_token\""));
    assert!(html.contains("CURRENT"));
    assert!(html.contains("ACTIVE"));
    assert!(html.contains("KNOWN"));
    assert!(html.contains("STALE"));
    assert!(html.contains("BLOCKED"));
    assert!(html.contains("Cleanup Holds"));
    assert!(html.contains("1 active session(s) remain"));
    assert!(html.contains("auto prune"));
    assert!(html.contains("missing path"));
    assert!(html.contains("projects-table-shell"));
    assert!(html.contains("/projects/current_ws/refresh"));
    assert!(html.contains("/projects/stale_ws/open"));
    assert!(
        !html.contains("/projects/blocked_ws/open"),
        "blocked missing workspaces should not offer an inline prune/open action"
    );
    assert!(!html.contains("/projects/current_ws/delete"));
    assert!(
        !html.contains("Reference Workspaces"),
        "projects dashboard should drop the dead workspace-pairing panel"
    );
}

#[tokio::test]
async fn test_projects_page_keeps_row_actions_compact() {
    let (state, daemon_db, _workspace_pool, temp_dir) = action_ready_state();
    let workspace_root = temp_dir.path().join("compact-row-target");
    write_workspace_source(&workspace_root);
    let workspace_id = generate_workspace_id(&workspace_root.to_string_lossy()).unwrap();
    daemon_db
        .upsert_workspace(&workspace_id, &workspace_root.to_string_lossy(), "ready")
        .unwrap();

    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let html = body_to_string(response.into_body()).await;
    assert!(
        html.contains("projects-table-shell"),
        "projects table should render inside the responsive shell"
    );
    assert!(
        html.contains(&format!("action=\"/projects/{workspace_id}/refresh\"")),
        "healthy row should keep the quick refresh action inline"
    );
    assert!(
        !html.contains(&format!("action=\"/projects/{workspace_id}/delete\"")),
        "delete should stay out of the compact table row"
    );
}

#[tokio::test]
async fn test_projects_register_action_indexes_workspace_without_activating_it() {
    let (state, daemon_db, _workspace_pool, temp_dir) = action_ready_state();
    let workspace_root = temp_dir.path().join("register-target");
    write_workspace_source(&workspace_root);
    let csrf_token = state.action_csrf_token().to_string();

    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects/register")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "path={}&csrf_token={}",
                    workspace_root.to_string_lossy(),
                    csrf_token,
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let html = body_to_string(response.into_body()).await;
    assert!(html.contains("Workspace Registered"));

    let workspace_id = generate_workspace_id(&workspace_root.to_string_lossy()).unwrap();
    let row = daemon_db
        .get_workspace(&workspace_id)
        .unwrap()
        .expect("registered row");
    assert_eq!(row.status, "ready");
    assert_eq!(row.session_count, 0);
    assert!(row.symbol_count.unwrap_or(0) > 0);
}

#[tokio::test]
async fn test_projects_register_action_renders_tool_error_as_danger_notice() {
    let (state, temp_dir) = action_state_without_daemon();
    let workspace_root = temp_dir.path().join("register-target");
    write_workspace_source(&workspace_root);
    let csrf_token = state.action_csrf_token().to_string();

    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects/register")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "path={}&csrf_token={}",
                    workspace_root.to_string_lossy(),
                    csrf_token,
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let html = body_to_string(response.into_body()).await;
    assert!(html.contains("Workspace registration requires the workspace registry"));
    // Guard: the registry-unavailable error must not point users at the
    // `julie daemon` subcommand, which Phase 3d.2a removed.
    assert!(
        !html.contains("julie daemon"),
        "registry errors must not reference the removed `julie daemon` subcommand, html={html}"
    );
    assert!(
        html.contains("rgba(212, 70, 88, 0.35)"),
        "tool-level errors must render as danger notices, html={html}"
    );
}

#[tokio::test]
#[serial_test::serial(dashboard_cwd)]
async fn test_dashboard_handler_does_not_write_project_log_under_process_cwd() {
    let (state, _daemon_db, _workspace_pool, temp_dir) = action_ready_state();
    let cwd = temp_dir.path().join("process-cwd");
    std::fs::create_dir_all(&cwd).expect("create cwd");
    let old_cwd = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(&cwd).expect("switch cwd");

    let result = dashboard_handler(&crate::dashboard::AppState {
        dashboard: state.clone(),
        tera: std::sync::Arc::new(tokio::sync::RwLock::new(tera::Tera::default())),
        config: DashboardConfig::default(),
    })
    .await;

    std::env::set_current_dir(old_cwd).expect("restore cwd");
    let (handler, _anchor_dir, anchor_id) = result.expect("dashboard handler");
    disconnect_dashboard_attached_workspaces(&handler).await;
    cleanup_dashboard_anchor(
        &crate::dashboard::AppState {
            dashboard: state,
            tera: std::sync::Arc::new(tokio::sync::RwLock::new(tera::Tera::default())),
            config: DashboardConfig::default(),
        },
        &anchor_id,
    )
    .await;

    assert!(
        !cwd.join(".julie").exists(),
        "dashboard synthetic handler must not create project logs under process cwd"
    );
}

#[tokio::test]
async fn test_cleanup_dashboard_anchor_does_not_remove_paths_outside_indexes_dir() {
    let (state, _daemon_db, _workspace_pool, temp_dir) = action_ready_state();
    std::fs::create_dir_all(temp_dir.path().join("indexes")).expect("indexes dir");
    let outside = temp_dir.path().join("outside-anchor-target");
    std::fs::create_dir_all(&outside).expect("outside dir");
    std::fs::write(outside.join("keep.txt"), "keep").expect("outside marker");
    let app_state = crate::dashboard::AppState {
        dashboard: state,
        tera: std::sync::Arc::new(tokio::sync::RwLock::new(tera::Tera::default())),
        config: DashboardConfig::default(),
    };

    cleanup_dashboard_anchor(&app_state, "../outside-anchor-target").await;

    assert!(
        outside.join("keep.txt").exists(),
        "dashboard anchor cleanup must not follow path traversal outside indexes dir"
    );
}

#[tokio::test]
async fn test_projects_refresh_action_blocks_while_daemon_is_stopping() {
    let (state, daemon_db, _workspace_pool, _temp_dir) = action_state_with_phase(
        LifecyclePhase::Stopping {
            cause: ShutdownCause::RestartRequired,
        },
        true,
    );
    let csrf_token = state.action_csrf_token().to_string();

    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/projects/missing_ws/refresh")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("csrf_token={csrf_token}")))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let html = body_to_string(response.into_body()).await;
    assert!(html.contains("Workspace Action Blocked"));
    assert!(html.contains("daemon STOPPING"));
    assert!(
        !html.contains("Workspace not found"),
        "shutdown guard must short-circuit before workspace action dispatch"
    );
    assert!(
        daemon_db.list_workspaces().unwrap().is_empty(),
        "blocked dashboard action must not create registry rows"
    );
}

#[tokio::test]
async fn test_projects_open_action_warms_workspace_without_leaking_session_count() {
    let (state, daemon_db, _workspace_pool, temp_dir) = action_ready_state();
    let workspace_root = temp_dir.path().join("open-target");
    write_workspace_source(&workspace_root);
    let workspace_id = generate_workspace_id(&workspace_root.to_string_lossy()).unwrap();
    daemon_db
        .upsert_workspace(&workspace_id, &workspace_root.to_string_lossy(), "ready")
        .unwrap();
    let csrf_token = state.action_csrf_token().to_string();

    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/projects/{workspace_id}/open"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("csrf_token={csrf_token}")))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let html = body_to_string(response.into_body()).await;
    assert!(html.contains("Workspace Opened"));

    let row = daemon_db
        .get_workspace(&workspace_id)
        .unwrap()
        .expect("opened row");
    assert_eq!(row.session_count, 0);
}

#[tokio::test]
async fn test_projects_delete_action_removes_inactive_workspace() {
    let (state, daemon_db, _workspace_pool, temp_dir) = action_ready_state();
    let workspace_root = temp_dir.path().join("delete-target");
    write_workspace_source(&workspace_root);
    let workspace_id = generate_workspace_id(&workspace_root.to_string_lossy()).unwrap();
    daemon_db
        .upsert_workspace(&workspace_id, &workspace_root.to_string_lossy(), "ready")
        .unwrap();
    let index_dir = temp_dir
        .path()
        .join("indexes")
        .join(&workspace_id)
        .join("db");
    std::fs::create_dir_all(&index_dir).expect("index dir");
    std::fs::write(index_dir.join("symbols.db"), b"placeholder").expect("index file");
    let csrf_token = state.action_csrf_token().to_string();

    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/projects/{workspace_id}/delete"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("csrf_token={csrf_token}")))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let html = body_to_string(response.into_body()).await;
    assert!(html.contains("Workspace Removed Successfully"));
    assert!(
        daemon_db.get_workspace(&workspace_id).unwrap().is_none(),
        "delete should remove the registry row"
    );
}

#[tokio::test]
async fn test_projects_delete_action_rejects_bad_csrf_token() {
    let (state, daemon_db, _workspace_pool, temp_dir) = action_ready_state();
    let workspace_root = temp_dir.path().join("delete-bad-token");
    write_workspace_source(&workspace_root);
    let workspace_id = generate_workspace_id(&workspace_root.to_string_lossy()).unwrap();
    daemon_db
        .upsert_workspace(&workspace_id, &workspace_root.to_string_lossy(), "ready")
        .unwrap();

    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/projects/{workspace_id}/delete"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("csrf_token=wrong-token"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 403);
    let html = body_to_string(response.into_body()).await;
    assert!(html.contains("Workspace Action Blocked"));
    assert!(
        daemon_db.get_workspace(&workspace_id).unwrap().is_some(),
        "delete should refuse requests with the wrong action token"
    );
}

#[tokio::test]
async fn test_project_detail_shows_workspace_state_without_reference_section() {
    let (state, daemon_db, _workspace_pool, temp_dir) = action_ready_state();
    let workspace_root = temp_dir.path().join("detail-target");
    write_workspace_source(&workspace_root);
    let workspace_id = generate_workspace_id(&workspace_root.to_string_lossy()).unwrap();
    daemon_db
        .upsert_workspace(&workspace_id, &workspace_root.to_string_lossy(), "ready")
        .unwrap();
    daemon_db.increment_session_count(&workspace_id).unwrap();

    let session_id = state.sessions().add_session();
    assert!(
        state
            .sessions()
            .set_current_workspace(&session_id, Some(workspace_id.clone())),
        "session should accept current workspace tracking"
    );

    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/projects/{workspace_id}/detail"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let html = body_to_string(response.into_body()).await;
    assert!(html.contains("Workspace State"));
    assert!(html.contains("CURRENT"));
    assert!(html.contains(&format!("/projects/{workspace_id}/refresh")));
    assert!(html.contains(&format!("/projects/{workspace_id}/delete")));
    assert!(html.contains(&format!("/metrics?workspace={workspace_id}")));
    assert!(html.contains(&format!("/intelligence/{workspace_id}")));
    assert!(html.contains(&format!("/signals/{workspace_id}")));
    assert!(
        !html.contains("Reference Workspaces"),
        "detail panel should no longer render workspace-pairing tags"
    );
}

#[tokio::test]
async fn test_project_detail_shows_blocked_cleanup_reason_for_missing_active_workspace() {
    let (state, daemon_db, _workspace_pool, temp_dir) = action_ready_state();
    let workspace_root = temp_dir.path().join("blocked-detail-target");
    write_workspace_source(&workspace_root);
    let workspace_id = generate_workspace_id(&workspace_root.to_string_lossy()).unwrap();
    daemon_db
        .upsert_workspace(&workspace_id, &workspace_root.to_string_lossy(), "ready")
        .unwrap();
    daemon_db.increment_session_count(&workspace_id).unwrap();
    std::fs::remove_dir_all(&workspace_root).expect("remove workspace path");

    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/projects/{workspace_id}/detail"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let html = body_to_string(response.into_body()).await;
    assert!(html.contains("BLOCKED"));
    assert!(html.contains("Cleanup"));
    assert!(html.contains("1 active session(s) remain"));
    assert!(
        !html.contains(&format!("action=\"/projects/{workspace_id}/open\"")),
        "blocked detail should not offer prune/open while cleanup is held live"
    );
}
