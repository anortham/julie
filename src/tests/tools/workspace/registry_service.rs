// Tests extracted from src/workspace/registry_service.rs
// These were previously inline tests that have been moved to follow project standards

use crate::workspace::registry::WorkspaceType;
use crate::workspace::registry_service::WorkspaceRegistryService;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Barrier;

#[tokio::test]
async fn test_registry_creation() {
    let temp_dir = TempDir::new().unwrap();
    let service = WorkspaceRegistryService::new(temp_dir.path().to_path_buf());

    let registry = service.get_or_create_registry().await.unwrap();
    assert_eq!(registry.version, "1.0");
    assert!(registry.primary_workspace.is_none());
    assert!(registry.reference_workspaces.is_empty());
}

#[tokio::test]
async fn test_workspace_registration() {
    let temp_dir = TempDir::new().unwrap();
    let service = WorkspaceRegistryService::new(temp_dir.path().to_path_buf());

    // Register primary workspace
    let primary = service
        .register_workspace(
            temp_dir.path().to_string_lossy().to_string(),
            WorkspaceType::Primary,
        )
        .await
        .unwrap();

    assert_eq!(primary.workspace_type, WorkspaceType::Primary);
    assert!(primary.expires_at.is_none()); // Primary never expires

    // Register reference workspace
    let ref_path = temp_dir
        .path()
        .join("reference")
        .to_string_lossy()
        .to_string();
    let reference = service
        .register_workspace(ref_path, WorkspaceType::Reference)
        .await
        .unwrap();

    assert_eq!(reference.workspace_type, WorkspaceType::Reference);
    assert!(reference.expires_at.is_some()); // Reference expires

    // Verify workspaces exist
    let all_workspaces = service.get_all_workspaces().await.unwrap();
    assert_eq!(all_workspaces.len(), 2);
}

#[tokio::test]
async fn test_registry_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_path_buf();

    // Create and populate registry
    {
        let service = WorkspaceRegistryService::new(workspace_path.clone());
        service
            .register_workspace(
                temp_dir.path().to_string_lossy().to_string(),
                WorkspaceType::Primary,
            )
            .await
            .unwrap();
    }

    // Create new service instance and verify persistence
    {
        let service = WorkspaceRegistryService::new(workspace_path);
        let registry = service.load_registry().await.unwrap();
        assert!(registry.primary_workspace.is_some());
    }
}

#[tokio::test]
async fn test_concurrent_registry_saves_do_not_conflict_on_temp_file() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path().to_path_buf();

    // Prime the registry with a primary workspace using one service instance
    {
        let primary_service = WorkspaceRegistryService::new(workspace_root.clone());
        let primary_dir = workspace_root.join("primary");
        std::fs::create_dir_all(&primary_dir).unwrap();
        primary_service
            .register_workspace(
                primary_dir.to_string_lossy().to_string(),
                WorkspaceType::Primary,
            )
            .await
            .unwrap();
    }

    let reference_a_dir = workspace_root.join("reference_a");
    let reference_b_dir = workspace_root.join("reference_b");
    std::fs::create_dir_all(&reference_a_dir).unwrap();
    std::fs::create_dir_all(&reference_b_dir).unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let barrier_a = barrier.clone();
    let barrier_b = barrier.clone();

    let workspace_path_a = workspace_root.clone();
    let workspace_path_b = workspace_root.clone();
    let reference_a_path = reference_a_dir.to_string_lossy().to_string();
    let reference_b_path = reference_b_dir.to_string_lossy().to_string();

    let (result_a, result_b) = tokio::join!(
        async move {
            let service = WorkspaceRegistryService::new(workspace_path_a);
            barrier_a.wait().await;
            service
                .register_workspace(reference_a_path, WorkspaceType::Reference)
                .await
        },
        async move {
            let service = WorkspaceRegistryService::new(workspace_path_b);
            barrier_b.wait().await;
            service
                .register_workspace(reference_b_path, WorkspaceType::Reference)
                .await
        }
    );

    assert!(
        result_a.is_ok(),
        "First concurrent save failed: {:?}",
        result_a
    );
    assert!(
        result_b.is_ok(),
        "Second concurrent save failed: {:?}",
        result_b
    );
}

