//! Response formatting for symbol queries
//!
//! Handles formatting symbol data into structured responses for MCP clients.

use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use tracing::debug;

use crate::extractors::base::Symbol;

/// Format symbol query response with structured content
pub fn format_symbol_response(
    file_path: &str,
    symbols: Vec<Symbol>,
    total_symbols: usize,
    max_depth: u32,
    target: Option<&str>,
    limit: Option<u32>,
    was_truncated: bool,
    workspace_id: Option<String>,
) -> anyhow::Result<CallToolResult> {
    let top_level_count = symbols.iter().filter(|s| s.parent_id.is_none()).count();

    debug!(
        "📋 Returning {} symbols (target: {:?}, truncated: {})",
        symbols.len(),
        target,
        was_truncated
    );

    // Minimal text output for AI agents (structured_content has all data)
    let truncation_warning = if was_truncated {
        format!(
            "\n\n⚠️  Showing {} of {} symbols (truncated)\n💡 Use 'target' parameter to filter to specific symbols",
            symbols.len(),
            total_symbols
        )
    } else {
        String::new()
    };

    let text_summary = if let Some(target) = target {
        format!(
            "{} ({} total symbols, {} matching '{}'){}",
            file_path,
            total_symbols,
            symbols
                .iter()
                .filter(|s| s.name.to_lowercase().contains(&target.to_lowercase()))
                .count(),
            target,
            truncation_warning
        )
    } else {
        let top_names: Vec<String> = symbols
            .iter()
            .filter(|s| s.parent_id.is_none())
            .take(5)
            .map(|s| s.name.clone())
            .collect();

        format!(
            "{} ({} symbols)\nTop-level: {}{}",
            file_path,
            symbols.len(),
            top_names.join(", "),
            truncation_warning
        )
    };

    // Return structured content with symbol data (agents parse this)
    let mut structured_json = serde_json::json!({
        "file_path": file_path,
        "total_symbols": total_symbols,
        "returned_symbols": symbols.len(),
        "top_level_count": top_level_count,
        "symbols": symbols,
        "max_depth": max_depth,
        "truncated": was_truncated,
        "limit": limit,
    });

    // Add workspace_id to response if it's a reference workspace
    if let Some(ref ws_id) = workspace_id {
        if let serde_json::Value::Object(ref mut obj) = structured_json {
            obj.insert("workspace_id".to_string(), serde_json::json!(ws_id));
        }
    }

    let mut result = CallToolResult::text_content(vec![TextContent::from(text_summary)]);

    // Convert JSON Value to Map
    if let serde_json::Value::Object(map) = structured_json {
        result.structured_content = Some(map);
    }

    Ok(result)
}
