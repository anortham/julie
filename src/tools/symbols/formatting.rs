//! Response formatting for symbol queries
//!
//! Handles formatting symbol data into structured responses for MCP clients.

use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use tracing::debug;

use crate::extractors::base::Symbol;

/// Format raw code output - just the source code, no metadata wrapper
///
/// This format is optimal for AI agents that can read code directly.
/// Returns code bodies separated by blank lines with a minimal file header.
fn format_code_output(file_path: &str, symbols: &[Symbol]) -> CallToolResult {

    let mut output = String::new();

    // Minimal file header
    output.push_str(&format!("// === {} ===\n\n", file_path));

    // Extract code from each symbol
    for (i, symbol) in symbols.iter().enumerate() {
        if let Some(code) = &symbol.code_context {
            output.push_str(code);
            // Add separator between symbols (but not after the last one)
            if i < symbols.len() - 1 {
                output.push_str("\n\n");
            }
        }
    }

    // Trim trailing whitespace but ensure single newline at end
    let output = output.trim_end().to_string() + "\n";

    CallToolResult::text_content(vec![Content::text(output)])
}

/// Format lean text overview — scannable symbol list for agents
///
/// Output format:
/// ```text
/// src/foo.rs — 12 symbols
///
///   struct Foo (10-25, public)
///     fn new() -> Self (12-15, public)
///     fn process(&self, data: &[u8]) (17-24, public)
///   fn helper(x: i32) -> bool (30-45, private)
/// ```
fn format_lean_symbols(file_path: &str, symbols: &[Symbol]) -> CallToolResult {
    let mut output = String::new();

    output.push_str(&format!("{} — {} symbols\n", file_path, symbols.len()));

    for symbol in symbols {
        let indent = if symbol.parent_id.is_some() { "    " } else { "  " };
        let kind = symbol.kind.to_string();

        // Use signature if available, otherwise just name
        let name_display = if let Some(sig) = &symbol.signature {
            // Signature often includes the kind keyword, use as-is
            sig.clone()
        } else {
            symbol.name.clone()
        };

        let vis = symbol
            .visibility
            .as_ref()
            .map(|v| format!("{:?}", v).to_lowercase())
            .unwrap_or_default();

        let vis_str = if vis.is_empty() || vis == "public" {
            String::new()
        } else {
            format!(", {}", vis)
        };

        output.push_str(&format!(
            "{}{} {} ({}-{}{})\n",
            indent,
            kind,
            name_display,
            symbol.start_line,
            symbol.end_line,
            vis_str,
        ));
    }

    CallToolResult::text_content(vec![Content::text(output.trim_end().to_string())])
}

/// Format symbol query response with structured content
pub fn format_symbol_response(
    file_path: &str,
    symbols: Vec<Symbol>,
    target: Option<&str>,
) -> anyhow::Result<CallToolResult> {
    // Auto-select format: "code" when code bodies are available, "lean" otherwise
    let has_code_bodies = symbols.iter().any(|s| s.code_context.is_some());
    let effective_format = if has_code_bodies { "code" } else { "lean" };

    // Handle "code" format - returns raw code without metadata
    if effective_format == "code" {
        debug!(
            "📋 Returning {} symbols as raw code (target: {:?})",
            symbols.len(),
            target
        );
        return Ok(format_code_output(file_path, &symbols));
    }

    // Everything else (including "lean", unknown formats) → lean text overview
    debug!(
        "📋 Returning {} symbols as lean overview (target: {:?})",
        symbols.len(),
        target
    );
    Ok(format_lean_symbols(file_path, &symbols))
}
