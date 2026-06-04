//! Target workspace symbol retrieval.
//!
//! Handles getting symbols from explicit non-primary workspaces.

use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::{Result, bail};
use tracing::{debug, info};

use super::body_extraction::extract_code_bodies;
use super::filtering::apply_all_filters;
use super::formatting::format_symbol_response;
use julie_context::ToolContext;

/// Get symbols from a target workspace.
pub async fn get_symbols_from_target_workspace(
    handler: &dyn ToolContext,
    file_path: &str,
    max_depth: u32,
    target: Option<&str>,
    limit: Option<u32>,
    mode: &str,
    target_workspace_id: String,
) -> Result<CallToolResult> {
    info!(
        "📋 Getting symbols from workspace: {} in file: {} (depth: {})",
        target_workspace_id, file_path, max_depth
    );

    // Pooled DB: read-only access, no mutation gate required. Workspace root
    // lookup supplies target-root normalization when available; absolute inputs
    // keep their existing fallback if the root lookup fails.
    let pooled_db = handler
        .get_pooled_database_for_workspace(&target_workspace_id)
        .await?
        .into_read_snapshot()?;

    // Strict contract: `resolve_workspace_file_input` rejects outside-workspace
    // paths with a typed `WorkspaceResolutionFailure`. We propagate via `?` so
    // the MCP boundary can surface `invalid_params` instead of silently feeding
    // a raw path string to the database.
    let input_is_absolute = std::path::Path::new(file_path).is_absolute();
    let (query_path, absolute_path) = match handler
        .get_workspace_root_for_target(&target_workspace_id)
        .await
    {
        Ok(target_workspace_root) => {
            debug!(
                "🗄️ Target workspace DB via handler helper, root: {}",
                target_workspace_root.display()
            );

            let resolution = crate::utils::paths::resolve_workspace_file_input(
                file_path,
                &target_workspace_root,
            )?;

            (
                resolution.relative_query_path,
                resolution.absolute_path.to_string_lossy().to_string(),
            )
        }
        Err(_) if input_is_absolute => {
            // Workspace root lookup failed (target not currently active) — we
            // can't enforce the workspace boundary, so we accept the absolute
            // path as-is. This is the only documented fallback path.
            let canonical = std::path::Path::new(file_path)
                .canonicalize()
                .unwrap_or_else(|_| std::path::PathBuf::from(file_path));

            (
                file_path.to_string(),
                canonical.to_string_lossy().to_string(),
            )
        }
        Err(err) => return Err(err),
    };

    debug!(
        "🔍 Path normalization: '{}' -> query='{}', absolute='{}' (workspace: {})",
        file_path, query_path, absolute_path, target_workspace_id
    );

    // Check if file exists before querying database
    if !std::path::Path::new(&absolute_path).exists() {
        bail!(super::file_not_found_message(file_path, target));
    }

    // Query symbols using relative Unix-style path via pooled DB.
    // In structure mode, use lightweight query that skips expensive columns.
    let mode_owned = mode.to_string();
    let query_path_clone = query_path.clone();
    let mut symbols = if mode_owned == "structure" {
        pooled_db
            .get_symbols_for_file_lightweight(&query_path_clone)
            .map_err(|e| anyhow::anyhow!("Failed to get symbols: {}", e))?
    } else {
        pooled_db
            .get_symbols_for_file(&query_path_clone)
            .map_err(|e| anyhow::anyhow!("Failed to get symbols: {}", e))?
    };

    if symbols.is_empty() && query_path != file_path {
        let fallback_query = file_path.replace('\\', "/");
        symbols = if mode_owned == "structure" {
            pooled_db
                .get_symbols_for_file_lightweight(&fallback_query)
                .map_err(|e| anyhow::anyhow!("Failed to get symbols: {}", e))?
        } else {
            pooled_db
                .get_symbols_for_file(&fallback_query)
                .map_err(|e| anyhow::anyhow!("Failed to get symbols: {}", e))?
        };
    }

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
