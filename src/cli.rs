use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

#[derive(Parser)]
#[command(
    name = "julie-server",
    version,
    about = "Julie - Code Intelligence Server"
)]
pub struct Cli {
    /// Workspace root directory
    #[arg(long, global = true)]
    pub workspace: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Run as persistent daemon (HTTP + IPC transport)
    Daemon {
        /// HTTP port for dashboard (default: 7890, fallback to auto if taken)
        #[arg(long, default_value = "7890")]
        port: u16,
        /// Disable auto-opening dashboard in browser
        #[arg(long)]
        no_dashboard: bool,
    },
    /// Open the dashboard in the default browser
    Dashboard,
    /// Stop the running daemon
    Stop,
    /// Check daemon status
    Status,
    /// Stop daemon; it will auto-restart on next tool call
    Restart,
}

/// Resolve the workspace root path from CLI arg, env var, or current directory.
///
/// Priority order:
/// 1. `--workspace <path>` CLI argument (already parsed by clap)
/// 2. `JULIE_WORKSPACE` environment variable
/// 3. Current working directory (fallback)
///
/// Paths are canonicalized to prevent duplicate workspace IDs for the same logical directory.
/// Tilde expansion is performed for paths like "~/projects/foo".
pub fn resolve_workspace_startup_hint(cli_workspace: Option<PathBuf>) -> WorkspaceStartupHint {
    if let Some(path) = resolve_explicit_workspace_candidate(
        cli_workspace,
        "CLI argument",
        "--workspace path does not exist",
    ) {
        return WorkspaceStartupHint {
            path,
            source: Some(WorkspaceStartupSource::Cli),
        };
    }

    if let Some(path) = resolve_explicit_workspace_candidate(
        std::env::var("JULIE_WORKSPACE").ok().map(PathBuf::from),
        "JULIE_WORKSPACE env var",
        "JULIE_WORKSPACE path does not exist",
    ) {
        return WorkspaceStartupHint {
            path,
            source: Some(WorkspaceStartupSource::Env),
        };
    }

    let current = std::env::current_dir().unwrap_or_else(|e| {
        eprintln!("Warning: Could not determine current directory: {}", e);
        eprintln!("Using fallback path '.'");
        PathBuf::from(".")
    });

    WorkspaceStartupHint {
        path: current.canonicalize().unwrap_or(current),
        source: Some(WorkspaceStartupSource::Cwd),
    }
}

pub fn resolve_workspace_root(cli_workspace: Option<PathBuf>) -> PathBuf {
    resolve_workspace_startup_hint(cli_workspace).path
}

fn resolve_explicit_workspace_candidate(
    raw_path: Option<PathBuf>,
    source_label: &str,
    missing_warning: &str,
) -> Option<PathBuf> {
    let raw_path = raw_path?;
    let path_str = raw_path.to_string_lossy();
    let expanded = shellexpand::tilde(&path_str).to_string();
    let path = PathBuf::from(expanded);
    let absolute_path = if path.is_absolute() {
        path.clone()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|e| {
                eprintln!("Warning: Could not determine current directory: {}", e);
                PathBuf::from(".")
            })
            .join(&path)
    };

    if !absolute_path.exists() {
        eprintln!("Warning: {}: {:?}", missing_warning, absolute_path);
        return Some(absolute_path);
    }

    let canonical = absolute_path.canonicalize().unwrap_or_else(|e| {
        eprintln!(
            "Warning: Could not canonicalize path {:?}: {}",
            absolute_path, e
        );
        absolute_path.clone()
    });
    eprintln!("Using workspace from {}: {:?}", source_label, canonical);
    Some(canonical)
}
