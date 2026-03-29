use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use crate::daemon::session::SessionTracker;
use crate::dashboard::state::{DashboardEvent, DashboardState};

#[test]
fn test_dashboard_state_creation() {
    let sessions = Arc::new(SessionTracker::new());
    let restart_pending = Arc::new(AtomicBool::new(false));
    let state = DashboardState::new(
        sessions,
        None,
        restart_pending,
        Instant::now(),
        false,
        None,
        50,
    );

    assert_eq!(state.sessions().active_count(), 0);
    assert!(!state.is_restart_pending());
    assert!(state.error_entries().is_empty());
    assert!(!state.embedding_available());
    assert!(state.workspace_pool().is_none());
}

#[tokio::test]
async fn test_dashboard_broadcast_send_receive() {
    let sessions = Arc::new(SessionTracker::new());
    let restart_pending = Arc::new(AtomicBool::new(false));
    let state = DashboardState::new(
        sessions,
        None,
        restart_pending,
        Instant::now(),
        true,
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
