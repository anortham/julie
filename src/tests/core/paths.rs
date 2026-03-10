//! Tests for path utility functions (display_path, to_relative_unix_style, etc.)

use std::path::Path;

use crate::utils::paths::display_path;

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
