#!/usr/bin/env cargo run --release

// Use modules from the library crate
// (imports are done directly where needed)

use std::fs;
use tracing::{debug, info};
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use clap::Parser;
use julie::cli::{Cli, Commands, DaemonAction, resolve_workspace_root};
use julie::daemon;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse CLI arguments with clap
    let cli = Cli::parse();

    // Resolve workspace root BEFORE setting up logging
    // VS Code/MCP servers may start with arbitrary working directories
    // Priority: --workspace flag > JULIE_WORKSPACE env > current directory
    let workspace_root = resolve_workspace_root(cli.workspace);

    // Initialize logging with both console and file output
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("julie=info"))
        .map_err(|e| anyhow::anyhow!("Failed to initialize logging filter: {}", e))?;

    // Ensure .julie/logs directory exists in the workspace root
    let logs_dir = workspace_root.join(".julie").join("logs");
    fs::create_dir_all(&logs_dir).unwrap_or_else(|e| {
        eprintln!("Failed to create logs directory at {:?}: {}", logs_dir, e);
    });

    // Set up file appender with daily rolling
    let file_appender = rolling::daily(&logs_dir, "julie.log");
    let (non_blocking_file, _file_guard) = non_blocking(file_appender);

    // 🔥 CRITICAL FIX: MCP servers MUST NOT log to stdout
    // stdout is reserved exclusively for JSON-RPC messages
    // Any text logging breaks the MCP protocol parser in VS Code/Copilot
    // ALL logging goes to file only: .julie/logs/julie.log
    tracing_subscriber::registry()
        .with(filter.clone())
        .with(
            fmt::layer()
                .with_writer(non_blocking_file)
                .with_target(true)
                .with_ansi(false)
                .with_file(true)
                .with_line_number(true),
        )
        .init();

    info!("🚀 Starting Julie - Cross-Platform Code Intelligence Server");
    debug!("Built with Rust for true cross-platform compatibility");
    info!(
        "📝 Logging enabled - File output to {:?}",
        logs_dir.join("julie.log")
    );
    info!("📂 Workspace root: {:?}", workspace_root);

    // Branch on subcommand: None = stdio MCP mode (backward compatible), Some = daemon/connect mode
    match cli.command {
        // Daemon mode: start/stop/status lifecycle management
        Some(Commands::Daemon { action }) => {
            match action {
                DaemonAction::Start { port, foreground } => {
                    info!("Daemon start requested: port={}, foreground={}", port, foreground);
                    daemon::daemon_start(port, workspace_root.clone(), foreground).await?;
                }
                DaemonAction::Stop => {
                    info!("Daemon stop requested");
                    daemon::daemon_stop()?;
                }
                DaemonAction::Status => {
                    info!("Daemon status requested");
                    daemon::daemon_status()?;
                }
            }
            Ok(())
        }

        // Connect mode: auto-start daemon + stdio↔HTTP bridge
        Some(Commands::Connect { port }) => {
            info!("Connect mode requested: port={}", port);
            julie::connect::run_connect(port, workspace_root).await
        }

        // No subcommand: stdio MCP mode (backward compatible — this is the default)
        None => julie::stdio::run_stdio_mode(workspace_root).await,
    }
}

// AUTO-INDEXING MOVED: Now handled in handler.rs on_initialized() callback
// This ensures MCP handshake completes immediately before indexing begins
//
// perform_auto_indexing() and update_workspace_statistics() functions removed
// - Auto-indexing now runs via on_initialized() callback in ServerHandler trait
// - Statistics updates are handled by ManageWorkspaceTool during indexing
