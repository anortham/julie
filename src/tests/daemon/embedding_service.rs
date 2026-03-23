use crate::daemon::embedding_service::EmbeddingService;
use crate::handler::JulieServerHandler;

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
