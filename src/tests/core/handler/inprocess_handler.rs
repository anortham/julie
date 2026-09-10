//! In-process handler constructor tests that survive the single-writer cutover:
//! startup-hint source round-trip, injected embedding provider, registry-backed
//! primary registration, F2 storage under `index_root`, and workspace-id
//! canonicalization.

use crate::handler::JulieServerHandler;
use crate::paths::RegistryPaths;
use crate::registry::database::DaemonDatabase;
use crate::workspace::registry::generate_workspace_id;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};
use std::fs;
use std::sync::Arc;

struct MockProvider;

impl crate::embeddings::EmbeddingProvider for MockProvider {
    fn embed_query(
        &self,
        _text: &str,
        _budget: &crate::embeddings::EmbeddingRequestBudget,
    ) -> anyhow::Result<Vec<f32>> {
        Ok(vec![1.0])
    }
    fn embed_batch(
        &self,
        _texts: &[String],
        _budget: &crate::embeddings::EmbeddingRequestBudget,
    ) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(vec![])
    }
    fn dimensions(&self) -> usize {
        1
    }
    fn encoder_identity(&self) -> anyhow::Result<crate::embeddings::EncoderIdentity> {
        Ok(crate::embeddings::EncoderIdentity::mock("mock", 1))
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

fn hint(path: &std::path::Path, source: WorkspaceStartupSource) -> WorkspaceStartupHint {
    WorkspaceStartupHint {
        path: path.to_path_buf(),
        source: Some(source),
    }
}

#[tokio::test]
async fn test_new_in_process_preserves_startup_hint_source() {
    for source in [WorkspaceStartupSource::Cwd, WorkspaceStartupSource::Cli] {
        let dir = tempfile::tempdir().unwrap();
        let handler = JulieServerHandler::new_in_process(hint(dir.path(), source), None, None)
            .await
            .unwrap();
        assert_eq!(handler.workspace_startup_hint().source, Some(source));
    }
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

    let handler = JulieServerHandler::new_in_process_with_daemon_db(
        hint(&primary_path, WorkspaceStartupSource::Cli),
        None,
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

#[tokio::test]
async fn test_new_in_process_injected_provider_returned() {
    let dir = tempfile::tempdir().unwrap();
    let provider: Arc<dyn crate::embeddings::EmbeddingProvider> = Arc::new(MockProvider);
    let handler = JulieServerHandler::new_in_process(
        hint(dir.path(), WorkspaceStartupSource::Cli),
        Some(Arc::clone(&provider)),
        None,
    )
    .await
    .unwrap();
    let got = handler
        .embedding_provider()
        .await
        .expect("injected provider must be returned by embedding_provider()");
    assert!(Arc::ptr_eq(&got, &provider));
}

#[tokio::test]
async fn test_inprocess_handler_f2_storage_under_index_root() {
    let home_dir = tempfile::tempdir().unwrap();
    let paths = RegistryPaths::with_home(home_dir.path().to_path_buf());

    let project_dir = tempfile::tempdir().unwrap();
    let workspace_id = generate_workspace_id(&project_dir.path().to_string_lossy()).unwrap();
    let index_root = paths.workspace_index_dir(&workspace_id);
    std::fs::create_dir_all(&index_root).unwrap();

    let handler = JulieServerHandler::new_in_process(
        hint(project_dir.path(), WorkspaceStartupSource::Cli),
        None,
        Some(index_root.clone()),
    )
    .await
    .expect("handler must build via new_in_process");

    handler
        .initialize_workspace_with_force(
            Some(project_dir.path().to_string_lossy().to_string()),
            false,
        )
        .await
        .expect("workspace initialization must succeed");

    let expected_facts = paths.workspace_facts_path(&workspace_id);
    assert!(expected_facts.starts_with(&index_root));
    assert!(
        expected_facts.exists(),
        "facts.sqlite missing at {}",
        expected_facts.display()
    );

    let project_local_db = project_dir
        .path()
        .join(".julie")
        .join("indexes")
        .join(&workspace_id)
        .join("facts.sqlite");
    assert!(!project_local_db.exists());
}

#[test]
fn test_in_process_workspace_id_is_canonicalized() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("sub")).unwrap();

    let non_canonical = dir.path().join("sub").join("..");
    let canonical = dir.path().to_path_buf();

    let id_from_noncanon = generate_workspace_id(
        &JulieServerHandler::canonicalize_workspace_path(non_canonical).to_string_lossy(),
    )
    .unwrap();
    let id_from_canon = generate_workspace_id(
        &JulieServerHandler::canonicalize_workspace_path(canonical).to_string_lossy(),
    )
    .unwrap();

    assert_eq!(id_from_noncanon, id_from_canon);
}
