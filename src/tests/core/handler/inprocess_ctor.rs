use crate::handler::JulieServerHandler;
use crate::leadership::LeadershipState;
use crate::registry::database::DaemonDatabase;
use crate::registry::discovery::DaemonLockGuard;
use crate::workspace::registry::generate_workspace_id;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};
use std::fs;
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
// startup_hint source-preservation tests (load-bearing invariant)
//
// new_in_process exists specifically because new() drops startup_hint.source
// to None, which would break on_initialized's Cwd auto-index deferral.
// These tests prove the source survives the ctor round-trip.
// ---------------------------------------------------------------------------

/// Cwd source (the deferral trigger) must survive new_in_process unchanged.
/// startup_source_prefers_request_roots returns true only for Some(Cwd).
#[tokio::test]
async fn test_new_in_process_preserves_startup_hint_source() {
    let dir = tempfile::tempdir().unwrap();

    // Cwd — the variant that triggers on_initialized deferral.
    let hint_cwd = WorkspaceStartupHint {
        path: dir.path().to_path_buf(),
        source: Some(WorkspaceStartupSource::Cwd),
    };
    let handler_cwd =
        JulieServerHandler::new_in_process(hint_cwd, None, LeadershipState::none(), None)
            .await
            .unwrap();
    assert_eq!(
        handler_cwd.workspace_startup_hint().source,
        Some(WorkspaceStartupSource::Cwd),
        "Cwd source must survive new_in_process (new() drops it to None, breaking deferral)"
    );

    // Cli — eager-index path; also must round-trip.
    let dir2 = tempfile::tempdir().unwrap();
    let hint_cli = WorkspaceStartupHint {
        path: dir2.path().to_path_buf(),
        source: Some(WorkspaceStartupSource::Cli),
    };
    let handler_cli =
        JulieServerHandler::new_in_process(hint_cli, None, LeadershipState::none(), None)
            .await
            .unwrap();
    assert_eq!(
        handler_cli.workspace_startup_hint().source,
        Some(WorkspaceStartupSource::Cli),
        "Cli source must also survive new_in_process"
    );
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
    let handler = JulieServerHandler::new_in_process(hint, None, LeadershipState::none(), None)
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
        JulieServerHandler::new_in_process(hint, None, LeadershipState::leader(guard), None)
            .await
            .unwrap();
    assert!(handler.is_leader(), "with lock → is_leader must be true");
}

#[tokio::test]
async fn test_new_in_process_with_daemon_db_registers_loaded_primary() {
    let dir = tempfile::tempdir().unwrap();
    let primary_root = dir.path().join("primary");
    fs::create_dir_all(&primary_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&dir.path().join("registry.db")).unwrap());
    let primary_path = primary_root.canonicalize().unwrap();
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str).unwrap();
    let hint = WorkspaceStartupHint {
        path: primary_path,
        source: Some(WorkspaceStartupSource::Cli),
    };

    let handler = JulieServerHandler::new_in_process_with_daemon_db(
        hint,
        None,
        LeadershipState::none(),
        None,
        Some(Arc::clone(&daemon_db)),
    )
    .await
    .unwrap();

    handler.ensure_workspace().await.unwrap();

    let row = daemon_db
        .get_workspace(&primary_id)
        .unwrap()
        .expect("loaded in-process primary should be visible to registry-backed dashboard");
    assert_eq!(row.path, primary_path_str);
}

// ---------------------------------------------------------------------------
// injected embedding_provider() test
// ---------------------------------------------------------------------------

/// The injected EmbeddingProvider must be returned by embedding_provider() as
/// the SAME Arc instance (not just any Some), taking priority over the
/// workspace and embedding-service paths.
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
        None,
    )
    .await
    .unwrap();
    let got = handler
        .embedding_provider()
        .await
        .expect("injected provider must be returned by embedding_provider()");
    assert!(
        Arc::ptr_eq(&got, &provider),
        "embedding_provider() must return the injected Arc, not a different provider"
    );
}
