//! Tests for path utility functions (display_path, to_relative_unix_style, etc.)

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use crate::utils::paths::{display_path, resolve_workspace_file_input};

// ============================================================================
// display_path() TESTS
// ============================================================================

#[test]
fn test_display_path_strips_extended_length_prefix() {
    let path = Path::new(r"\\?\C:\Users\alice\project");
    assert_eq!(display_path(path), r"C:\Users\alice\project");
}

#[test]
fn test_display_path_converts_unc_extended_to_unc() {
    // \\?\UNC\server\share\folder → \\server\share\folder
    let path = Path::new(r"\\?\UNC\server\share\folder");
    assert_eq!(display_path(path), r"\\server\share\folder");
}

#[test]
fn test_display_path_converts_unc_extended_root() {
    // \\?\UNC\server\share → \\server\share
    let path = Path::new(r"\\?\UNC\server\share");
    assert_eq!(display_path(path), r"\\server\share");
}

#[test]
fn test_display_path_passes_through_normal_path() {
    let path = Path::new(r"C:\Users\alice\project");
    assert_eq!(display_path(path), r"C:\Users\alice\project");
}

#[test]
fn test_display_path_passes_through_unix_path() {
    let path = Path::new("/home/alice/project");
    assert_eq!(display_path(path), "/home/alice/project");
}

#[test]
fn test_display_path_passes_through_relative_path() {
    let path = Path::new("src/main.rs");
    assert_eq!(display_path(path), "src/main.rs");
}

fn write_test_file(path: &Path) {
    fs::create_dir_all(path.parent().expect("test path should have a parent"))
        .expect("parent directory should be created");
    fs::write(path, "").expect("test file should be written");
}

#[test]
fn test_resolve_workspace_file_input_handles_tool_file_paths() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let workspace = temp_dir.path().join("workspace");
    let inside_file = workspace.join("src/lib.rs");
    let outside_file = temp_dir.path().join("outside.rs");
    let tilde_file = workspace.join("~/literal.rs");

    write_test_file(&inside_file);
    write_test_file(&outside_file);
    write_test_file(&tilde_file);

    let workspace_root = workspace
        .canonicalize()
        .expect("workspace root should canonicalize");
    let canonical_inside = inside_file
        .canonicalize()
        .expect("inside file should canonicalize");
    let canonical_outside = outside_file
        .canonicalize()
        .expect("outside file should canonicalize");
    let canonical_tilde = tilde_file
        .canonicalize()
        .expect("literal tilde file should canonicalize");

    let absolute_inside =
        resolve_workspace_file_input(canonical_inside.to_str().unwrap(), &workspace_root);
    assert_eq!(absolute_inside.absolute_path, canonical_inside);
    assert_eq!(absolute_inside.relative_query_path.unwrap(), "src/lib.rs");
    assert!(absolute_inside.canonicalized);

    let relative_inside = resolve_workspace_file_input("src/lib.rs", &workspace_root);
    assert_eq!(relative_inside.absolute_path, canonical_inside);
    assert_eq!(relative_inside.relative_query_path.unwrap(), "src/lib.rs");
    assert!(relative_inside.canonicalized);

    let dot_relative = resolve_workspace_file_input("./src/lib.rs", &workspace_root);
    assert_eq!(dot_relative.absolute_path, canonical_inside);
    assert_eq!(dot_relative.relative_query_path.unwrap(), "src/lib.rs");
    assert!(dot_relative.canonicalized);

    let parent_relative = resolve_workspace_file_input("src/../src/lib.rs", &workspace_root);
    assert_eq!(parent_relative.absolute_path, canonical_inside);
    assert_eq!(parent_relative.relative_query_path.unwrap(), "src/lib.rs");
    assert!(parent_relative.canonicalized);

    let missing_inside = resolve_workspace_file_input("src/missing.rs", &workspace_root);
    assert_eq!(
        missing_inside.absolute_path,
        workspace_root.join("src/missing.rs")
    );
    assert_eq!(
        missing_inside.relative_query_path.unwrap(),
        "src/missing.rs"
    );
    assert!(!missing_inside.canonicalized);

    let absolute_outside =
        resolve_workspace_file_input(canonical_outside.to_str().unwrap(), &workspace_root);
    assert_eq!(absolute_outside.absolute_path, canonical_outside);
    assert!(absolute_outside.relative_query_path.is_err());
    assert!(absolute_outside.canonicalized);

    let traversal_outside = resolve_workspace_file_input("../outside.rs", &workspace_root);
    assert_eq!(traversal_outside.absolute_path, canonical_outside);
    assert!(traversal_outside.relative_query_path.is_err());
    assert!(traversal_outside.canonicalized);

    let tilde_literal = resolve_workspace_file_input("~/literal.rs", &workspace_root);
    assert_eq!(tilde_literal.absolute_path, canonical_tilde);
    assert_eq!(tilde_literal.relative_query_path.unwrap(), "~/literal.rs");
    assert!(tilde_literal.canonicalized);
}
