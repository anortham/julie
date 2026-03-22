use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "julie-server", version, about = "Julie - Code Intelligence Server")]
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
        /// HTTP port for Streamable HTTP transport (0 = auto-assign)
        #[arg(long, default_value = "0")]
        port: u16,
    },
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
            eprintln!("Warning: JULIE_WORKSPACE path does not exist: {:?}", path);
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
