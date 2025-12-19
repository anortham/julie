//! Reference workspace symbol retrieval
//!
//! Handles getting symbols from reference (non-primary) workspaces.

use anyhow::Result;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use tracing::{debug, info, warn};

use super::body_extraction::extract_code_bodies;
use super::filtering::apply_all_filters;
use super::formatting::format_symbol_response;
use crate::handler::JulieServerHandler;
use crate::workspace::registry_service::WorkspaceRegistryService;

/// Get symbols from a reference workspace
pub async fn get_symbols_from_reference(
    handler: &JulieServerHandler,
    file_path: &str,
    max_depth: u32,
    target: Option<&str>,
    limit: Option<u32>,
    mode: &str,
    ref_workspace_id: String,
    output_format: Option<&str>,
) -> Result<CallToolResult> {
    info!(
        "📋 Getting symbols from reference workspace: {} in file: {} (depth: {})",
        ref_workspace_id, file_path, max_depth
    );

    // Get primary workspace to access helper methods
    let primary_workspace = handler
        .get_workspace()
        .await?
        .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;

    // Get path to reference workspace's separate database file
    let ref_db_path = primary_workspace.workspace_db_path(&ref_workspace_id);

    debug!(
        "🗄️ Opening reference workspace DB: {}",
        ref_db_path.display()
    );

    // Get reference workspace entry to access its original_path (workspace root)
    let registry_service = WorkspaceRegistryService::new(primary_workspace.root.clone());
    let ref_workspace_entry = registry_service
        .get_workspace(&ref_workspace_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Reference workspace not found: {}", ref_workspace_id))?;

    // 🚨 CRITICAL FIX: Wrap blocking file I/O in spawn_blocking
    // Opening SQLite database involves blocking filesystem operations
    let ref_db =
        tokio::task::spawn_blocking(move || crate::database::SymbolDatabase::new(ref_db_path))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to spawn database open task: {}", e))??;

    // Phase 2: Database stores relative Unix-style paths
    // Reference workspace root is from WorkspaceEntry.original_path
    let ref_workspace_root = std::path::PathBuf::from(&ref_workspace_entry.original_path);

    let (query_path, absolute_path) = if std::path::Path::new(file_path).is_absolute() {
        // Absolute path input
        let canonical = std::path::Path::new(file_path)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(file_path));

        let relative = crate::utils::paths::to_relative_unix_style(&canonical, &ref_workspace_root)
            .unwrap_or_else(|_| {
                warn!("Failed to convert absolute path to relative: {}", file_path);
                file_path.to_string()
            });

        (relative, canonical.to_string_lossy().to_string())
    } else {
        // Relative path input - normalize separators for query, join for absolute
        let relative_unix = file_path.replace('\\', "/");
        let absolute = ref_workspace_root
            .join(file_path)
            .canonicalize()
            .unwrap_or_else(|_| ref_workspace_root.join(file_path))
            .to_string_lossy()
            .to_string();

        (relative_unix, absolute)
    };

    debug!(
        "🔍 Path normalization: '{}' -> query='{}', absolute='{}' (ref workspace: {})",
        file_path, query_path, absolute_path, ref_workspace_id
    );

    // Check if file exists before querying database
    if !std::path::Path::new(&absolute_path).exists() {
        let message = format!(
            "❌ File not found: {}\n💡 Check the file path - use relative paths from workspace root",
            file_path
        );
        return Ok(CallToolResult::text_content(vec![Content::text(
            message,
        )]));
    }

    // Query symbols using relative Unix-style path
    // ✅ NO MUTEX: ref_db is owned (not Arc<Mutex<>>), so we can call directly
    let symbols = ref_db
        .get_symbols_for_file(&query_path)
        .map_err(|e| anyhow::anyhow!("Failed to get symbols: {}", e))?;

    if symbols.is_empty() {
        let message = format!("No symbols found in: {}", file_path);
        return Ok(CallToolResult::text_content(vec![Content::text(
            message,
        )]));
    }

    // Apply all filters and get the final symbol list
    let (symbols_to_return, was_truncated, total_symbols) =
        apply_all_filters(symbols, max_depth, target, limit);

    if symbols_to_return.is_empty() {
        let message = format!("No symbols found after filtering in: {}", file_path);
        return Ok(CallToolResult::text_content(vec![Content::text(
            message,
        )]));
    }

    // Extract code bodies based on mode
    let symbols_to_return = extract_code_bodies(symbols_to_return, &absolute_path, mode)?;

    // Format and return the response
    format_symbol_response(
        file_path,
        symbols_to_return,
        total_symbols,
        max_depth,
        target,
        limit,
        was_truncated,
        Some(ref_workspace_id),
        output_format,
    )
}
