use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::embedding_service::EmbeddingService;
use crate::daemon::lifecycle::{LifecyclePhase, ShutdownCause};
use crate::daemon::session::{SessionLifecyclePhase, SessionTracker};
use crate::daemon::workspace_pool::WorkspacePool;
use crate::dashboard::state::{DashboardDaemonPhase, DashboardEvent, DashboardState};
use crate::database::types::FileInfo;
use crate::extractors::{Symbol, SymbolKind};
use crate::health::{HealthLevel, ProjectionFreshness, ProjectionState, SystemStatus};
use crate::search::SearchProjection;
use crate::workspace::registry::generate_workspace_id;

#[tokio::test]
async fn test_dashboard_health_snapshot_reports_ready_state() {
    let temp_dir = tempfile::tempdir().unwrap();
    let sessions = Arc::new(SessionTracker::new());
    let restart_pending = Arc::new(AtomicBool::new(false));
    let daemon_db =
        Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).expect("open daemon.db"));

    daemon_db
        .upsert_workspace("ready-a", "/proj/a", "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats("ready-a", 120, 12, None, None, None)
        .unwrap();
    daemon_db.increment_session_count("ready-a").unwrap();
    daemon_db.increment_session_count("ready-a").unwrap();

    daemon_db
        .upsert_workspace("ready-b", "/proj/b", "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats("ready-b", 80, 8, None, None, None)
        .unwrap();

    sessions.add_session();
    sessions.add_session();

    let state = DashboardState::new(
        Arc::clone(&sessions),
        Some(daemon_db),
        Arc::clone(&restart_pending),
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        None,
        None,
        50,
    );

    let health = state.health_snapshot().await;

    assert_eq!(health.overall, HealthLevel::Ready);
    assert_eq!(health.control_plane.level, HealthLevel::Ready);
    assert_eq!(health.control_plane.active_sessions, 2);
    assert!(!health.control_plane.restart_pending);
    assert_eq!(health.data_plane.level, HealthLevel::Ready);
    assert_eq!(health.data_plane.workspace_count, 2);
    assert_eq!(health.data_plane.active_workspace_count, 1);
    assert_eq!(health.data_plane.session_count, 2);
    assert_eq!(health.data_plane.ready_workspace_count, 2);
    assert_eq!(health.data_plane.pending_workspace_count, 0);
    assert_eq!(health.data_plane.other_workspace_count, 0);
    assert_eq!(health.data_plane.symbol_count, 200);
    assert_eq!(health.data_plane.file_count, 20);
    assert_eq!(
        health.data_plane.readiness,
        SystemStatus::FullyReady { symbol_count: 200 }
    );
    assert_eq!(health.runtime_plane.level, HealthLevel::Unavailable);
    assert!(!health.runtime_plane.configured);
}

#[tokio::test]
async fn test_dashboard_health_snapshot_reports_restart_pending_and_embedding_degraded() {
    let temp_dir = tempfile::tempdir().unwrap();
    let sessions = Arc::new(SessionTracker::new());
    let restart_pending = Arc::new(AtomicBool::new(true));
    let service = Arc::new(EmbeddingService::initializing());

    let daemon_db =
        Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).expect("open daemon.db"));
    daemon_db
        .upsert_workspace("ready-a", "/proj/a", "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats("ready-a", 42, 4, None, None, None)
        .unwrap();

    sessions.add_session();

    let state = DashboardState::new(
        Arc::clone(&sessions),
        Some(daemon_db),
        Arc::clone(&restart_pending),
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        Some(service),
        None,
        50,
    );

    let health = state.health_snapshot().await;

    assert_eq!(health.control_plane.level, HealthLevel::Degraded);
    assert!(health.control_plane.restart_pending);
    assert_eq!(health.data_plane.level, HealthLevel::Ready);
    assert_eq!(health.runtime_plane.level, HealthLevel::Degraded);
    assert!(health.runtime_plane.embedding_initializing);
    assert_eq!(
        health.runtime_plane.embeddings.state,
        crate::health::EmbeddingState::Initializing
    );
    assert_eq!(health.runtime_plane.embeddings.query_fallback, "pending");
    assert_eq!(health.overall, HealthLevel::Degraded);
}

