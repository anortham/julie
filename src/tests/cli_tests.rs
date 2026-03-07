//! Tests for CLI argument parsing (clap) and workspace resolution.

use crate::cli::{Cli, Commands, DaemonAction, resolve_workspace_root};
use clap::Parser;
use std::path::PathBuf;

// ============================================================================
// CLI PARSING TESTS
// ============================================================================

#[test]
fn test_no_args_parses_to_stdio_mode() {
    let cli = Cli::parse_from(["julie-server"]);
    assert!(cli.command.is_none(), "No subcommand should mean stdio MCP mode");
    assert!(cli.workspace.is_none());
}

#[test]
fn test_workspace_flag_parsed() {
    let cli = Cli::parse_from(["julie-server", "--workspace", "/tmp/myproject"]);
    assert!(cli.command.is_none());
    assert_eq!(cli.workspace, Some(PathBuf::from("/tmp/myproject")));
}

#[test]
fn test_daemon_start_default_port() {
    let cli = Cli::parse_from(["julie-server", "daemon", "start"]);
    match cli.command {
        Some(Commands::Daemon {
            action: DaemonAction::Start { port, foreground },
        }) => {
            assert_eq!(port, 7890);
            assert!(!foreground);
        }
        other => panic!("Expected Daemon Start, got command.is_some()={}", other.is_some()),
    }
}

#[test]
fn test_daemon_start_custom_port() {
    let cli = Cli::parse_from(["julie-server", "daemon", "start", "--port", "8080"]);
    match cli.command {
        Some(Commands::Daemon {
            action: DaemonAction::Start { port, foreground },
        }) => {
            assert_eq!(port, 8080);
            assert!(!foreground);
        }
        other => panic!("Expected Daemon Start, got command.is_some()={}", other.is_some()),
    }
}

#[test]
fn test_daemon_start_foreground() {
    let cli = Cli::parse_from(["julie-server", "daemon", "start", "--foreground"]);
    match cli.command {
        Some(Commands::Daemon {
            action: DaemonAction::Start { foreground, .. },
        }) => {
            assert!(foreground);
        }
        other => panic!("Expected Daemon Start, got command.is_some()={}", other.is_some()),
    }
}

#[test]
fn test_daemon_stop() {
    let cli = Cli::parse_from(["julie-server", "daemon", "stop"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Daemon {
            action: DaemonAction::Stop
        })
    ));
}

#[test]
fn test_daemon_status() {
    let cli = Cli::parse_from(["julie-server", "daemon", "status"]);
    assert!(matches!(
        cli.command,
        Some(Commands::Daemon {
            action: DaemonAction::Status
        })
    ));
}

#[test]
fn test_workspace_global_with_daemon() {
    let cli = Cli::parse_from([
        "julie-server",
        "--workspace",
        "/tmp/myproject",
        "daemon",
        "start",
    ]);
    assert_eq!(cli.workspace, Some(PathBuf::from("/tmp/myproject")));
    assert!(matches!(
        cli.command,
        Some(Commands::Daemon {
            action: DaemonAction::Start { .. }
        })
    ));
}

#[test]
fn test_workspace_after_subcommand_also_works() {
    // clap global args can appear after the subcommand too
    let cli = Cli::parse_from([
        "julie-server",
        "daemon",
        "start",
        "--workspace",
        "/tmp/myproject",
    ]);
    assert_eq!(cli.workspace, Some(PathBuf::from("/tmp/myproject")));
    assert!(matches!(
        cli.command,
        Some(Commands::Daemon {
            action: DaemonAction::Start { .. }
        })
    ));
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
    let result = resolve_workspace_root(Some(PathBuf::from(
        "/nonexistent/path/that/does/not/exist",
    )));
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
