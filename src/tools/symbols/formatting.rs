//! Response formatting for symbol queries
//!
//! Handles formatting symbol data into structured responses for MCP clients.

use rust_mcp_sdk::schema::CallToolResult;
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
    output_format: Option<&str>,
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

    let _text_summary = if let Some(target) = target {
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

    // Build structured response
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

    // Return based on output_format: TOON uses text only, JSON uses structured only
    match output_format {
        Some("toon") => {
            // TOON mode: Return ONLY TOON in text, NO structured content
            match toon_format::encode_default(&structured_json) {
                Ok(toon_text) => {
                    debug!("✅ Returning get_symbols results in TOON-only mode ({} chars, no structured_content)", toon_text.len());
                    Ok(CallToolResult::text_content(vec![toon_text.into()]))
                }
                Err(e) => {
                    debug!("⚠️  TOON encoding failed: {}, falling back to JSON", e);
                    let mut result = CallToolResult::text_content(vec![]);
                    if let serde_json::Value::Object(map) = structured_json {
                        result.structured_content = Some(map);
                    }
                    Ok(result)
                }
            }
        }
        Some("auto") => {
            // Auto mode: TOON for 5+ symbols, JSON for smaller responses
            if symbols.len() >= 5 {
                match toon_format::encode_default(&structured_json) {
                    Ok(toon_text) => {
                        debug!("✅ Auto-selected TOON for {} symbols ({} chars, no structured_content)", symbols.len(), toon_text.len());
                        return Ok(CallToolResult::text_content(vec![toon_text.into()]));
                    }
                    Err(e) => {
                        debug!("⚠️  TOON encoding failed: {}, falling back to JSON", e);
                        // Fall through to JSON mode
                    }
                }
            }
            {
                // Small response: use JSON-only (no redundant text)
                let mut result = CallToolResult::text_content(vec![]);
                if let serde_json::Value::Object(map) = structured_json {
                    result.structured_content = Some(map);
                }
                debug!("✅ Auto-selected JSON for {} symbols (no redundant text_content)", symbols.len());
                Ok(result)
            }
        }
        _ => {
            // Default (JSON/None): ONLY structured content (no redundant text)
            let mut result = CallToolResult::text_content(vec![]);
            if let serde_json::Value::Object(map) = structured_json {
                result.structured_content = Some(map);
            }
            debug!("✅ Returning get_symbols results as JSON-only (no redundant text_content)");
            Ok(result)
        }
    }
}