#[test]
fn test_dashboard_state_creation() {
    let sessions = Arc::new(SessionTracker::new());
    let restart_pending = Arc::new(AtomicBool::new(false));
    let state = DashboardState::new(
        sessions,
        None,
        restart_pending,
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        None, // no embedding service
        None,
        50,
    );

    assert_eq!(state.sessions().active_count(), 0);
    assert!(!state.is_restart_pending());
    assert!(state.error_entries().is_empty());
    assert!(!state.embedding_available());
    assert!(state.workspace_pool().is_none());
}

/// The whole point of Task 5: DashboardState should reflect the
/// EmbeddingService's state live, not snapshot it at construction time.
/// Build a state with the service in Initializing, assert
/// embedding_available is false, then call publish_ready on the underlying
/// service and assert embedding_available flips to true WITHOUT
/// reconstructing the DashboardState.
#[test]
fn test_dashboard_state_embedding_available_reflects_service_live() {
    let sessions = Arc::new(SessionTracker::new());
    let restart_pending = Arc::new(AtomicBool::new(false));

    // Construct service in Initializing and share the Arc with the dashboard.
    let service = Arc::new(EmbeddingService::initializing());
    let state = DashboardState::new(
        Arc::clone(&sessions),
        None,
        restart_pending,
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        Some(Arc::clone(&service)),
        None,
        50,
    );

    // Initial state: service is Initializing → not available
    assert!(
        !state.embedding_available(),
        "embedding_available should be false while service is Initializing"
    );
    assert!(
        state.embedding_runtime_status().is_none(),
        "runtime_status should be None while service is Initializing"
    );

    // Background init "completes" — publish a ready state with a fake provider.
    let provider: Arc<dyn crate::embeddings::EmbeddingProvider> = Arc::new(NoopProvider::default());
    let status = crate::embeddings::EmbeddingRuntimeStatus {
        requested_backend: crate::embeddings::EmbeddingBackend::Unresolved,
        resolved_backend: crate::embeddings::EmbeddingBackend::Unresolved,
        accelerated: false,
        degraded_reason: None,
    };
    service.publish_ready(provider, status);

    // Same DashboardState instance — but the live read should now see Ready.
    assert!(
        state.embedding_available(),
        "embedding_available should flip to true after publish_ready, without reconstructing DashboardState"
    );
    assert!(
        state.embedding_runtime_status().is_some(),
        "runtime_status should be Some after publish_ready"
    );
}

/// Symmetric test for the failure path: service publishes Unavailable
/// → embedding_available stays false, but runtime_status surfaces if
/// the publish carried one.
/// Dashboard must distinguish "Initializing" from "Not configured" so the
/// template can show a spinner instead of the misleading "Not configured".
#[test]
fn test_dashboard_state_embedding_initializing_reflects_service_lifecycle() {
    let sessions = Arc::new(SessionTracker::new());
    let restart_pending = Arc::new(AtomicBool::new(false));

    // No service at all → not initializing (it's "Not configured")
    let state_no_svc = DashboardState::new(
        Arc::clone(&sessions),
        None,
        Arc::clone(&restart_pending),
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        None,
        None,
        50,
    );
    assert!(
        !state_no_svc.embedding_initializing(),
        "no service → not initializing, it's not configured"
    );

    // Service in Initializing state → should report initializing
    let service = Arc::new(EmbeddingService::initializing());
    let state = DashboardState::new(
        Arc::clone(&sessions),
        None,
        Arc::clone(&restart_pending),
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        Some(Arc::clone(&service)),
        None,
        50,
    );
    assert!(
        state.embedding_initializing(),
        "service in Initializing state → embedding_initializing should be true"
    );
    assert!(
        !state.embedding_available(),
        "service in Initializing state → embedding_available should be false"
    );

    // Transition to Ready → no longer initializing
    let provider: Arc<dyn crate::embeddings::EmbeddingProvider> = Arc::new(NoopProvider::default());
    let status = crate::embeddings::EmbeddingRuntimeStatus {
        requested_backend: crate::embeddings::EmbeddingBackend::Unresolved,
        resolved_backend: crate::embeddings::EmbeddingBackend::Unresolved,
        accelerated: false,
        degraded_reason: None,
    };
    service.publish_ready(provider, status);
    assert!(
        !state.embedding_initializing(),
        "after publish_ready → embedding_initializing should be false"
    );
    assert!(
        state.embedding_available(),
        "after publish_ready → embedding_available should be true"
    );
}

