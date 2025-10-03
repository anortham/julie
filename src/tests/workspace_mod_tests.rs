//! Tests for `workspace::JulieWorkspace` extracted from the implementation module.

use crate::workspace::JulieWorkspace;
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn test_workspace_initialization() {
    std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    let temp_dir = TempDir::new().unwrap();
    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
        .await
        .unwrap();

    // Check that .julie directory was created
    assert!(workspace.julie_dir.exists());

    // Check that all required root subdirectories exist
    // Note: Per-workspace directories (indexes/{workspace_id}/) are created on-demand during indexing
    assert!(
        workspace.julie_dir.join("indexes").exists(),
        "indexes/ root directory should exist"
    );
    assert!(workspace.julie_dir.join("models").exists());
    assert!(workspace.julie_dir.join("cache").exists());
    assert!(workspace.julie_dir.join("logs").exists());
    assert!(workspace.julie_dir.join("config").exists());

    // Check that config file was created
    assert!(workspace.julie_dir.join("config/julie.toml").exists());
}

#[tokio::test]
async fn test_workspace_detection() {
    std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
        .await
        .unwrap();
    drop(workspace);

    // Clean up any per-workspace directories if they exist
    let indexes_dir = temp_dir.path().join(".julie").join("indexes");
    if indexes_dir.exists() {
        let _ = fs::remove_dir_all(&indexes_dir);
        fs::create_dir_all(&indexes_dir).unwrap();
    }

    // Test detection from same directory
    let detected = JulieWorkspace::detect_and_load(temp_dir.path().to_path_buf())
        .await
        .unwrap();
    assert!(detected.is_some());

    // Test detection from subdirectory
    let subdir = temp_dir.path().join("subdir");
    fs::create_dir(&subdir).unwrap();
    let detected = JulieWorkspace::detect_and_load(subdir).await.unwrap();
    assert!(detected.is_some());
}

#[tokio::test]
async fn test_health_check() {
    std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    let temp_dir = TempDir::new().unwrap();
    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
        .await
        .unwrap();

    let health = workspace.health_check().unwrap();
    assert!(health.is_healthy());
    assert!(health.structure_valid);
    assert!(health.has_write_permissions);
}
