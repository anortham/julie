//! Tests for CLI argument parsing (clap) and workspace resolution.

use crate::cli::{Cli, resolve_workspace_root};
use clap::Parser;
use std::path::PathBuf;

// ============================================================================
// CLI PARSING TESTS
// ============================================================================

#[test]
fn test_no_args_parses_successfully() {
    let cli = Cli::parse_from(["julie-server"]);
    assert!(cli.workspace.is_none());
}

#[test]
fn test_workspace_flag_parsed() {
    let cli = Cli::parse_from(["julie-server", "--workspace", "/tmp/myproject"]);
    assert_eq!(cli.workspace, Some(PathBuf::from("/tmp/myproject")));
}

// ============================================================================
// WORKSPACE RESOLUTION TESTS
// ============================================================================

#[test]
fn test_resolve_workspace_root_with_existing_path() {
    // Use a path that definitely exists
    let result = resolve_workspace_root(Some(PathBuf::from("/tmp")));
    // Should be canonicalized (on macOS /tmp -> /private/tmp)
    assert!(result.exists());
}

#[test]
fn test_resolve_workspace_root_with_nonexistent_path_falls_through() {
    // Non-existent CLI path should fall through to env var or cwd
    let result =
        resolve_workspace_root(Some(PathBuf::from("/nonexistent/path/that/does/not/exist")));
    // Should fall through to current directory
    assert!(result.exists());
}

#[test]
fn test_resolve_workspace_root_none_uses_cwd() {
    let result = resolve_workspace_root(None);
    let cwd = std::env::current_dir().unwrap();
    // Both should resolve to the same canonical path
    assert_eq!(
        result.canonicalize().unwrap_or(result.clone()),
        cwd.canonicalize().unwrap_or(cwd)
    );
}

#[test]
fn test_resolve_workspace_root_canonicalizes() {
    // /tmp on macOS is a symlink to /private/tmp — verify canonicalization
    let result = resolve_workspace_root(Some(PathBuf::from("/tmp")));
    // The result should be the canonical form
    let canonical = PathBuf::from("/tmp").canonicalize().unwrap();
    assert_eq!(result, canonical);
}
