use std::sync::Arc;
use std::sync::RwLock;
use std::time::Instant;

use crate::dashboard::state::{DashboardDaemonPhase, DashboardEvent, DashboardState};
use crate::health::{HealthLevel, SystemStatus};
use crate::registry::database::DaemonDatabase;
use crate::registry::lifecycle::{LifecyclePhase, ShutdownCause};

#[tokio::test]
async fn test_dashboard_health_snapshot_reports_ready_state() {
    let temp_dir = tempfile::tempdir().unwrap();
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

    let state = DashboardState::new(
        Some(daemon_db),
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        50,
    );

    let health = state.health_snapshot().await;

    assert_eq!(health.overall, HealthLevel::Ready);
    assert_eq!(health.control_plane.level, HealthLevel::Ready);
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

#[test]
fn test_dashboard_state_creation() {
    let state = DashboardState::new(
        None,
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        50,
    );

    assert!(state.error_entries().is_empty());
}

#[tokio::test]
async fn test_dashboard_broadcast_send_receive() {
    let state = DashboardState::new(
        None,
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
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
async fn test_dashboard_health_snapshot_reports_daemon_phase() {
    let temp_dir = tempfile::tempdir().unwrap();
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

    let state = DashboardState::new(
        Some(daemon_db),
        Arc::clone(&daemon_phase),
        Instant::now(),
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
}

#[tokio::test]
async fn test_dashboard_health_snapshot_reports_detached_projection_contract() {
    let temp_dir = tempfile::tempdir().unwrap();
    let daemon_db =
        Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).expect("open daemon.db"));
    daemon_db
        .upsert_workspace("ready-a", "/proj/a", "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats("ready-a", 2, 1, None, None, None)
        .unwrap();

    let state = DashboardState::new(
        Some(daemon_db),
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        50,
    );

    let health = state.health_snapshot().await;
    assert_eq!(health.data_plane.projections.len(), 2);
    assert_eq!(health.data_plane.projections[0].name, "tantivy");
    assert_eq!(health.data_plane.projections[1].name, "web_edges");
    for projection in &health.data_plane.projections {
        assert_eq!(projection.level, HealthLevel::Unavailable);
        assert!(!projection.repair_needed);
        assert!(projection.workspace_id.is_none());
        assert!(projection.detail.contains("workspace pool is detached"));
    }
}
