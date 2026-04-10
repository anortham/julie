use crate::daemon::embedding_service::{EmbeddingService, EmbeddingServiceSettled};
use crate::handler::JulieServerHandler;
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_embedding_service_provider_returns_none_when_unavailable() {
    let service = EmbeddingService::initialize_for_test(None);
    assert!(!service.is_available());
    assert!(service.provider().is_none());
}

#[test]
fn test_embedding_service_shutdown_is_safe_when_no_provider() {
    let service = EmbeddingService::initialize_for_test(None);
    service.shutdown(); // should not panic
}

#[tokio::test]
async fn test_handler_embedding_provider_returns_none_in_stdio_mode() {
    let handler = JulieServerHandler::new_for_test()
        .await
        .expect("new_for_test should succeed");
    // Stdio mode: no embedding_service, workspace not initialized yet
    assert!(
        handler.embedding_provider().await.is_none(),
        "stdio mode handler should have no embedding provider"
    );
    assert!(
        handler.embedding_runtime_status().await.is_none(),
        "stdio mode handler should have no embedding runtime status"
    );
}

/// Daemon-mode integration: when the handler holds an `EmbeddingService` in
/// `Initializing` and a background task publishes `Ready` mid-wait, callers
/// using `wait_until_settled` (e.g. `spawn_workspace_embedding`,
/// `maybe_initialize_embeddings_for_nl_definitions`) observe `Ready` and
/// receive the published provider. This is the daemon warmup happy path.
///
/// Scope note: this exercises the handler+service integration pattern that
/// downstream callers depend on. Full end-to-end coverage of
/// `spawn_workspace_embedding` (which also opens a real workspace database
/// and spawns the embedding pipeline) is provided by the Task 7 integration
/// test, which has the necessary tempdir/db scaffolding.
#[tokio::test]
async fn test_handler_daemon_mode_wait_until_settled_publishes_ready() {
    let mut handler = JulieServerHandler::new_for_test()
        .await
        .expect("new_for_test should succeed");

    // Inject a fresh service in Initializing state. Simulates the daemon
    // having started up and assigned the shared service to a session
    // handler before the background init task has finished.
    let service = Arc::new(EmbeddingService::initializing());
    handler.embedding_service = Some(Arc::clone(&service));

    // Background publisher: waits 50ms then publishes a synthetic Ready.
    // This is the same race condition pattern Task 1 covers in unit tests
    // for the service itself; here we verify it works through a real
    // handler holding an Arc<EmbeddingService>.
    let publisher_service = Arc::clone(&service);
    let publisher = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let provider: Arc<dyn crate::embeddings::EmbeddingProvider> =
            Arc::new(NoopProvider::default());
        let status = crate::embeddings::EmbeddingRuntimeStatus {
            requested_backend: crate::embeddings::EmbeddingBackend::Unresolved,
            resolved_backend: crate::embeddings::EmbeddingBackend::Unresolved,
            accelerated: false,
            degraded_reason: None,
        };
        publisher_service.publish_ready(provider, status);
    });

    // The waiter is the path spawn_workspace_embedding takes when the
    // shared service is in Initializing.
    let svc = handler
        .embedding_service
        .as_ref()
        .expect("handler should have embedding_service set")
        .clone();
    let outcome = svc.wait_until_settled(Duration::from_secs(1)).await;
    publisher.await.expect("publisher task should not panic");

    match outcome {
        EmbeddingServiceSettled::Ready(_) => {
            // Verify the provider is now visible through handler.embedding_provider()
            let provider = handler.embedding_provider().await;
            assert!(
                provider.is_some(),
                "handler.embedding_provider() should return the published provider"
            );
        }
        other => panic!("expected Ready, got: {}", describe_settled(&other)),
    }
}

/// Daemon-mode integration: when the shared service publishes `Unavailable`
/// (e.g. provider init failed or was disabled), callers waiting via
/// `wait_until_settled` get `Unavailable` and can degrade gracefully without
/// hanging. This is the daemon warmup failure path.
#[tokio::test]
async fn test_handler_daemon_mode_wait_until_settled_publishes_unavailable() {
    let mut handler = JulieServerHandler::new_for_test()
        .await
        .expect("new_for_test should succeed");

    let service = Arc::new(EmbeddingService::initializing());
    handler.embedding_service = Some(Arc::clone(&service));

    let publisher_service = Arc::clone(&service);
    let publisher = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        publisher_service.publish_unavailable("test: model load failed".to_string(), None);
    });

    let svc = handler
        .embedding_service
        .as_ref()
        .expect("handler should have embedding_service set")
        .clone();
    let outcome = svc.wait_until_settled(Duration::from_secs(1)).await;
    publisher.await.expect("publisher task should not panic");

    match outcome {
        EmbeddingServiceSettled::Unavailable(reason) => {
            assert_eq!(reason, "test: model load failed");
            // Provider remains None after Unavailable
            assert!(
                handler.embedding_provider().await.is_none(),
                "handler.embedding_provider() should return None after Unavailable"
            );
        }
        other => panic!("expected Unavailable, got: {}", describe_settled(&other)),
    }
}

// ---- test helpers ----

fn describe_settled(s: &EmbeddingServiceSettled) -> &'static str {
    match s {
        EmbeddingServiceSettled::Ready(_) => "Ready",
        EmbeddingServiceSettled::Unavailable(_) => "Unavailable",
        EmbeddingServiceSettled::Timeout => "Timeout",
    }
}

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
