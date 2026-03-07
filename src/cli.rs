use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "julie", version, about = "Julie - Cross-Platform Code Intelligence Server")]
pub struct Cli {
    /// Workspace root path (overrides JULIE_WORKSPACE env var)
    #[arg(long, global = true)]
    pub workspace: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run Julie as a persistent daemon
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
}

#[derive(Subcommand)]
pub enum DaemonAction {
    /// Start the Julie daemon
    Start {
        /// Port to listen on
        #[arg(long, default_value = "7890", env = "JULIE_PORT")]
        port: u16,
        /// Run in foreground (don't daemonize)
        #[arg(long)]
        foreground: bool,
    },
    /// Stop the running daemon
    Stop,
    /// Show daemon status
    Status,
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
pub fn resolve_workspace_root(cli_workspace: Option<PathBuf>) -> PathBuf {
    // 1. CLI argument (clap already parsed it, but we still need tilde expansion + canonicalization)
    if let Some(raw_path) = cli_workspace {
        let path_str = raw_path.to_string_lossy();
        let expanded = shellexpand::tilde(&path_str).to_string();
        let path = PathBuf::from(expanded);

        if path.exists() {
            let canonical = path.canonicalize().unwrap_or_else(|e| {
                eprintln!("Warning: Could not canonicalize path {:?}: {}", path, e);
                path.clone()
            });
            eprintln!("Using workspace from CLI argument: {:?}", canonical);
            return canonical;
        } else {
            eprintln!("Warning: --workspace path does not exist: {:?}", path);
        }
    }

    // 2. JULIE_WORKSPACE environment variable
    if let Ok(path_str) = std::env::var("JULIE_WORKSPACE") {
        let expanded = shellexpand::tilde(&path_str).to_string();
        let path = PathBuf::from(expanded);

        if path.exists() {
            let canonical = path.canonicalize().unwrap_or_else(|e| {
                eprintln!("Warning: Could not canonicalize path {:?}: {}", path, e);
                path.clone()
            });
            eprintln!(
                "Using workspace from JULIE_WORKSPACE env var: {:?}",
                canonical
            );
            return canonical;
        } else {
            eprintln!(
                "Warning: JULIE_WORKSPACE path does not exist: {:?}",
                path
            );
        }
    }

    // 3. Fallback to current directory
    let current = std::env::current_dir().unwrap_or_else(|e| {
        eprintln!("Warning: Could not determine current directory: {}", e);
        eprintln!("Using fallback path '.'");
        PathBuf::from(".")
    });

    current.canonicalize().unwrap_or(current)
}

