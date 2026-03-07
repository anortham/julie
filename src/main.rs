#!/usr/bin/env cargo run --release

// Use modules from the library crate
// (imports are done directly where needed)

use std::fs;
use tracing::{debug, error, info, warn};
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use clap::Parser;
use julie::cli::{Cli, Commands, DaemonAction, resolve_workspace_root};
use julie::handler::JulieServerHandler;
use rmcp::{ServiceExt, transport::stdio};

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

    // Branch on subcommand: None = stdio MCP mode (backward compatible), Some = daemon mode
    match cli.command {
        // Daemon mode (stubbed for now — Task 2 will implement lifecycle)
        Some(Commands::Daemon { action }) => {
            match action {
                DaemonAction::Start { port, foreground } => {
                    info!("Daemon start requested: port={}, foreground={}", port, foreground);
                    eprintln!(
                        "julie-server daemon start: not yet implemented (port={}, foreground={})",
                        port, foreground
                    );
                }
                DaemonAction::Stop => {
                    info!("Daemon stop requested");
                    eprintln!("julie-server daemon stop: not yet implemented");
                }
                DaemonAction::Status => {
                    info!("Daemon status requested");
                    eprintln!("julie-server daemon status: not yet implemented");
                }
            }
            Ok(())
        }

        // No subcommand: stdio MCP mode (backward compatible — this is the default)
        None => run_stdio_mode(workspace_root).await,
    }
}

/// Run Julie in stdio MCP mode (the original and default behavior).
///
/// MCP clients (Claude Code, etc.) spawn `julie-server` with no subcommand
/// and communicate over stdin/stdout using JSON-RPC.
async fn run_stdio_mode(workspace_root: std::path::PathBuf) -> anyhow::Result<()> {
    // Create the Julie server handler with the resolved workspace root
    let handler = JulieServerHandler::new(workspace_root)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create handler: {}", e))?;

    info!("Server configuration: Julie v{}", env!("CARGO_PKG_VERSION"));
    info!("Auto-indexing will run in background after MCP handshake completes");

    // Capture database reference for shutdown checkpoint
    let db_for_shutdown = if let Ok(Some(workspace)) = handler.get_workspace().await {
        workspace.db.clone()
    } else {
        None
    };

    info!("Starting Julie MCP server (stdio mode)...");

    // Start the MCP server with stdio transport
    let service = match handler.serve(stdio()).await {
        Ok(s) => s,
        Err(e) => {
            error!("Server failed to start: {}", e);
            return Err(anyhow::anyhow!("Server failed to start: {}", e));
        }
    };

    // Wait for the server to complete
    if let Err(e) = service.waiting().await {
        error!("Server error: {}", e);
        return Err(anyhow::anyhow!("Server error: {}", e));
    }

    info!("Julie server stopped");

    // Shutdown cleanup: checkpoint WAL before exit
    // This prevents unbounded WAL growth in long-running MCP server sessions
    info!("Performing shutdown cleanup...");
    if let Some(db_arc) = db_for_shutdown {
        match db_arc.lock() {
            Ok(mut db) => match db.checkpoint_wal() {
                Ok((busy, log, checkpointed)) => {
                    info!(
                        "WAL checkpoint complete: busy={}, log={}, checkpointed={}",
                        busy, log, checkpointed
                    );
                }
                Err(e) => {
                    warn!("WAL checkpoint failed: {}", e);
                }
            },
            Err(e) => {
                warn!("Could not acquire database lock for checkpoint: {}", e);
            }
        }
    } else {
        debug!("No database available for shutdown checkpoint (workspace not initialized)");
    }

    Ok(())
}
// AUTO-INDEXING MOVED: Now handled in handler.rs on_initialized() callback
// This ensures MCP handshake completes immediately before indexing begins
//
// perform_auto_indexing() and update_workspace_statistics() functions removed
// - Auto-indexing now runs via on_initialized() callback in ServerHandler trait
// - Statistics updates are handled by ManageWorkspaceTool during indexing
