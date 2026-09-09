use crate::embeddings::EmbeddingProvider;
use crate::handler::JulieServerHandler;
use crate::registry::embedding_service::{EmbeddingService, EmbeddingServiceSettled};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_embedding_service_provider_returns_none_when_unavailable() {
    let service = EmbeddingService::initialize_for_test(None);
    assert!(!service.is_available());
    assert!(service.provider().is_none());
    assert!(service.runtime_status().is_none());
    assert!(
        service.is_settled(),
        "None provider should settle to Unavailable"
    );
}

#[tokio::test]
async fn test_embedding_service_shutdown_is_safe_when_no_provider() {
    let service = EmbeddingService::initialize_for_test(None);
    service.shutdown().await; // should not panic
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
#[tokio::test]
async fn test_handler_daemon_mode_wait_until_settled_publishes_ready() {
    let mut handler = JulieServerHandler::new_for_test()
        .await
        .expect("new_for_test should succeed");

    let service = Arc::new(EmbeddingService::initializing());
    handler.embedding_service = Some(Arc::clone(&service));

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

    let svc = handler
        .embedding_service
        .as_ref()
        .expect("handler should have embedding_service set")
        .clone();
    let outcome = svc.wait_until_settled(Duration::from_secs(1)).await;
    publisher.await.expect("publisher task should not panic");

    match outcome {
        EmbeddingServiceSettled::Ready { runtime_status, .. } => {
            assert_eq!(
                runtime_status.resolved_backend.as_str(),
                "unresolved",
                "ready state should preserve runtime metadata"
            );
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
        EmbeddingServiceSettled::Unavailable {
            reason,
            runtime_status,
        } => {
            assert_eq!(reason, "test: model load failed");
            assert!(
                runtime_status.is_none(),
                "unavailable state should preserve optional runtime metadata"
            );
            assert!(
                handler.embedding_provider().await.is_none(),
                "handler.embedding_provider() should return None after Unavailable"
            );
        }
        other => panic!("expected Unavailable, got: {}", describe_settled(&other)),
    }
}

#[tokio::test]
async fn test_handler_daemon_mode_wait_until_settled_preserves_unavailable_runtime_status() {
    let mut handler = JulieServerHandler::new_for_test()
        .await
        .expect("new_for_test should succeed");

    let service = Arc::new(EmbeddingService::initializing());
    handler.embedding_service = Some(Arc::clone(&service));

    let publisher_service = Arc::clone(&service);
    let publisher = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        publisher_service.publish_unavailable(
            "test: strict acceleration disabled CPU runtime".to_string(),
            Some(crate::embeddings::EmbeddingRuntimeStatus {
                requested_backend: crate::embeddings::EmbeddingBackend::Auto,
                resolved_backend: crate::embeddings::EmbeddingBackend::Sidecar,
                accelerated: false,
                degraded_reason: Some("Embedding disabled by strict acceleration mode".to_string()),
            }),
        );
    });

    let svc = handler
        .embedding_service
        .as_ref()
        .expect("handler should have embedding_service set")
        .clone();
    let outcome = svc.wait_until_settled(Duration::from_secs(1)).await;
    publisher.await.expect("publisher task should not panic");

    match outcome {
        EmbeddingServiceSettled::Unavailable {
            reason,
            runtime_status,
        } => {
            assert_eq!(reason, "test: strict acceleration disabled CPU runtime");
            let runtime_status = runtime_status.expect("runtime status should be preserved");
            assert_eq!(runtime_status.requested_backend.as_str(), "auto");
            assert_eq!(runtime_status.resolved_backend.as_str(), "sidecar");
            assert!(!runtime_status.accelerated);
            assert!(
                runtime_status
                    .degraded_reason
                    .as_deref()
                    .is_some_and(|reason| reason.contains("strict acceleration")),
                "unavailable state should preserve degraded reason"
            );
        }
        other => panic!("expected Unavailable, got: {}", describe_settled(&other)),
    }
}

/// `initializing()` starts in `Initializing`. `is_available` is false,
/// `is_settled` is false, `provider` is None.
#[test]
fn test_initializing_state() {
    let service = EmbeddingService::initializing();
    assert!(!service.is_available());
    assert!(!service.is_settled());
    assert!(service.provider().is_none());
    assert!(service.runtime_status().is_none());
}

/// `wait_until_settled` returns `Ready` immediately when the service is
/// already `Ready` before the call.
#[tokio::test]
async fn test_wait_until_settled_already_ready() {
    let fake_provider: Arc<dyn EmbeddingProvider> = Arc::new(NoopProvider::default());
    let status = synthetic_status();
    let service = EmbeddingService::initializing();
    service.publish_ready(fake_provider, status);

    let outcome = service.wait_until_settled(Duration::from_millis(100)).await;
    assert!(matches!(outcome, EmbeddingServiceSettled::Ready { .. }));
}

