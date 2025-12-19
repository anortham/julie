#!/usr/bin/env cargo run --release

// Use modules from the library crate
// (imports are done directly where needed)

use std::env;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, error, info, warn};
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use julie::handler::JulieServerHandler;
use rmcp::{ServiceExt, transport::stdio};

/// Determine the workspace root path from CLI args, environment, or current directory
///
/// Priority order:
/// 1. --workspace <path> CLI argument
/// 2. JULIE_WORKSPACE environment variable
/// 3. Current working directory (fallback)
///
/// Paths are canonicalized to prevent duplicate workspace IDs for the same logical directory.
/// Tilde expansion is performed for paths like "~/projects/foo".
fn get_workspace_root() -> PathBuf {
    // Check CLI arguments for --workspace flag
    let args: Vec<String> = env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--workspace") {
        if let Some(path_str) = args.get(pos + 1) {
            // Expand tilde for paths like "~/projects/foo"
            let expanded = shellexpand::tilde(path_str).to_string();
            let path = PathBuf::from(expanded);

            if path.exists() {
                // Canonicalize to resolve symlinks and normalize path representation
                let canonical = path.canonicalize().unwrap_or_else(|e| {
                    eprintln!("⚠️ Warning: Could not canonicalize path {:?}: {}", path, e);
                    path.clone()
                });
                eprintln!("📂 Using workspace from CLI argument: {:?}", canonical);
                return canonical;
            } else {
                eprintln!("⚠️ Warning: --workspace path does not exist: {:?}", path);
            }
        }
    }

    // Check environment variable (e.g., JULIE_WORKSPACE set by VS Code)
    if let Ok(path_str) = env::var("JULIE_WORKSPACE") {
        // Expand tilde for paths like "~/projects/foo"
        let expanded = shellexpand::tilde(&path_str).to_string();
        let path = PathBuf::from(expanded);

        if path.exists() {
            // Canonicalize to resolve symlinks and normalize path representation
            let canonical = path.canonicalize().unwrap_or_else(|e| {
                eprintln!("⚠️ Warning: Could not canonicalize path {:?}: {}", path, e);
                path.clone()
            });
            eprintln!(
                "📂 Using workspace from JULIE_WORKSPACE env var: {:?}",
                canonical
            );
            return canonical;
        } else {
            eprintln!(
                "⚠️ Warning: JULIE_WORKSPACE path does not exist: {:?}",
                path
            );
        }
    }

    // Fallback to current directory
    let current = env::current_dir().unwrap_or_else(|e| {
        eprintln!("⚠️ Warning: Could not determine current directory: {}", e);
        eprintln!("Using fallback path '.'");
        PathBuf::from(".")
    });

    // Canonicalize current directory as well for consistency
    current.canonicalize().unwrap_or(current)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 🔧 CRITICAL: Determine workspace root BEFORE setting up logging
    // VS Code/MCP servers may start with arbitrary working directories
    // We support multiple detection methods (see get_workspace_root())
    let workspace_root = get_workspace_root();

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

    // Create the Julie server handler
    let handler = JulieServerHandler::new()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create handler: {}", e))?;

    info!("📋 Server configuration:");
    info!("  Name: Julie");
    info!("  Version: {}", env!("CARGO_PKG_VERSION"));

    // Start the periodic embedding engine cleanup task
    handler.start_embedding_cleanup_task();

    info!("🎯 Auto-indexing will run in background after MCP handshake completes");

    // Capture database reference for shutdown checkpoint
    let db_for_shutdown = if let Ok(Some(workspace)) = handler.get_workspace().await {
        workspace.db.clone()
    } else {
        None
    };

    info!("🎯 Julie server created and ready to start");
    info!("🔥 Starting Julie MCP server...");

    // Start the MCP server with stdio transport
    let service = match handler.serve(stdio()).await {
        Ok(s) => s,
        Err(e) => {
            error!("❌ Server failed to start: {}", e);
            return Err(anyhow::anyhow!("Server failed to start: {}", e));
        }
    };

    // Wait for the server to complete
    if let Err(e) = service.waiting().await {
        error!("❌ Server error: {}", e);
        return Err(anyhow::anyhow!("Server error: {}", e));
    }

    info!("🏁 Julie server stopped");

    // 🧹 SHUTDOWN CLEANUP: Checkpoint WAL before exit
    // This prevents unbounded WAL growth in long-running MCP server sessions
    info!("🧹 Performing shutdown cleanup...");
    if let Some(db_arc) = db_for_shutdown {
        match db_arc.lock() {
            Ok(mut db) => match db.checkpoint_wal() {
                Ok((busy, log, checkpointed)) => {
                    info!(
                        "✅ WAL checkpoint complete: busy={}, log={}, checkpointed={}",
                        busy, log, checkpointed
                    );
                }
                Err(e) => {
                    warn!("⚠️ WAL checkpoint failed: {}", e);
                }
            },
            Err(e) => {
                warn!("⚠️ Could not acquire database lock for checkpoint: {}", e);
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
