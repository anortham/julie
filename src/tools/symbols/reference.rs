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

    // Use handler helpers for DB access. Reference root lookup is only needed
    // when we must normalize relative paths against the reference workspace.
    let db_arc = handler
        .get_database_for_workspace(&ref_workspace_id)
        .await?;

    let (query_path, absolute_path) = if std::path::Path::new(file_path).is_absolute() {
        // Absolute path input
        let canonical = std::path::Path::new(file_path)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(file_path));

        let query_path = match handler
            .get_workspace_root_for_target(&ref_workspace_id)
            .await
        {
            Ok(ref_workspace_root) => {
                crate::utils::paths::to_relative_unix_style(&canonical, &ref_workspace_root)
                    .unwrap_or_else(|_| file_path.to_string())
            }
            Err(_) => file_path.to_string(),
        };

        (query_path, canonical.to_string_lossy().to_string())
    } else {
        let ref_workspace_root = handler
            .get_workspace_root_for_target(&ref_workspace_id)
            .await?;

        debug!(
            "🗄️ Reference workspace DB via handler helper, root: {}",
            ref_workspace_root.display()
        );

        // Relative path input - normalize (handle ./ and ../) against the reference root,
        // then convert back to relative Unix-style for the SQLite query.
        let absolute = ref_workspace_root
            .join(file_path)
            .canonicalize()
            .unwrap_or_else(|_| ref_workspace_root.join(file_path));

        let relative_unix =
            crate::utils::paths::to_relative_unix_style(&absolute, &ref_workspace_root)
                .unwrap_or_else(|_| {
                    warn!("Failed to convert path to relative: {}", file_path);
                    file_path.replace('\\', "/")
                });

        (relative_unix, absolute.to_string_lossy().to_string())
    };

    debug!(
        "🔍 Path normalization: '{}' -> query='{}', absolute='{}' (ref workspace: {})",
        file_path, query_path, absolute_path, ref_workspace_id
    );

    // Check if file exists before querying database
    if !std::path::Path::new(&absolute_path).exists() {
        let mut message = format!("❌ File not found: {}", file_path);
        if let Some(target_name) = target {
            // Symbol-first intent — point to deep_dive
            message.push_str(&format!(
                "\n💡 Try deep_dive(symbol=\"{}\") to find it without needing the file path",
                target_name
            ));
        } else {
            // File-first intent — point to fast_search with the filename
            let filename = std::path::Path::new(file_path)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| file_path.to_string());
            message.push_str(&format!(
                "\n💡 Try fast_search(query=\"{}\", search_target=\"definitions\") to locate the file",
                filename
            ));
        }
        return Ok(CallToolResult::text_content(vec![Content::text(message)]));
    }

    // Query symbols using relative Unix-style path via Arc<Mutex<>> DB
    // In structure mode, use lightweight query that skips expensive columns
    let mode_owned = mode.to_string();
    let query_path_clone = query_path.clone();
    let mut symbols = {
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

    if symbols.is_empty() && query_path != file_path {
        let fallback_query = file_path.replace('\\', "/");
        let db = db_arc
            .lock()
            .map_err(|e| anyhow::anyhow!("Database lock error: {}", e))?;
        symbols = if mode_owned == "structure" {
            db.get_symbols_for_file_lightweight(&fallback_query)
                .map_err(|e| anyhow::anyhow!("Failed to get symbols: {}", e))?
        } else {
            db.get_symbols_for_file(&fallback_query)
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
