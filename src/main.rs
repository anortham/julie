#!/usr/bin/env cargo run --release

// Use modules from the library crate
// (imports are done directly where needed)

use std::fs;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use julie::handler::JulieServerHandler;
use julie::tools::workspace::ManageWorkspaceTool;
use julie::workspace::registry::{EmbeddingStatus, WorkspaceType};
use julie::workspace::registry_service::WorkspaceRegistryService;
use rust_mcp_sdk::schema::{
    Implementation, InitializeResult, ServerCapabilities, ServerCapabilitiesTools,
    LATEST_PROTOCOL_VERSION,
};

use rust_mcp_sdk::{
    error::SdkResult,
    mcp_server::{server_runtime, ServerRuntime},
    McpServer, StdioTransport, TransportOptions,
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

#[tokio::main]
async fn main() -> SdkResult<()> {
    // Initialize logging with both console and file output
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("julie=info"))
        .unwrap();

    // Ensure .julie/logs directory exists
    let logs_dir = ".julie/logs";
    fs::create_dir_all(logs_dir).unwrap_or_else(|e| {
        eprintln!("Failed to create logs directory: {}", e);
    });

    // Set up file appender with daily rolling
    let file_appender = rolling::daily(logs_dir, "julie.log");
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
    info!("📝 Logging enabled - File output to .julie/logs/julie.log");

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

    // STEP 3.5: 🚀 AUTO-INDEXING - Initialize workspace and check if indexing is needed
    info!("🔍 Performing workspace auto-detection and quick indexing check...");

    // Perform auto-indexing with timeout to prevent blocking startup
    // TIMEOUT: 60s allows ~300 files to index (tested: 290 files took ~15s)
    // Background embedding generation will continue after indexing completes
    let indexing_start = std::time::Instant::now();
    match tokio::time::timeout(
        std::time::Duration::from_secs(60), // 60 second timeout for large workspaces
        perform_auto_indexing(&handler),
    )
    .await
    {
        Ok(Ok(_)) => {
            let duration = indexing_start.elapsed();
            info!(
                "✅ Auto-indexing completed in {:.2}s",
                duration.as_secs_f64()
            );
        }
        Ok(Err(e)) => {
            warn!("⚠️ Auto-indexing failed: {} (server will continue)", e);
            eprintln!("Warning: Auto-indexing failed: {}. You can run manual indexing later with the manage_workspace tool.", e);
        }
        Err(_) => {
            warn!("⏰ Auto-indexing timed out after 60s (server will continue)");
            eprintln!("Info: Very large workspace detected - indexing will continue in background. Use manage_workspace tool to check status.");
        }
    }

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
    Ok(())
}

/// Perform auto-indexing on server startup
///
/// This function:
/// 1. Detects if workspace needs initialization
/// 2. Checks if database is empty or stale
/// 3. Performs fast indexing using bulk SQLite operations
/// 4. Starts background tasks for Tantivy and embeddings
async fn perform_auto_indexing(handler: &JulieServerHandler) -> anyhow::Result<()> {
    use anyhow::Context;

    info!("🔍 Starting auto-indexing process...");

    // STEP 1: Check if we need indexing BEFORE creating any folders
    let current_dir = std::env::current_dir().context("Failed to get current directory")?;
    let julie_dir = current_dir.join(".julie");

    let needs_indexing = if !julie_dir.exists() {
        info!("📁 No .julie folder found - this is a new project, indexing needed");
        true
    } else {
        // Initialize workspace to check existing state
        handler
            .initialize_workspace(None)
            .await
            .context("Failed to initialize workspace")?;
        info!("✅ Workspace initialized");

        // Check if existing workspace needs indexing
        julie::startup::check_if_indexing_needed(handler).await?
    };

    if !needs_indexing {
        info!("📋 Workspace is up-to-date, no indexing needed");

        // Even though no indexing is needed, update statistics to keep registry in sync
        update_workspace_statistics(&current_dir, handler).await?;

        return Ok(());
    }

    info!("🔄 Workspace needs indexing, starting fast index process...");

    // STEP 2: Initialize workspace if not already done (for new projects)
    if !julie_dir.exists() {
        handler
            .initialize_workspace(None)
            .await
            .context("Failed to initialize workspace")?;
        info!("✅ Workspace initialized");
    }

    // STEP 3: Perform fast indexing using our workspace tool

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(current_dir.to_string_lossy().to_string()),
        force: Some(false), // Don't force unless database is completely empty
        name: None,
        workspace_id: None,
        detailed: None,
    };

    // Perform the indexing
    let index_result = match index_tool.call_tool(handler).await {
        Ok(result) => result,
        Err(e) => {
            return Err(e).context("Failed to perform workspace indexing");
        }
    };

    // Log the result
    debug!("Indexing result: {:?}", index_result);

    info!("🚀 Auto-indexing completed successfully!");
    Ok(())
}

