// Workspace initialization tests relocated from the top-crate.
// Only handler-free tests live here (env_paths.rs).
// root_detection.rs stays top-crate — it uses ManageWorkspaceTool.

use serial_test::serial;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

mod env_paths;

/// Helper to create a test directory structure (mirrors top-crate setup_test_workspace)
fn setup_test_workspace() -> TempDir {
    let temp = TempDir::new().expect("Failed to create temp dir");
    fs::create_dir_all(temp.path().join("src")).expect("Failed to create src dir");
    temp
}
