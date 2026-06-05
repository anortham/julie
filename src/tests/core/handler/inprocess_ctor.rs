use crate::handler::{JulieServerHandler, LeadershipState};
use crate::daemon::discovery::DaemonLockGuard;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Minimal mock EmbeddingProvider for injection tests.
// ---------------------------------------------------------------------------

struct MockProvider;

impl crate::embeddings::EmbeddingProvider for MockProvider {
    fn embed_query(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(vec![1.0])
    }
    fn embed_batch(&self, _texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(vec![])
    }
    fn dimensions(&self) -> usize {
        1
    }
    fn device_info(&self) -> crate::embeddings::DeviceInfo {
        crate::embeddings::DeviceInfo {
            runtime: "mock".into(),
            device: "cpu".into(),
            model_name: "mock".into(),
            dimensions: 1,
        }
    }
}

// ---------------------------------------------------------------------------
// is_leader() tests
// ---------------------------------------------------------------------------

/// Handler created with LeadershipState::none() must not claim leadership.
#[tokio::test]
async fn test_new_in_process_no_leader_is_not_leader() {
    let dir = tempfile::tempdir().unwrap();
    let hint = WorkspaceStartupHint {
        path: dir.path().to_path_buf(),
        source: Some(WorkspaceStartupSource::Cli),
    };
    let handler = JulieServerHandler::new_in_process(hint, None, LeadershipState::none())
        .await
        .unwrap();
    assert!(!handler.is_leader(), "no lock → is_leader must be false");
}

/// Handler created with a real DaemonLockGuard must report is_leader() == true.
#[tokio::test]
async fn test_new_in_process_with_leader_is_leader() {
    let dir = tempfile::tempdir().unwrap();
    let lock_path = dir.path().join("leader.lock");
    let guard = DaemonLockGuard::try_acquire(&lock_path)
        .expect("lock must be acquirable on uncontended path");
    let hint = WorkspaceStartupHint {
        path: dir.path().to_path_buf(),
        source: Some(WorkspaceStartupSource::Cli),
    };
    let handler =
        JulieServerHandler::new_in_process(hint, None, LeadershipState::leader(guard))
            .await
            .unwrap();
    assert!(handler.is_leader(), "with lock → is_leader must be true");
}

// ---------------------------------------------------------------------------
// injected embedding_provider() test
// ---------------------------------------------------------------------------

/// An injected EmbeddingProvider must be returned by embedding_provider(),
/// taking priority over the workspace and embedding-service paths.
#[tokio::test]
async fn test_new_in_process_injected_provider_returned() {
    let dir = tempfile::tempdir().unwrap();
    let hint = WorkspaceStartupHint {
        path: dir.path().to_path_buf(),
        source: Some(WorkspaceStartupSource::Cli),
    };
    let provider: Arc<dyn crate::embeddings::EmbeddingProvider> = Arc::new(MockProvider);
    let handler = JulieServerHandler::new_in_process(
        hint,
        Some(Arc::clone(&provider)),
        LeadershipState::none(),
    )
    .await
    .unwrap();
    let got = handler.embedding_provider().await;
    assert!(
        got.is_some(),
        "injected provider must be returned by embedding_provider()"
    );
}
