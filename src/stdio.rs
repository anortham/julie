//! Stdio MCP mode — the original and default transport.
//!
//! MCP clients spawn `julie-server` (no subcommand) and communicate over
//! stdin/stdout using JSON-RPC. This module exposes the entry point so both
//! `main.rs` (no-subcommand path) and `connect.rs` (fallback path) can use it.

use std::path::PathBuf;

use anyhow::Result;
use tracing::{debug, error, info, warn};

use crate::handler::JulieServerHandler;

/// Run Julie in stdio MCP mode.
///
/// Creates a `JulieServerHandler` for the given workspace, starts the rmcp
/// stdio transport, and blocks until the MCP session ends. On exit, performs
/// a WAL checkpoint for clean shutdown.
///
/// Used as:
/// - The default mode when no subcommand is given
/// - The fallback when `connect` can't start the daemon
pub async fn run_stdio_mode(workspace_root: PathBuf) -> Result<()> {
    use rmcp::{ServiceExt, transport::stdio};

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
