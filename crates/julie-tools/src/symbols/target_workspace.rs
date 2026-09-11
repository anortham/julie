//! Target workspace symbol retrieval.
//!
//! Handles getting symbols from explicit non-primary workspaces.

use anyhow::{Result, bail};
use julie_core::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use tracing::{debug, info};

use super::body_extraction::extract_code_bodies;
use super::filtering::apply_all_filters;
use super::formatting::format_symbol_response;
use super::rows::symbols_in_path;
use julie_context::{ToolContext, WorkspaceTarget};

/// Get symbols from a target workspace.
pub async fn get_symbols_from_target_workspace(
    handler: &dyn ToolContext,
    file_path: &str,
    max_depth: u32,
    target: Option<&str>,
    limit: Option<u32>,
    offset: u32,
    mode: &str,
    target_workspace_id: String,
) -> Result<(CallToolResult, u32)> {
    info!(
        "📋 Getting symbols from workspace: {} in file: {} (depth: {})",
        target_workspace_id, file_path, max_depth
    );

    let snapshot = handler
        .snapshot(&WorkspaceTarget::Target(target_workspace_id.clone()))
        .await?;

    // Strict contract: `resolve_workspace_file_input` rejects outside-workspace
    // paths with a typed `WorkspaceResolutionFailure`. We propagate via `?` so
    // the MCP boundary can surface `invalid_params` instead of silently feeding
    // a raw path string to the snapshot.
    let input_is_absolute = std::path::Path::new(file_path).is_absolute();
    let (query_path, absolute_path) = match handler
        .get_workspace_root_for_target(&target_workspace_id)
        .await
    {
        Ok(target_workspace_root) => {
            debug!(
                "🗄️ Target workspace root via handler helper: {}",
                target_workspace_root.display()
            );

            let resolution =
                julie_core::paths::resolve_workspace_file_input(file_path, &target_workspace_root)?;

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

    if !std::path::Path::new(&absolute_path).exists() {
        bail!(super::file_not_found_message(file_path, target));
    }

    let mut symbols = symbols_in_path(&snapshot, &query_path);
    if symbols.is_empty() && query_path != file_path {
        symbols = symbols_in_path(&snapshot, &file_path.replace('\\', "/"));
    }

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
