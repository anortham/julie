//! Primary workspace symbol retrieval
//!
//! Handles getting symbols from the primary (active) workspace.

use anyhow::{Result, bail};
use julie_core::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use tracing::{debug, info};

use super::body_extraction::extract_code_bodies;
use super::filtering::apply_all_filters;
use super::formatting::format_symbol_response;
use super::rows::symbols_in_path;
use julie_context::{ToolContext, WorkspaceTarget};

/// Get symbols from the primary workspace
pub async fn get_symbols_from_primary(
    handler: &dyn ToolContext,
    file_path: &str,
    max_depth: u32,
    target: Option<&str>,
    limit: Option<u32>,
    offset: u32,
    mode: &str,
) -> Result<(CallToolResult, u32)> {
    info!(
        "📋 Getting symbols for file: {} (depth: {})",
        file_path, max_depth
    );

    let current_workspace_root = handler.require_primary_workspace_root()?;
    let snapshot = handler.snapshot(&WorkspaceTarget::Primary).await?;

    // Strict contract: `resolve_workspace_file_input` rejects outside-workspace
    // paths with a typed `WorkspaceResolutionFailure`. We propagate via `?` so
    // the MCP boundary can surface `invalid_params` instead of silently feeding
    // a raw path string to the snapshot.
    let resolution =
        julie_core::paths::resolve_workspace_file_input(file_path, &current_workspace_root)?;
    let query_path = resolution.relative_query_path;
    let absolute_path = resolution.absolute_path.to_string_lossy().to_string();

    debug!(
        "🔍 Path normalization: '{}' -> query='{}', absolute='{}'",
        file_path, query_path, absolute_path
    );
    debug!("🔍 Workspace root: '{}'", current_workspace_root.display());

    if !std::path::Path::new(&absolute_path).exists() {
        bail!(super::file_not_found_message(file_path, target));
    }

    let symbols = symbols_in_path(&snapshot, &query_path);

    if symbols.is_empty() {
        let message = format!("No symbols found in: {}", file_path);
        return Ok((
            CallToolResult::text_content(vec![Content::text(message)]),
            0,
        ));
    }

    let offset = offset as usize;
    let page_limit = limit.map(|n| n.max(1) as usize);
    let fetch_limit = page_limit.map(|n| (n + offset + 1) as u32);
    let (symbols_to_return, was_truncated, _total_symbols) =
        apply_all_filters(symbols, max_depth, target, fetch_limit);
    let more = match page_limit {
        Some(n) => symbols_to_return.len() > offset + n || was_truncated,
        None => false,
    };
    let take = page_limit.unwrap_or(usize::MAX);
    let symbols_to_return: Vec<_> = symbols_to_return
        .into_iter()
        .skip(offset)
        .take(take)
        .collect();

    if symbols_to_return.is_empty() {
        let message = format!("No symbols found after filtering in: {}", file_path);
        return Ok((
            CallToolResult::text_content(vec![Content::text(message)]),
            0,
        ));
    }

    // When target is set, upgrade "minimal" to "full" — the user explicitly asked for this
    // symbol, so always include its body even if it's a child (has parent_id).
    let body_mode = if target.is_some() && mode == "minimal" {
        "full"
    } else {
        mode
    };
    let kept = symbols_to_return.len();
    let symbols_to_return = extract_code_bodies(symbols_to_return, &absolute_path, body_mode)?;
    let count = symbols_to_return.len() as u32;
    let next = more.then(|| {
        crate::shared::next_line("get_symbols", &[("file_path", file_path)], offset + kept)
    });
    let result = format_symbol_response(file_path, symbols_to_return, target, next)?;
    Ok((result, count))
}
