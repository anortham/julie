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