#[tokio::test]
async fn test_unregister_workspace() {
    let temp_dir = TempDir::new().unwrap();
    let service = WorkspaceRegistryService::new(temp_dir.path().to_path_buf());

    // Register two workspaces
    let primary = service
        .register_workspace(
            temp_dir.path().to_string_lossy().to_string(),
            WorkspaceType::Primary,
        )
        .await
        .unwrap();

    let ref_path = temp_dir
        .path()
        .join("reference")
        .to_string_lossy()
        .to_string();
    let reference = service
        .register_workspace(ref_path.clone(), WorkspaceType::Reference)
        .await
        .unwrap();

    // Verify both exist
    assert_eq!(service.get_all_workspaces().await.unwrap().len(), 2);

    // Unregister reference workspace
    let removed = service
        .unregister_workspace(&reference.id)
        .await
        .unwrap();
    assert!(removed, "Should successfully unregister workspace");

    // Verify only primary remains
    let remaining = service.get_all_workspaces().await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, primary.id);

    // Unregistering again should return false
    let removed_again = service
        .unregister_workspace(&reference.id)
        .await
        .unwrap();
    assert!(!removed_again, "Should return false for non-existent workspace");
}

#[tokio::test]
async fn test_get_workspace_by_id() {
    let temp_dir = TempDir::new().unwrap();
    let service = WorkspaceRegistryService::new(temp_dir.path().to_path_buf());

    // Register workspace
    let registered = service
        .register_workspace(
            temp_dir.path().to_string_lossy().to_string(),
            WorkspaceType::Primary,
        )
        .await
        .unwrap();

    // Get by ID - should succeed
    let found = service
        .get_workspace(&registered.id)
        .await
        .unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, registered.id);

    // Get by non-existent ID - should return None
    let not_found = service.get_workspace("nonexistent_id").await.unwrap();
    assert!(not_found.is_none());
}

#[tokio::test]
async fn test_get_workspace_by_path() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_string_lossy().to_string();
    let service = WorkspaceRegistryService::new(temp_dir.path().to_path_buf());

    // Register workspace
    service
        .register_workspace(workspace_path.clone(), WorkspaceType::Primary)
        .await
        .unwrap();

    // Get by path - should succeed
    let found = service.get_workspace_by_path(&workspace_path).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().original_path, workspace_path);

    // Get by non-existent path - should return None
    let not_found = service
        .get_workspace_by_path("/nonexistent/path")
        .await
        .unwrap();
    assert!(not_found.is_none());
}

#[tokio::test]
async fn test_get_primary_workspace_id() {
    let temp_dir = TempDir::new().unwrap();
    let service = WorkspaceRegistryService::new(temp_dir.path().to_path_buf());

    // Initially no primary workspace
    let initial = service.get_primary_workspace_id().await.unwrap();
    assert!(initial.is_none());

    // Register primary workspace
    let primary = service
        .register_workspace(
            temp_dir.path().to_string_lossy().to_string(),
            WorkspaceType::Primary,
        )
        .await
        .unwrap();

    // Now should return primary workspace ID
    let found = service.get_primary_workspace_id().await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap(), primary.id);

    // Add reference workspace - primary ID should not change
    let ref_path = temp_dir
        .path()
        .join("reference")
        .to_string_lossy()
        .to_string();
    service
        .register_workspace(ref_path, WorkspaceType::Reference)
        .await
        .unwrap();

    let still_primary = service.get_primary_workspace_id().await.unwrap();
    assert_eq!(still_primary.unwrap(), primary.id);
}

