use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use crate::daemon::embedding_service::EmbeddingService;
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
#[test]
fn test_dashboard_state_embedding_unavailable_with_runtime_status() {
    let sessions = Arc::new(SessionTracker::new());
    let restart_pending = Arc::new(AtomicBool::new(false));

    let service = Arc::new(EmbeddingService::initializing());
    let state = DashboardState::new(
        Arc::clone(&sessions),
        None,
        restart_pending,
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
async fn test_dashboard_broadcast_send_receive() {
    let sessions = Arc::new(SessionTracker::new());
    let restart_pending = Arc::new(AtomicBool::new(false));
    let state = DashboardState::new(
        sessions,
        None,
        restart_pending,
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
