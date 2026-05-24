//! Primary workspace symbol retrieval
//!
//! Handles getting symbols from the primary (active) workspace.

use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::{Result, bail};
use tracing::{debug, info, warn};

use super::body_extraction::extract_code_bodies;
use super::filtering::apply_all_filters;
use super::formatting::format_symbol_response;
use crate::handler::JulieServerHandler;

/// Get symbols from the primary workspace
pub async fn get_symbols_from_primary(
    handler: &JulieServerHandler,
    file_path: &str,
    max_depth: u32,
    target: Option<&str>,
    limit: Option<u32>,
    mode: &str,
) -> Result<CallToolResult> {
    info!(
        "📋 Getting symbols for file: {} (depth: {})",
        file_path, max_depth
    );

    let binding = handler.require_primary_workspace_binding()?;
    let current_workspace_root = binding.workspace_root;
    let db = handler
        .primary_pooled_database()
        .await?
        .into_read_snapshot()?;

    // Phase 2: Database stores relative Unix-style paths for token efficiency
    // We need TWO paths:
    // 1. query_path: Relative Unix-style for database queries
    // 2. absolute_path: Absolute path for file I/O (extract_code_bodies)

    let input_is_absolute = std::path::Path::new(file_path).is_absolute();
    let resolution =
        crate::utils::paths::resolve_workspace_file_input(file_path, &current_workspace_root);
    let query_path = resolution.relative_query_path.unwrap_or_else(|_| {
        if input_is_absolute {
            warn!("Failed to convert absolute path to relative: {}", file_path);
            file_path.to_string()
        } else {
            warn!("Failed to convert path to relative: {}", file_path);
            file_path.replace('\\', "/")
        }
    });
    let absolute_path = resolution.absolute_path.to_string_lossy().to_string();

    debug!(
        "🔍 Path normalization: '{}' -> query='{}', absolute='{}'",
        file_path, query_path, absolute_path
    );
    debug!("🔍 Workspace root: '{}'", current_workspace_root.display());

    // Check if file exists before querying database
    if !std::path::Path::new(&absolute_path).exists() {
        bail!(super::file_not_found_message(file_path, target));
    }

    // Query symbols for this file using relative Unix-style path.
    // In structure mode, use lightweight query that skips expensive columns
    // (code_context, metadata, semantic_group, confidence, content_type).
    let symbols = if mode == "structure" {
        db.get_symbols_for_file_lightweight(&query_path)
            .map_err(|e| anyhow::anyhow!("Failed to get symbols: {}", e))?
    } else {
        db.get_symbols_for_file(&query_path)
            .map_err(|e| anyhow::anyhow!("Failed to get symbols: {}", e))?
    };

    if symbols.is_empty() {
        let message = format!("No symbols found in: {}", file_path);
        return Ok(CallToolResult::text_content(vec![Content::text(message)]));
    }

    // Apply all filters and get the final symbol list
    let (symbols_to_return, _was_truncated, _total_symbols) =
        apply_all_filters(symbols, max_depth, target, limit);

    if symbols_to_return.is_empty() {
        let message = format!("No symbols found after filtering in: {}", file_path);
        return Ok(CallToolResult::text_content(vec![Content::text(message)]));
    }

    // Extract code bodies based on mode
    // When target is set, upgrade "minimal" to "full" — the user explicitly asked for this
    // symbol, so always include its body even if it's a child (has parent_id).
    let body_mode = if target.is_some() && mode == "minimal" {
        "full"
    } else {
        mode
    };
    let symbols_to_return = extract_code_bodies(symbols_to_return, &absolute_path, body_mode)?;

    // Format and return the response
    format_symbol_response(file_path, symbols_to_return, target)
}