#[tokio::test]
async fn test_update_workspace_statistics() {
    let temp_dir = TempDir::new().unwrap();
    let service = WorkspaceRegistryService::new(temp_dir.path().to_path_buf());

    // Register workspace
    let workspace = service
        .register_workspace(
            temp_dir.path().to_string_lossy().to_string(),
            WorkspaceType::Primary,
        )
        .await
        .unwrap();

    // Update statistics
    service
        .update_workspace_statistics(&workspace.id, 100, 50, 1024 * 1024)
        .await
        .unwrap();

    // Verify statistics were updated
    let updated = service
        .get_workspace(&workspace.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.symbol_count, 100);
    assert_eq!(updated.file_count, 50);
    assert_eq!(updated.index_size_bytes, 1024 * 1024);
}

#[tokio::test]
async fn test_update_last_accessed() {
    let temp_dir = TempDir::new().unwrap();
    let service = WorkspaceRegistryService::new(temp_dir.path().to_path_buf());

    // Register workspace
    let workspace = service
        .register_workspace(
            temp_dir.path().to_string_lossy().to_string(),
            WorkspaceType::Primary,
        )
        .await
        .unwrap();

    let original_accessed = workspace.last_accessed;

    // Wait sufficient time to ensure timestamp difference (Unix timestamps are in seconds)
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Update last accessed
    service
        .update_last_accessed(&workspace.id)
        .await
        .unwrap();

    // Verify timestamp was updated
    let updated = service
        .get_workspace(&workspace.id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        updated.last_accessed > original_accessed,
        "Last accessed should be updated: {} > {}",
        updated.last_accessed,
        original_accessed
    );
}

#[tokio::test]
async fn test_update_embedding_status() {
    use crate::workspace::registry::EmbeddingStatus;

    let temp_dir = TempDir::new().unwrap();
    let service = WorkspaceRegistryService::new(temp_dir.path().to_path_buf());

    // Register workspace
    let workspace = service
        .register_workspace(
            temp_dir.path().to_string_lossy().to_string(),
            WorkspaceType::Primary,
        )
        .await
        .unwrap();

    // Initially should be NotStarted or Generating
    let initial = service
        .get_workspace(&workspace.id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        initial.embedding_status == EmbeddingStatus::NotStarted
            || initial.embedding_status == EmbeddingStatus::Generating
    );

    // Update to Ready
    service
        .update_embedding_status(&workspace.id, EmbeddingStatus::Ready)
        .await
        .unwrap();

    // Verify status was updated
    let updated = service
        .get_workspace(&workspace.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.embedding_status, EmbeddingStatus::Ready);
}

#[tokio::test]
async fn test_get_all_workspaces_empty() {
    let temp_dir = TempDir::new().unwrap();
    let service = WorkspaceRegistryService::new(temp_dir.path().to_path_buf());

    // Initially empty
    let workspaces = service.get_all_workspaces().await.unwrap();
    assert!(workspaces.is_empty());
}

#[tokio::test]
async fn test_get_all_workspaces_multiple() {
    let temp_dir = TempDir::new().unwrap();
    let service = WorkspaceRegistryService::new(temp_dir.path().to_path_buf());

    // Register primary
    service
        .register_workspace(
            temp_dir.path().to_string_lossy().to_string(),
            WorkspaceType::Primary,
        )
        .await
        .unwrap();

    // Register multiple reference workspaces
    for i in 1..=3 {
        let ref_path = temp_dir
            .path()
            .join(format!("reference_{}", i))
            .to_string_lossy()
            .to_string();
        service
            .register_workspace(ref_path, WorkspaceType::Reference)
            .await
            .unwrap();
    }

    // Should have 4 total (1 primary + 3 reference)
    let all_workspaces = service.get_all_workspaces().await.unwrap();
    assert_eq!(all_workspaces.len(), 4);

    // Verify types
    let primary_count = all_workspaces
        .iter()
        .filter(|w| w.workspace_type == WorkspaceType::Primary)
        .count();
    let reference_count = all_workspaces
        .iter()
        .filter(|w| w.workspace_type == WorkspaceType::Reference)
        .count();

    assert_eq!(primary_count, 1);
    assert_eq!(reference_count, 3);
}
