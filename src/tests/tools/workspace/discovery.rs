//! Tests for workspace discovery, filtering, and generated `.julieignore` behavior.

use crate::tools::shared::{
    BLACKLISTED_DIRECTORIES, BLACKLISTED_EXTENSIONS, BLACKLISTED_FILENAMES,
};
use crate::tools::workspace::ManageWorkspaceTool;
use std::collections::HashSet;
use std::path::PathBuf;
use tempfile::TempDir;

mod daemon_home;
mod file_discovery;
mod file_filtering;
mod julieignore_generation;
mod vendor_patterns;

// =============================================================================
// Test Helper: Create ManageWorkspaceTool
// =============================================================================

fn create_tool() -> ManageWorkspaceTool {
    ManageWorkspaceTool {
        operation: "test".to_string(), // Dummy operation for testing
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: None,
    }
}

fn create_workspace_with_files(files: Vec<&str>) -> (TempDir, Vec<PathBuf>) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let workspace_root = temp_dir.path();

    let mut file_paths = Vec::new();

    for file_path in files {
        let full_path = workspace_root.join(file_path);

        // Create parent directories
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).expect("Failed to create parent dirs");
        }

        // Create empty file
        std::fs::write(&full_path, "").expect("Failed to create file");

        file_paths.push(full_path);
    }

    (temp_dir, file_paths)
}