#[test]
fn test_dashboard_state_embedding_unavailable_with_runtime_status() {
    let sessions = Arc::new(SessionTracker::new());
    let restart_pending = Arc::new(AtomicBool::new(false));

    let service = Arc::new(EmbeddingService::initializing());
    let state = DashboardState::new(
        Arc::clone(&sessions),
        None,
        restart_pending,
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        Some(Arc::clone(&service)),
        None,
        50,
    );

    let status = crate::embeddings::EmbeddingRuntimeStatus {
        requested_backend: crate::embeddings::EmbeddingBackend::Unresolved,
        resolved_backend: crate::embeddings::EmbeddingBackend::Unresolved,
        accelerated: false,
        degraded_reason: Some("test: backend resolver failed".to_string()),
    };
    service.publish_unavailable("test failure".to_string(), Some(status));

    assert!(
        !state.embedding_available(),
        "embedding_available should be false after Unavailable"
    );
    let runtime = state
        .embedding_runtime_status()
        .expect("runtime_status should surface from Unavailable");
    assert_eq!(
        runtime.degraded_reason.as_deref(),
        Some("test: backend resolver failed")
    );
}

#[tokio::test]
async fn test_dashboard_health_snapshot_surfaces_embedding_runtime_details() {
    let sessions = Arc::new(SessionTracker::new());
    let restart_pending = Arc::new(AtomicBool::new(false));
    let service = Arc::new(EmbeddingService::initializing());

    let state = DashboardState::new(
        Arc::clone(&sessions),
        None,
        restart_pending,
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        Some(Arc::clone(&service)),
        None,
        50,
    );

    let provider: Arc<dyn crate::embeddings::EmbeddingProvider> = Arc::new(NoopProvider::default());
    let status = crate::embeddings::EmbeddingRuntimeStatus {
        requested_backend: crate::embeddings::EmbeddingBackend::Auto,
        resolved_backend: crate::embeddings::EmbeddingBackend::Sidecar,
        accelerated: false,
        degraded_reason: Some("CPU only: no GPU detected".to_string()),
    };
    service.publish_ready(provider, status);

    let health = state.health_snapshot().await;

    assert_eq!(health.runtime_plane.embeddings.runtime, "test");
    assert_eq!(health.runtime_plane.embeddings.backend, "sidecar");
    assert_eq!(health.runtime_plane.embeddings.device, "test");
    assert!(!health.runtime_plane.embeddings.accelerated);
    assert_eq!(
        health.runtime_plane.embeddings.detail,
        "CPU only: no GPU detected"
    );
    assert_eq!(health.runtime_plane.embeddings.query_fallback, "semantic");
}

#[tokio::test]
async fn test_dashboard_broadcast_send_receive() {
    let sessions = Arc::new(SessionTracker::new());
    let restart_pending = Arc::new(AtomicBool::new(false));
    let state = DashboardState::new(
        sessions,
        None,
        restart_pending,
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        None, // embedding service not needed for broadcast test
        None,
        50,
    );

    let mut rx = state.subscribe();

    state.send_event(DashboardEvent::ToolCall {
        tool_name: "fast_search".to_string(),
        workspace: "primary".to_string(),
        duration_ms: 42.5,
    });

    let event: DashboardEvent = rx.recv().await.expect("expected an event");
    match event {
        DashboardEvent::ToolCall {
            tool_name,
            workspace,
            duration_ms,
        } => {
            assert_eq!(tool_name, "fast_search");
            assert_eq!(workspace, "primary");
            assert!((duration_ms - 42.5).abs() < f64::EPSILON);
        }
        other => panic!("unexpected event: {:?}", other),
    }
}

