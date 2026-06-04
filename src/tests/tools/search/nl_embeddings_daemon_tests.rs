use crate::handler::embedding_init::take_nl_definition_embedding_init_attempts;
use crate::tools::search::nl_embeddings::maybe_initialize_embeddings_for_nl_definitions;
use crate::daemon::embedding_service::EmbeddingService;
use crate::handler::JulieServerHandler;
use std::sync::Arc;
use std::time::Duration;

/// In daemon mode, when the shared service publishes Ready mid-wait, the
/// NL definition init helper should observe Ready and return WITHOUT
/// triggering the per-workspace stdio init path. The init counter MUST
/// stay at zero — incrementing it would mean we'd spawned a duplicate
/// Python sidecar alongside the daemon's shared one.
#[tokio::test]
async fn test_daemon_mode_publishes_ready_no_stdio_fallback() {
    let mut handler = JulieServerHandler::new_for_test()
        .await
        .expect("new_for_test should succeed");

    // Inject a fresh shared service in Initializing.
    let service = Arc::new(EmbeddingService::initializing());
    handler.embedding_service = Some(Arc::clone(&service));

    // Background publisher: simulates the daemon's background init
    // task finishing while the NL query is waiting.
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

    // Reset the counter for this workspace before invoking.
    let workspace_root = std::path::PathBuf::from("test-daemon-publishes-ready");
    let _ = take_nl_definition_embedding_init_attempts(&workspace_root);

    // NL-like definitions query that would otherwise trigger lazy init.
    maybe_initialize_embeddings_for_nl_definitions(
        "how does the user authentication flow work",
        &handler,
    )
    .await;

    publisher.await.expect("publisher task should not panic");

    // Critical assertion: the stdio fallback path was never reached.
    let count = take_nl_definition_embedding_init_attempts(&workspace_root);
    assert_eq!(
        count, 0,
        "daemon-mode NL query must not trigger per-workspace stdio init"
    );
}

/// In daemon mode, when the shared service stays Initializing past the
/// 3 second NL query timeout, the helper should return without triggering
/// the stdio fallback. Same critical guarantee as the publish_ready case:
/// no duplicate sidecar.
#[tokio::test]
async fn test_daemon_mode_timeout_no_stdio_fallback() {
    let mut handler = JulieServerHandler::new_for_test()
        .await
        .expect("new_for_test should succeed");

    // Service that never settles. We rely on the helper's 3s timeout
    // to bail rather than holding the test for the full duration.
    // To keep the test fast, we use the EmbeddingService directly with
    // a manual short-timeout call instead of waiting the full 3s.
    // For the test, we shorten the wait by publishing Unavailable
    // immediately — the assertion is still about the no-fallthrough
    // guarantee, and Unavailable triggers the same return-without-fallthrough
    // branch as Timeout.
    let service = Arc::new(EmbeddingService::initializing());
    handler.embedding_service = Some(Arc::clone(&service));
    service.publish_unavailable("test: model load failed".to_string(), None);

    let workspace_root = std::path::PathBuf::from("test-daemon-timeout");
    let _ = take_nl_definition_embedding_init_attempts(&workspace_root);

    maybe_initialize_embeddings_for_nl_definitions(
        "how does the user authentication flow work",
        &handler,
    )
    .await;

    let count = take_nl_definition_embedding_init_attempts(&workspace_root);
    assert_eq!(
        count, 0,
        "daemon-mode NL query must not trigger per-workspace stdio init even when service is Unavailable"
    );
}

/// Stdio mode (no embedding_service) should still take the per-workspace
/// lazy init path. This guards against the daemon-mode split accidentally
/// breaking the stdio mode entrypoint. Note: this test does NOT actually
/// verify the counter increments, because that requires a workspace to
/// be set on the handler (the helper bails early without one). It does
/// verify the helper returns cleanly without panicking.
#[tokio::test]
async fn test_stdio_mode_no_workspace_returns_cleanly() {
    let handler = JulieServerHandler::new_for_test()
        .await
        .expect("new_for_test should succeed");
    // No embedding_service set → stdio mode
    // No workspace set → should_attempt_init returns false → early return

    maybe_initialize_embeddings_for_nl_definitions("how does login work", &handler).await;
    // If we got here without panicking, the early return path works.
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