// check_if_indexing_needed() moved to src/startup.rs for testing
// Use julie::startup::check_if_indexing_needed() instead

/// Update workspace registry statistics with current database state
/// This ensures the registry stays in sync even when no indexing is performed
async fn update_workspace_statistics(
    workspace_path: &std::path::Path,
    handler: &JulieServerHandler,
) -> anyhow::Result<()> {
    use anyhow::Context;

    debug!("🔄 Updating workspace statistics...");

    // Get workspace
    let workspace = match handler.get_workspace().await? {
        Some(ws) => ws,
        None => {
            debug!("No workspace found - skipping statistics update");
            return Ok(());
        }
    };

    // Get registry service
    let registry_service = WorkspaceRegistryService::new(workspace_path.to_path_buf());

    // Get or register primary workspace
    let workspace_id = match registry_service
        .register_workspace(
            workspace_path.to_string_lossy().to_string(),
            WorkspaceType::Primary,
        )
        .await
    {
        Ok(entry) => entry.id,
        Err(_) => {
            // Already registered - get existing ID
            match registry_service.get_primary_workspace_id().await? {
                Some(id) => id,
                None => {
                    debug!("Failed to get primary workspace ID - skipping statistics update");
                    return Ok(());
                }
            }
        }
    };

    // Count symbols and files in database
    let (symbol_count, file_count) = if let Some(db_arc) = &workspace.db {
        let db = db_arc.lock().unwrap();
        let symbols = db.get_symbol_count_for_workspace().unwrap_or(0) as usize;
        let files = db.get_file_count_for_workspace().unwrap_or(0) as usize;
        (symbols, files)
    } else {
        (0, 0)
    };

    // Calculate index size (SQLite database size)
    // 🚨 CRITICAL: Move blocking filesystem operations to spawn_blocking
    let db_path = workspace
        .root
        .join(".julie/indexes")
        .join(&workspace_id)
        .join("db");
    let index_size = if db_path.exists() {
        let db_path_clone = db_path.clone();
        match tokio::task::spawn_blocking(move || {
            julie::tools::workspace::calculate_dir_size(&db_path_clone)
        })
        .await
        {
            Ok(Ok(size)) => size,
            Ok(Err(e)) => {
                warn!("Failed to calculate index size: {}", e);
                0
            }
            Err(e) => {
                warn!("Index size calculation task failed: {}", e);
                0
            }
        }
    } else {
        0
    };

    // Update registry statistics
    registry_service
        .update_workspace_statistics(&workspace_id, symbol_count, file_count, index_size)
        .await
        .context("Failed to update workspace statistics")?;

    // Reconcile embedding status - fix registry if embeddings exist but status is wrong
    let embedding_count = if let Some(db_arc) = &workspace.db {
        let db = db_arc.lock().unwrap();
        db.count_embeddings().unwrap_or(0)
    } else {
        0
    };

    if embedding_count > 0 {
        // Embeddings exist, ensure registry shows "Ready"
        if let Err(e) = registry_service
            .update_embedding_status(&workspace_id, EmbeddingStatus::Ready)
            .await
        {
            warn!("Failed to reconcile embedding status: {}", e);
        } else {
            debug!(
                "📝 Reconciled embedding status: {} embeddings found, registry updated to Ready",
                embedding_count
            );
        }
    }

    info!(
        "📊 Updated workspace statistics: {} files, {} symbols, {:.2} MB index",
        file_count,
        symbol_count,
        index_size as f64 / 1_048_576.0
    );

    Ok(())
}

// calculate_dir_size moved to shared utility: src/tools/workspace/utils.rs
// Use julie::tools::workspace::calculate_dir_size() instead
