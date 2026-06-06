//! Tests for CLI argument parsing (clap) and workspace resolution.

use crate::cli::{
    Cli, Command, cli_command_needs_workspace_startup_hint, resolve_workspace_root,
    resolve_workspace_startup_hint,
};
use crate::workspace::startup_hint::WorkspaceStartupSource;
use clap::Parser;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

fn workspace_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn with_workspace_env_cleared<T>(f: impl FnOnce() -> T) -> T {
    let _guard = workspace_env_lock().lock().unwrap();
    let previous = std::env::var_os("JULIE_WORKSPACE");

    unsafe {
        std::env::remove_var("JULIE_WORKSPACE");
    }

    let result = f();

    match previous {
        Some(value) => unsafe {
            std::env::set_var("JULIE_WORKSPACE", value);
        },
        None => unsafe {
            std::env::remove_var("JULIE_WORKSPACE");
        },
    }

    result
}

fn with_workspace_env_set<T>(value: &std::path::Path, f: impl FnOnce() -> T) -> T {
    let _guard = workspace_env_lock().lock().unwrap();
    let previous = std::env::var_os("JULIE_WORKSPACE");

    unsafe {
        std::env::set_var("JULIE_WORKSPACE", value);
    }

    let result = f();

    match previous {
        Some(previous) => unsafe {
            std::env::set_var("JULIE_WORKSPACE", previous);
        },
        None => unsafe {
            std::env::remove_var("JULIE_WORKSPACE");
        },
    }

    result
}

// ============================================================================
// CLI PARSING TESTS
// ============================================================================

#[test]
fn test_no_args_parses_successfully() {
    let cli = Cli::parse_from(["julie-server"]);
    assert!(cli.workspace.is_none());
    assert!(cli.command.is_none());
}

#[test]
fn test_workspace_flag_parsed() {
    let cli = Cli::parse_from(["julie-server", "--workspace", "/tmp/myproject"]);
    assert_eq!(cli.workspace, Some(PathBuf::from("/tmp/myproject")));
    assert!(cli.command.is_none());
}

// ============================================================================
// SUBCOMMAND PARSING TESTS
// ============================================================================

#[test]
fn test_workspace_startup_hint_is_in_process_only() {
    let in_process = Cli::parse_from(["julie-server"]);
    let tool = Cli::parse_from(["julie-server", "--workspace", "/tmp/ignored", "dashboard"]);

    assert!(cli_command_needs_workspace_startup_hint(
        &in_process.command
    ));
    assert!(!cli_command_needs_workspace_startup_hint(&tool.command));

    assert!(matches!(tool.command, Some(Command::Dashboard)));
}

#[test]
fn test_workspace_flag_global_with_subcommand() {
    let cli = Cli::parse_from(["julie-server", "--workspace", "/tmp/proj", "dashboard"]);
    assert_eq!(cli.workspace, Some(PathBuf::from("/tmp/proj")));
    assert!(matches!(cli.command, Some(Command::Dashboard)));
}

// ============================================================================
// WORKSPACE RESOLUTION TESTS
// ============================================================================

fn nonexistent_absolute_path(path: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(format!("C:/{path}"))
    } else {
        PathBuf::from(format!("/{path}"))
    }
}

#[test]
fn test_resolve_workspace_root_with_existing_path() {
    // Use a path that definitely exists
    let result = resolve_workspace_root(Some(PathBuf::from("/tmp")));
    // Should be canonicalized (on macOS /tmp -> /private/tmp)
    assert!(result.exists());
}

#[test]
fn test_resolve_workspace_root_with_nonexistent_path_preserves_explicit_path() {
    let raw = nonexistent_absolute_path("nonexistent/path/that/does/not/exist");

    let result = resolve_workspace_root(Some(raw.clone()));

    assert_eq!(result, raw);
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
    // resolve_workspace_root must return the canonical form of an existing path
    // (e.g. /tmp -> /private/tmp on macOS, and the \\?\ verbatim form on Windows).
    // Use a real temp dir so the test is hermetic and cross-platform; the
    // original hardcoded "/tmp" resolves to C:\tmp on Windows, which does not
    // exist on a clean box/CI, so canonicalize().unwrap() panicked there.
    let temp = tempfile::tempdir().unwrap();

    let result = resolve_workspace_root(Some(temp.path().to_path_buf()));

    let canonical = temp.path().canonicalize().unwrap();
    assert_eq!(result, canonical);
}

#[test]
fn test_resolve_workspace_startup_hint_prefers_cli_source() {
    let temp = tempfile::tempdir().unwrap();

    let hint = with_workspace_env_cleared(|| {
        resolve_workspace_startup_hint(Some(temp.path().to_path_buf()))
    });

    assert_eq!(hint.source, Some(WorkspaceStartupSource::Cli));
    assert_eq!(hint.path, temp.path().canonicalize().unwrap());
}

#[test]
fn test_resolve_workspace_startup_hint_preserves_nonexistent_cli_path() {
    let raw = nonexistent_absolute_path("nonexistent/path/that/does/not/exist");

    let hint = with_workspace_env_cleared(|| resolve_workspace_startup_hint(Some(raw.clone())));

    assert_eq!(hint.source, Some(WorkspaceStartupSource::Cli));
    assert_eq!(hint.path, raw);
}

#[test]
fn test_resolve_workspace_startup_hint_absolutizes_nonexistent_relative_cli_path() {
    let raw = PathBuf::from("does/not/exist/yet");

    let hint = with_workspace_env_cleared(|| resolve_workspace_startup_hint(Some(raw.clone())));

    assert_eq!(hint.source, Some(WorkspaceStartupSource::Cli));
    assert!(hint.path.is_absolute());
    assert_eq!(hint.path, std::env::current_dir().unwrap().join(raw));
}

#[test]
fn test_resolve_workspace_startup_hint_falls_back_to_env_source() {
    let temp = tempfile::tempdir().unwrap();

    let hint = with_workspace_env_set(temp.path(), || resolve_workspace_startup_hint(None));

    assert_eq!(hint.source, Some(WorkspaceStartupSource::Env));
    assert_eq!(hint.path, temp.path().canonicalize().unwrap());
}

#[test]
fn test_resolve_workspace_startup_hint_preserves_nonexistent_env_path() {
    let raw = nonexistent_absolute_path("nonexistent/env/path/that/does/not/exist");

    let hint = with_workspace_env_set(&raw, || resolve_workspace_startup_hint(None));

    assert_eq!(hint.source, Some(WorkspaceStartupSource::Env));
    assert_eq!(hint.path, raw);
}

#[test]
fn test_resolve_workspace_startup_hint_falls_back_to_cwd_source() {
    let hint = with_workspace_env_cleared(|| resolve_workspace_startup_hint(None));
    let cwd = std::env::current_dir().unwrap();

    assert_eq!(hint.source, Some(WorkspaceStartupSource::Cwd));
    assert_eq!(hint.path, cwd.canonicalize().unwrap_or(cwd));
}
