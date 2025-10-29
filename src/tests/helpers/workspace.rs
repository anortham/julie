//! Test workspace builders with proper isolation

use anyhow::Result;
use std::path::PathBuf;
use tempfile::TempDir;

/// Create a unique test workspace with process ID to prevent collisions
pub fn create_unique_test_workspace(test_name: &str) -> Result<TempDir> {
    let unique_id = format!("{}_{}", test_name, std::process::id());
    let temp_dir = tempfile::Builder::new()
        .prefix(&unique_id)
        .tempdir()?;
    Ok(temp_dir)
}

/// Get fixture path (existing helper, centralized)
pub fn get_fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/test-workspaces")
        .join(name)
}
