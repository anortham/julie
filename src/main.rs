#!/usr/bin/env cargo run --release

// Use modules from the library crate
// (imports are done directly where needed)

use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use julie::handler::JulieServerHandler;
use rust_mcp_sdk::schema::{
    Implementation, InitializeResult, LATEST_PROTOCOL_VERSION, ServerCapabilities,
    ServerCapabilitiesTools,
};

use rust_mcp_sdk::{
    McpServer, StdioTransport, TransportOptions,
    error::SdkResult,
    mcp_server::{ServerRuntime, server_runtime},
};

/// Load agent instructions from JULIE_AGENT_INSTRUCTIONS.md
fn load_agent_instructions() -> String {
    // Try to load from file first
    match fs::read_to_string("JULIE_AGENT_INSTRUCTIONS.md") {
        Ok(content) => {
            info!("📖 Loaded agent instructions from JULIE_AGENT_INSTRUCTIONS.md");
            content
        }
        Err(e) => {
            warn!("⚠️  Could not load JULIE_AGENT_INSTRUCTIONS.md: {}", e);
            warn!("📝 Using minimal fallback instructions");
            // Minimal fallback instructions
            r#"# Julie - Code Intelligence Server

## Quick Start
1. Index your workspace: `manage_workspace operation="index"`
2. Search code: `fast_search query="your_search"`
3. Navigate: `fast_goto symbol="SymbolName"`
4. Find references: `fast_refs symbol="SymbolName"`

## Key Tools
- **get_symbols**: See file structure without reading full content
- **trace_call_path**: Trace execution flow across languages (unique!)
- **fast_search**: Instant semantic + text search
- **fast_explore**: Understand architecture

Use Julie for INTELLIGENCE, built-in tools for MECHANICS."#
                .to_string()
        }
    }
}

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
async fn main() -> SdkResult<()> {
    // 🔧 CRITICAL: Determine workspace root BEFORE setting up logging
    // VS Code/MCP servers may start with arbitrary working directories
    // We support multiple detection methods (see get_workspace_root())
    let workspace_root = get_workspace_root();

    // Initialize logging with both console and file output
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("julie=info"))
        .map_err(|e| {
            rust_mcp_sdk::error::McpSdkError::Io(std::io::Error::other(format!(
                "Failed to initialize logging filter: {}",
                e
            )))
        })?;

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

    // STEP 1: Define server details and capabilities
    let server_details = InitializeResult {
        server_info: Implementation {
            name: "Julie".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            title: Some("Julie - Cross-Platform Code Intelligence Server".to_string()),
        },
        capabilities: ServerCapabilities {
            tools: Some(ServerCapabilitiesTools { list_changed: None }),
            ..Default::default()
        },
        meta: None,
        instructions: Some(load_agent_instructions()),
        protocol_version: LATEST_PROTOCOL_VERSION.to_string(),
    };

    info!("📋 Server configuration:");
    info!("  Name: {}", server_details.server_info.name);
    info!("  Version: {}", server_details.server_info.version);
    info!("  Protocol: {}", server_details.protocol_version);

    // STEP 2: Create stdio transport with default options
    let transport = StdioTransport::new(TransportOptions::default())?;
    debug!("✓ STDIO transport initialized");

    // STEP 3: Instantiate our custom handler
    let handler = JulieServerHandler::new()
        .await
        .map_err(|e| rust_mcp_sdk::error::McpSdkError::Io(std::io::Error::other(e.to_string())))?;
    debug!("✓ Julie server handler initialized");

    // STEP 3.1: 🕐 Start the periodic embedding engine cleanup task
    // This task checks every minute if the engine has been idle >5 minutes and drops it
    handler.start_embedding_cleanup_task();

    // STEP 3.5: 🚀 AUTO-INDEXING moved to on_initialized() callback in handler.rs
    // This ensures the MCP handshake completes immediately without blocking
    // The workspace will be indexed in the background after the client connects
    info!("🎯 Auto-indexing will run in background after MCP handshake completes");

    // STEP 3.9: 🗂️ Capture database reference for shutdown checkpoint
    // We need this before moving handler into create_server()
    let db_for_shutdown = if let Ok(Some(workspace)) = handler.get_workspace().await {
        workspace.db.clone()
    } else {
        None
    };

    // STEP 4: Create MCP server
    let server: Arc<ServerRuntime> =
        server_runtime::create_server(server_details, transport, handler);

    info!("🎯 Julie server created and ready to start");

    // STEP 5: Start the server
    info!("🔥 Starting Julie MCP server...");
    if let Err(start_error) = server.start().await {
        error!("❌ Server failed to start: {}", start_error);
        eprintln!(
            "Julie server error: {}",
            start_error
                .rpc_error_message()
                .unwrap_or(&start_error.to_string())
        );
        return Err(start_error);
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
