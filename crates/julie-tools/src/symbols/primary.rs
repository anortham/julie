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
    next_request: &super::GetSymbolsTool,
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

    if let Some(source_hash) = next_request.source_hash.as_deref() {
        super::body_extraction::validate_source_hash(&snapshot, &query_path, source_hash)?;
    }
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
    let mut extraction = extract_code_bodies(
        &snapshot,
        symbols_to_return,
        body_mode,
        next_request.body_offset,
        next_request.body_limit,
        next_request.source_hash.as_deref(),
    )?;
    let count = extraction.symbols.len() as u32;
    let next = more.then(|| crate::shared::next_line("get_symbols", next_request, offset + kept));
    let body_next = if extraction.pages.iter().any(|page| !page.end_reached) {
        let mut request = next_request.clone();
        request.body_limit = Some(
            next_request
                .body_limit
                .unwrap_or(super::body_extraction::DEFAULT_BODY_LIMIT)
                .max(1),
        );
        request.source_hash = extraction.source_hash.clone();
        Some(crate::shared::request_line(
            "get_symbols",
            &request,
            "body_offset",
            request.body_offset as usize + request.body_limit.unwrap_or(1) as usize,
        ))
    } else {
        None
    };
    for page in &mut extraction.pages {
        if !page.end_reached {
            page.continuation = body_next.clone();
        }
    }
    let trailers = [next, body_next].into_iter().flatten().collect::<Vec<_>>();
    let next = (!trailers.is_empty()).then(|| trailers.join("\n"));
    let result = format_symbol_response(
        file_path,
        extraction.symbols,
        target,
        next,
        extraction.pages,
        next_request.body_limit.is_some()
            || next_request.body_offset > 0
            || next_request.source_hash.is_some(),
    )?;
    Ok((result, count))
}
