//! Path traversal security tests for editing tools.

use crate::utils::file_utils::secure_path_resolution;
use tempfile::TempDir;

#[test]
fn test_absolute_path_outside_workspace_rejected() {
    let workspace = TempDir::new().unwrap();
    let result = secure_path_resolution("/etc/passwd", workspace.path());
    assert!(result.is_err(), "Absolute path outside workspace should be rejected");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Security") || err.contains("traversal"),
        "Error should mention security: {}", err
    );
}

#[test]
fn test_relative_traversal_rejected() {
    let workspace = TempDir::new().unwrap();
    let result = secure_path_resolution("../../../../etc/passwd", workspace.path());
    assert!(result.is_err(), "Relative traversal should be rejected");
}

#[test]
#[cfg(unix)]
fn test_symlink_outside_workspace_rejected() {
    use std::os::unix::fs::symlink;
    let workspace = TempDir::new().unwrap();
    let link_path = workspace.path().join("evil_link");
    symlink("/etc/passwd", &link_path).unwrap();
    let result = secure_path_resolution("evil_link", workspace.path());
    assert!(result.is_err(), "Symlink pointing outside workspace should be rejected");
}

#[test]
fn test_valid_path_within_workspace_accepted() {
    let workspace = TempDir::new().unwrap();
    std::fs::create_dir_all(workspace.path().join("src")).unwrap();
    std::fs::write(workspace.path().join("src/main.rs"), "fn main() {}").unwrap();
    let result = secure_path_resolution("src/main.rs", workspace.path());
    assert!(result.is_ok(), "Valid path should be accepted");
}