/// `wait_until_settled` returns `Unavailable` immediately when the service
/// is already `Unavailable` before the call.
#[tokio::test]
async fn test_wait_until_settled_already_unavailable() {
    let service = EmbeddingService::initializing();
    service.publish_unavailable("boom".to_string(), None);

    let outcome = service.wait_until_settled(Duration::from_millis(100)).await;
    match outcome {
        EmbeddingServiceSettled::Unavailable { reason, .. } => assert_eq!(reason, "boom"),
        _ => panic!("expected Unavailable"),
    }
}

/// `wait_until_settled` returns `Ready` when `publish_ready` fires
/// concurrently during the wait — covers the common lazy-init case.
#[tokio::test]
async fn test_wait_until_settled_publish_during_wait() {
    let service = Arc::new(EmbeddingService::initializing());

    let publisher = {
        let service = Arc::clone(&service);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let fake_provider: Arc<dyn EmbeddingProvider> = Arc::new(NoopProvider::default());
            service.publish_ready(fake_provider, synthetic_status());
        })
    };

    let outcome = service.wait_until_settled(Duration::from_millis(500)).await;
    publisher.await.unwrap();
    assert!(
        matches!(outcome, EmbeddingServiceSettled::Ready { .. }),
        "waiter should observe Ready published during the wait"
    );
}

/// `wait_until_settled` returns `Timeout` when the service never settles.
#[tokio::test]
async fn test_wait_until_settled_timeout() {
    let service = EmbeddingService::initializing();
    let outcome = service.wait_until_settled(Duration::from_millis(20)).await;
    assert!(matches!(outcome, EmbeddingServiceSettled::Timeout));
}

/// Multiple concurrent waiters all receive the settlement.
#[tokio::test]
async fn test_wait_until_settled_multiple_waiters() {
    let service = Arc::new(EmbeddingService::initializing());

    let waiter_count = 4;
    let mut handles = Vec::new();
    for _ in 0..waiter_count {
        let service = Arc::clone(&service);
        handles.push(tokio::spawn(async move {
            service.wait_until_settled(Duration::from_millis(500)).await
        }));
    }

    tokio::time::sleep(Duration::from_millis(20)).await;
    service.publish_unavailable("shared reason".to_string(), None);

    for handle in handles {
        let outcome = handle.await.unwrap();
        match outcome {
            EmbeddingServiceSettled::Unavailable { reason, .. } => {
                assert_eq!(reason, "shared reason")
            }
            _ => panic!("expected all waiters to observe Unavailable"),
        }
    }
}

/// Sync `initialize()` compat path with provider explicitly disabled via env var.
#[test]
fn test_embedding_service_initialize_with_provider_disabled() {
    unsafe {
        std::env::set_var("JULIE_EMBEDDING_PROVIDER", "none");
    }

    let service = EmbeddingService::initialize();

    unsafe {
        std::env::remove_var("JULIE_EMBEDDING_PROVIDER");
    }

    assert!(
        !service.is_available(),
        "provider=none should settle to Unavailable"
    );
    assert!(
        service.runtime_status().is_none(),
        "provider=none skips initialization entirely, so no runtime_status"
    );
    assert!(
        service.is_settled(),
        "service should be settled after initialize()"
    );
}

/// `publish_unavailable` that carries a runtime status exposes it via `runtime_status()`.
#[test]
fn test_unavailable_with_runtime_status_is_queryable() {
    let status = synthetic_status();
    let service = EmbeddingService::initializing();
    service.publish_unavailable("degraded".to_string(), Some(status.clone()));

    assert!(!service.is_available());
    let fetched = service.runtime_status().expect("status should be present");
    assert_eq!(fetched.degraded_reason, status.degraded_reason);
}

// ---- test helpers ----

fn synthetic_status() -> crate::embeddings::EmbeddingRuntimeStatus {
    crate::embeddings::EmbeddingRuntimeStatus {
        requested_backend: crate::embeddings::EmbeddingBackend::Unresolved,
        resolved_backend: crate::embeddings::EmbeddingBackend::Unresolved,
        accelerated: false,
        degraded_reason: None,
    }
}

fn describe_settled(s: &EmbeddingServiceSettled) -> &'static str {
    match s {
        EmbeddingServiceSettled::Ready { .. } => "Ready",
        EmbeddingServiceSettled::Unavailable { .. } => "Unavailable",
        EmbeddingServiceSettled::Timeout => "Timeout",
    }
}

#[derive(Default)]
struct NoopProvider;

impl crate::embeddings::EmbeddingProvider for NoopProvider {
    fn embed_query(
        &self,
        _text: &str,
        _budget: &crate::embeddings::EmbeddingRequestBudget,
    ) -> anyhow::Result<Vec<f32>> {
        Ok(Vec::new())
    }

    fn embed_batch(
        &self,
        _texts: &[String],
        _budget: &crate::embeddings::EmbeddingRequestBudget,
    ) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(Vec::new())
    }

    fn dimensions(&self) -> usize {
        0
    }

    fn encoder_identity(&self) -> anyhow::Result<crate::embeddings::EncoderIdentity> {
        Ok(crate::embeddings::EncoderIdentity::mock("test-noop", 0))
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
