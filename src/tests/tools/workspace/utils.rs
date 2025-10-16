//! Inline tests extracted from tools/workspace/utils.rs
//!
//! Tests for workspace utility functions, particularly directory size calculation.

use crate::tools::workspace::calculate_dir_size;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_calculate_dir_size_empty() {
    let temp_dir = TempDir::new().unwrap();
    let size = calculate_dir_size(temp_dir.path()).unwrap();
    assert_eq!(size, 0);
}

#[test]
fn test_calculate_dir_size_with_files() {
    let temp_dir = TempDir::new().unwrap();

    // Create test files
    fs::write(temp_dir.path().join("file1.txt"), "hello").unwrap();
    fs::write(temp_dir.path().join("file2.txt"), "world!").unwrap();

    let size = calculate_dir_size(temp_dir.path()).unwrap();
    assert_eq!(size, 11); // "hello" (5) + "world!" (6)
}

#[test]
fn test_calculate_dir_size_nested() {
    let temp_dir = TempDir::new().unwrap();

    // Create nested structure
    let subdir = temp_dir.path().join("subdir");
    fs::create_dir(&subdir).unwrap();
    fs::write(temp_dir.path().join("root.txt"), "root").unwrap();
    fs::write(subdir.join("nested.txt"), "nested").unwrap();

    let size = calculate_dir_size(temp_dir.path()).unwrap();
    assert_eq!(size, 10); // "root" (4) + "nested" (6)
}

#[tokio::test]
async fn test_calculate_dir_size_in_spawn_blocking() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("test.txt"), "async test").unwrap();

    let path = temp_dir.path().to_path_buf();
    let size = tokio::task::spawn_blocking(move || calculate_dir_size(&path))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(size, 10); // "async test" (10)
}
