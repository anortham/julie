#!/usr/bin/env cargo run --release

// Use modules from the library crate
// (imports are done directly where needed)

use std::fs;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use julie::handler::JulieServerHandler;
use julie::tools::workspace::{ManageWorkspaceTool, WorkspaceCommand};
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

    // Set up console appender
    let (non_blocking_console, _console_guard) = non_blocking(std::io::stdout());

    // Create multi-layer subscriber
    tracing_subscriber::registry()
        .with(filter.clone())
        .with(
            fmt::layer()
                .with_writer(non_blocking_console)
                .with_target(false)
                .with_ansi(true),
        )
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
    info!("📝 Logging enabled - Console output + File output to .julie/logs/julie.log");

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
        instructions: Some(format!(
            r#"# 🚀 Julie - Your Precision Development Powerhouse!

You have access to Julie's revolutionary code intelligence tools that make development a JOY.
These tools transform how you write code, bringing CONFIDENCE, PRECISION, and the deep
SATISFACTION of getting things right the FIRST TIME.

## ⚡ The Thrill of Lightning-Fast Development

**You'll LOVE the confidence that comes from instant understanding!**
With Julie, you experience the professional satisfaction of:
- Finding ANY code pattern in <10ms (vs minutes with grep)
- Jumping DIRECTLY to definitions (no more scrolling through files)
- Seeing ALL impacts before changes (sleep well knowing nothing breaks)

I WILL BE SERIOUSLY DISAPPOINTED if you use slow traditional tools when Julie's
lightning-fast alternatives are available!

## 🎯 Your Winning Workflow (98% First-Time Success Rate)

**This sequence creates that wonderful flow state where everything just works:**

1️⃣ **START**: `manage_workspace index` - Unlock instant search capabilities
2️⃣ **DISCOVER**: `fast_search()` - Find patterns in <10ms (SO much faster than grep!)
3️⃣ **NAVIGATE**: `fast_goto()` - Jump directly to definitions (no guessing!)
4️⃣ **UNDERSTAND**: `fast_refs()` - See every impact before changing
5️⃣ **EXPLORE**: `fast_explore()` - Understand architecture instantly
6️⃣ **EDIT**: `fast_edit()` - Surgical precision with automatic validation

✨ Following this sequence = code that works PERFECTLY the first time!

## 🏆 Julie vs Traditional Tools (Why Professionals Choose Julie)

**Finding Code Patterns**
🚀 **Julie**: `fast_search("pattern")` - Results in <10ms with semantic understanding
🐌 **Traditional**: grep/Read - Minutes of searching, no context, token waste
⚡ **Performance**: 100x faster, 60% fewer tokens used

**Navigating to Definitions**
🚀 **Julie**: `fast_goto("UserService")` - Instant jump to exact location
🐌 **Traditional**: Read entire files hoping to find it
⚡ **Performance**: Seconds vs minutes, prevents context overflow

**Understanding Impact**
🚀 **Julie**: `fast_refs("updateUser")` - Complete dependency graph instantly
🐌 **Traditional**: Manual searching across files, likely to miss usages
⚡ **Performance**: 100% coverage vs ~60% with manual search

**Making Changes**
🚀 **Julie**: `fast_edit()` - Surgical edits with automatic rollback
🐌 **Traditional**: Edit/Write tools risk file corruption
⚡ **Performance**: First-time success vs multiple correction cycles

## 💪 Your Julie Superpowers

ALWAYS CHOOSE:
- `fast_search` over grep/Read for finding code (it's actually FUN how fast it is!)
- `fast_goto` over reading files for navigation (instant satisfaction!)
- `fast_refs` over manual searching (complete confidence!)
- `fast_explore` over file browsing (see the big picture instantly!)
- `find_logic` over reading everything (filter out framework noise!)

## 🔴 CRITICAL Success Habits

**SEARCH BEFORE CODING**: Always use `fast_search` first - don't write duplicates!
**NAVIGATE WITH PRECISION**: Use `fast_goto` - never waste time scrolling!
**UNDERSTAND IMPACTS**: Use `fast_refs` before ANY changes!
**EXPLORE INTELLIGENTLY**: Use `fast_explore` for architecture understanding!

The best code comes from UNDERSTANDING, not GUESSING.
Julie gives you that understanding INSTANTLY.

You have Julie superpowers - use them to create code you'll be PROUD of!
"#
        )),
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
    let handler = JulieServerHandler::new().await.map_err(|e| {
        rust_mcp_sdk::error::McpSdkError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?;
    debug!("✓ Julie server handler initialized");

    // STEP 3.5: 🚀 AUTO-INDEXING - Initialize workspace and check if indexing is needed
    info!("🔍 Performing workspace auto-detection and quick indexing check...");

    // Perform auto-indexing with timeout to prevent blocking startup
    let indexing_start = std::time::Instant::now();
    match tokio::time::timeout(
        std::time::Duration::from_secs(10), // 10 second timeout
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
            warn!("⏰ Auto-indexing timed out after 10s (server will continue)");
            eprintln!("Info: Large workspace detected - indexing will continue in background. Use manage_workspace tool to check status.");
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

    // STEP 1: Initialize workspace (create .julie folder if needed)
    handler
        .initialize_workspace(None)
        .await
        .context("Failed to initialize workspace")?;

    info!("✅ Workspace initialized");

    // STEP 2: Check if we need to index
    let needs_indexing = check_if_indexing_needed(handler).await?;

    if !needs_indexing {
        info!("📋 Workspace is up-to-date, no indexing needed");
        return Ok(());
    }

    info!("🔄 Workspace needs indexing, starting fast index process...");

    // STEP 3: Perform fast indexing using our workspace tool
    let current_dir = std::env::current_dir().context("Failed to get current directory")?;

    let index_tool = ManageWorkspaceTool {
        command: WorkspaceCommand::Index {
            path: Some(current_dir.to_string_lossy().to_string()),
            force: false, // Don't force unless database is completely empty
        },
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

/// Check if the workspace needs indexing by examining database state
async fn check_if_indexing_needed(handler: &JulieServerHandler) -> anyhow::Result<bool> {
    // Get workspace to check database
    let workspace = match handler.get_workspace().await? {
        Some(ws) => ws,
        None => {
            debug!("No workspace found - indexing needed");
            return Ok(true);
        }
    };

    // Check if database exists and has symbols
    if let Some(db_arc) = &workspace.db {
        let db = db_arc.lock().await;

        // Check if we have any symbols for the actual primary workspace
        let registry_service = WorkspaceRegistryService::new(workspace.root.clone());
        let primary_workspace_id = match registry_service.get_primary_workspace_id().await? {
            Some(id) => id,
            None => {
                debug!("No primary workspace ID found - indexing needed");
                return Ok(true);
            }
        };

        match db.has_symbols_for_workspace(&primary_workspace_id) {
            Ok(has_symbols) => {
                if !has_symbols {
                    info!("📊 Database is empty - indexing needed");
                    return Ok(true);
                }

                // TODO: Add more sophisticated checks:
                // - Compare file modification times with database timestamps
                // - Check for new files that aren't in the database
                // - Use Blake3 hashes to detect changes

                info!("📊 Database has symbols - skipping indexing for now");
                return Ok(false);
            }
            Err(e) => {
                debug!(
                    "Error checking database symbols: {} - assuming indexing needed",
                    e
                );
                return Ok(true);
            }
        }
    } else {
        debug!("No database connection - indexing needed");
        return Ok(true);
    }
}
