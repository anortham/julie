//! Tests for `workspace::JulieWorkspace` extracted from the implementation module.

use crate::workspace::JulieWorkspace;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_workspace_initialization() {
    std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    let temp_dir = TempDir::new().unwrap();
    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf()).unwrap();

    // Check that .julie directory was created
    assert!(workspace.julie_dir.exists());

    // Check that all required subdirectories exist
    assert!(workspace.julie_dir.join("db").exists());
    assert!(workspace.julie_dir.join("index/tantivy").exists());
    assert!(workspace.julie_dir.join("vectors").exists());
    assert!(workspace.julie_dir.join("models").exists());
    assert!(workspace.julie_dir.join("cache").exists());
    assert!(workspace.julie_dir.join("logs").exists());
    assert!(workspace.julie_dir.join("config").exists());

    // Check that config file was created
    assert!(workspace.julie_dir.join("config/julie.toml").exists());
}

#[test]
fn test_workspace_detection() {
    std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf()).unwrap();
    drop(workspace);

    let lock_dir = temp_dir.path().join(".julie").join("index").join("tantivy");
    for lock_name in ["write.lock", "meta.lock"] {
        let lock_path = lock_dir.join(lock_name);
        if lock_path.exists() {
            fs::remove_file(&lock_path).unwrap();
        }
    }

    // Test detection from same directory
    let detected = JulieWorkspace::detect_and_load(temp_dir.path().to_path_buf()).unwrap();
    assert!(detected.is_some());

    // Test detection from subdirectory
    let subdir = temp_dir.path().join("subdir");
    fs::create_dir(&subdir).unwrap();
    let detected = JulieWorkspace::detect_and_load(subdir).unwrap();
    assert!(detected.is_some());
}

#[test]
fn test_health_check() {
    std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    let temp_dir = TempDir::new().unwrap();
    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf()).unwrap();

    let health = workspace.health_check().unwrap();
    assert!(health.is_healthy());
    assert!(health.structure_valid);
    assert!(health.has_write_permissions);
}