#[tokio::test]
async fn test_dashboard_health_snapshot_reports_daemon_and_session_phases() {
    let temp_dir = tempfile::tempdir().unwrap();
    let sessions = Arc::new(SessionTracker::new());
    let restart_pending = Arc::new(AtomicBool::new(true));
    let daemon_phase = Arc::new(RwLock::new(LifecyclePhase::Draining {
        cause: ShutdownCause::RestartRequired,
    }));
    let daemon_db =
        Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).expect("open daemon.db"));

    daemon_db
        .upsert_workspace("ready-a", "/proj/a", "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats("ready-a", 10, 1, None, None, None)
        .unwrap();

    let bound_session = sessions.add_session();
    let serving_session = sessions.add_session();
    sessions.set_phase(&bound_session, SessionLifecyclePhase::Bound);
    sessions.set_phase(&serving_session, SessionLifecyclePhase::Serving);

    let state = DashboardState::new(
        Arc::clone(&sessions),
        Some(daemon_db),
        Arc::clone(&restart_pending),
        Arc::clone(&daemon_phase),
        Instant::now(),
        None,
        None,
        50,
    );

    let health = state.health_snapshot().await;

    assert_eq!(
        health.control_plane.daemon_phase,
        DashboardDaemonPhase::Draining
    );
    assert_eq!(
        health.control_plane.shutdown_cause,
        Some(ShutdownCause::RestartRequired)
    );
    assert_eq!(health.control_plane.session_phases.connecting, 0);
    assert_eq!(health.control_plane.session_phases.bound, 1);
    assert_eq!(health.control_plane.session_phases.serving, 1);
    assert_eq!(health.control_plane.session_phases.closing, 0);
    assert!(health.control_plane.restart_pending);
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
        annotations: Vec::new(),
    }
}

#[tokio::test]
async fn test_dashboard_health_snapshot_reports_projection_revision_lag() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_root = temp_dir.path().join("workspace");
    std::fs::create_dir_all(&workspace_root).unwrap();
    let workspace_id = generate_workspace_id(&workspace_root.to_string_lossy()).unwrap();

    let sessions = Arc::new(SessionTracker::new());
    let restart_pending = Arc::new(AtomicBool::new(false));
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

    let state = DashboardState::new(
        Arc::clone(&sessions),
        Some(daemon_db),
        restart_pending,
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        None,
        Some(pool),
        50,
    );

    let health = state.health_snapshot().await;

    assert_eq!(
        health.data_plane.search_projection.workspace_id.as_deref(),
        Some(workspace_id.as_str())
    );
    assert_eq!(
        health.data_plane.search_projection.state,
        ProjectionState::Ready
    );
    assert_eq!(
        health.data_plane.search_projection.freshness,
        ProjectionFreshness::Lagging
    );
    assert_eq!(
        health.data_plane.search_projection.canonical_revision,
        Some(2)
    );
    assert_eq!(
        health.data_plane.search_projection.projected_revision,
        Some(1)
    );
    assert_eq!(health.data_plane.search_projection.revision_lag, Some(1));
    assert!(health.data_plane.search_projection.repair_needed);
}

// ---- test helpers ----

#[derive(Default)]
struct NoopProvider;

impl crate::embeddings::EmbeddingProvider for NoopProvider {
    fn embed_query(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(Vec::new())
    }

    fn embed_batch(&self, _texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(Vec::new())
    }

    fn dimensions(&self) -> usize {
        0
    }

    fn device_info(&self) -> crate::embeddings::DeviceInfo {
        crate::embeddings::DeviceInfo {
            runtime: "test".to_string(),
            device: "test".to_string(),
            model_name: "test-noop".to_string(),
            dimensions: 0,
        }
    }
}
