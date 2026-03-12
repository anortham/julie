//! Reference workspace symbol retrieval
//!
//! Handles getting symbols from reference (non-primary) workspaces.

use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use tracing::{debug, info, warn};

use super::body_extraction::extract_code_bodies;
use super::filtering::apply_all_filters;
use super::formatting::format_symbol_response;
use crate::handler::JulieServerHandler;

/// Get symbols from a reference workspace
pub async fn get_symbols_from_reference(
    handler: &JulieServerHandler,
    file_path: &str,
    max_depth: u32,
    target: Option<&str>,
    limit: Option<u32>,
    mode: &str,
    ref_workspace_id: String,
) -> Result<CallToolResult> {
    info!(
        "📋 Getting symbols from reference workspace: {} in file: {} (depth: {})",
        ref_workspace_id, file_path, max_depth
    );

    // Use handler helpers for DB and workspace root access
    let db_arc = handler
        .get_database_for_workspace(&ref_workspace_id)
        .await?;
    let ref_workspace_root = handler
        .get_workspace_root_for_target(&ref_workspace_id)
        .await?;

    debug!(
        "🗄️ Reference workspace DB via handler helper, root: {}",
        ref_workspace_root.display()
    );

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
        return Ok(CallToolResult::text_content(vec![Content::text(message)]));
    }

    // Query symbols using relative Unix-style path via Arc<Mutex<>> DB
    // In structure mode, use lightweight query that skips expensive columns
    let mode_owned = mode.to_string();
    let query_path_clone = query_path.clone();
    let symbols = {
        let db = db_arc
            .lock()
            .map_err(|e| anyhow::anyhow!("Database lock error: {}", e))?;
        if mode_owned == "structure" {
            db.get_symbols_for_file_lightweight(&query_path_clone)
                .map_err(|e| anyhow::anyhow!("Failed to get symbols: {}", e))?
        } else {
            db.get_symbols_for_file(&query_path_clone)
                .map_err(|e| anyhow::anyhow!("Failed to get symbols: {}", e))?
        }
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
