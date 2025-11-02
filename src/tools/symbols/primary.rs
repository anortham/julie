//! Primary workspace symbol retrieval
//!
//! Handles getting symbols from the primary (active) workspace.

use anyhow::Result;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use tracing::{debug, info, warn};

use crate::handler::JulieServerHandler;
use super::filtering::apply_all_filters;
use super::body_extraction::extract_code_bodies;
use super::formatting::format_symbol_response;

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

    let workspace = handler.get_workspace().await?.ok_or_else(|| {
        anyhow::anyhow!("No workspace initialized. Run 'manage_workspace index' first")
    })?;

    let db = workspace
        .db
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No database available"))?;

    // Phase 2: Database stores relative Unix-style paths for token efficiency
    // We need TWO paths:
    // 1. query_path: Relative Unix-style for database queries
    // 2. absolute_path: Absolute path for file I/O (extract_code_bodies)

    let (query_path, absolute_path) = if std::path::Path::new(file_path).is_absolute() {
        // Absolute path input
        let canonical = std::path::Path::new(file_path)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(file_path));

        let relative = crate::utils::paths::to_relative_unix_style(&canonical, &workspace.root)
            .unwrap_or_else(|_| {
                warn!("Failed to convert absolute path to relative: {}", file_path);
                file_path.to_string()
            });

        (relative, canonical.to_string_lossy().to_string())
    } else {
        // Relative path input - need to normalize (handle ./ and ../)
        // Join with workspace root, canonicalize, then convert back to relative
        let absolute = workspace
            .root
            .join(file_path)
            .canonicalize()
            .unwrap_or_else(|_| workspace.root.join(file_path));

        // Convert canonicalized path back to relative Unix-style for database query
        let relative_unix = crate::utils::paths::to_relative_unix_style(&absolute, &workspace.root)
            .unwrap_or_else(|_| {
                warn!("Failed to convert path to relative: {}", file_path);
                file_path.replace('\\', "/")
            });

        (relative_unix, absolute.to_string_lossy().to_string())
    };

    debug!(
        "🔍 Path normalization: '{}' -> query='{}', absolute='{}'",
        file_path, query_path, absolute_path
    );
    debug!("🔍 Workspace root: '{}'", workspace.root.display());

    // Check if file exists before querying database
    if !std::path::Path::new(&absolute_path).exists() {
        let message = format!(
            "❌ File not found: {}\n💡 Check the file path - use relative paths from workspace root",
            file_path
        );
        return Ok(CallToolResult::text_content(vec![TextContent::from(
            message,
        )]));
    }

    // Query symbols for this file using relative Unix-style path
    let symbols = {
        let db_lock = match db.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("Database mutex poisoned in get_symbols_from_primary, recovering: {}", poisoned);
                poisoned.into_inner()
            }
        };
        db_lock
            .get_symbols_for_file(&query_path)
            .map_err(|e| anyhow::anyhow!("Failed to get symbols: {}", e))?
    };

    if symbols.is_empty() {
        let message = format!("No symbols found in: {}", file_path);
        return Ok(CallToolResult::text_content(vec![TextContent::from(
            message,
        )]));
    }

    // Apply all filters and get the final symbol list
    let (symbols_to_return, was_truncated, total_symbols) =
        apply_all_filters(symbols, max_depth, target, limit);

    if symbols_to_return.is_empty() {
        let message = format!("No symbols found after filtering in: {}", file_path);
        return Ok(CallToolResult::text_content(vec![TextContent::from(
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
        None,
    )
}
