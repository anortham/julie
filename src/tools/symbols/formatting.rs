//! Response formatting for symbol queries
//!
//! Handles formatting symbol data into structured responses for MCP clients.

use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use serde::Serialize;
use tracing::debug;

use crate::extractors::base::Symbol;
use crate::tools::shared::create_toonable_result;

/// Simplified symbol for TOON encoding (primitives only, no skip_serializing_if)
///
/// CRITICAL: No #[serde(skip_serializing_if)] attributes!
/// TOON requires ALL objects to have IDENTICAL key sets for tabular encoding.
#[derive(Debug, Clone, Serialize)]
pub struct ToonFlatSymbol {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub language: String,
    pub file_path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub parent_id: Option<String>, // Always serialized (null for top-level)
    pub signature: Option<String>,
    pub doc_comment: Option<String>,
    pub visibility: Option<String>,
}

impl From<&Symbol> for ToonFlatSymbol {
    fn from(s: &Symbol) -> Self {
        Self {
            id: s.id.clone(),
            name: s.name.clone(),
            kind: s.kind.to_string(), // Display trait: lowercase "function", "enum_member"
            language: s.language.clone(),
            file_path: s.file_path.clone(),
            start_line: s.start_line,
            end_line: s.end_line,
            parent_id: s.parent_id.clone(),
            signature: s.signature.clone(),
            doc_comment: s.doc_comment.clone(),
            visibility: s.visibility.as_ref().map(|v| format!("{:?}", v)),
        }
    }
}

/// Result structure for get_symbols that supports hierarchical TOON encoding
#[derive(Debug, Clone, Serialize)]
pub struct GetSymbolsResult {
    pub file_path: String,
    pub total_symbols: usize,
    pub returned_symbols: usize,
    pub top_level_count: usize,
    pub max_depth: u32,
    pub truncated: bool,
    pub limit: Option<u32>,
    pub workspace_id: Option<String>, // Always serialized
    pub symbols: Vec<Symbol>,
}

impl GetSymbolsResult {
    /// Convert to completely flat structure for TOON encoding
    ///
    /// TOON can't handle Symbol's complex structure (skip_serializing_if, metadata HashMap),
    /// so we convert to ToonFlatSymbol with primitives only and uniform keys.
    pub fn to_toon_flat(&self) -> Vec<ToonFlatSymbol> {
        self.symbols.iter().map(ToonFlatSymbol::from).collect()
    }
}

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
    // Smart default: use "code" format when code bodies are available
    // Fall back to structured output (TOON/JSON) when only metadata exists
    let has_code_bodies = symbols.iter().any(|s| s.code_context.is_some());
    let effective_format = match output_format {
        Some(fmt) => fmt,                  // Explicit format wins
        None if has_code_bodies => "code", // Default to code when available
        None => "auto",                    // Fall back to structured output
    };

    // Handle "code" format - returns raw code without metadata
    if effective_format == "code" {
        debug!(
            "📋 Returning {} symbols as raw code (target: {:?})",
            symbols.len(),
            target
        );
        return Ok(format_code_output(file_path, &symbols));
    }

    let top_level_count = symbols.iter().filter(|s| s.parent_id.is_none()).count();

    debug!(
        "📋 Returning {} symbols (target: {:?}, truncated: {})",
        symbols.len(),
        target,
        was_truncated
    );

    // Build GetSymbolsResult
    let result = GetSymbolsResult {
        file_path: file_path.to_string(),
        total_symbols,
        returned_symbols: symbols.len(),
        top_level_count,
        max_depth,
        truncated: was_truncated,
        limit,
        workspace_id,
        symbols,
    };

    // Convert to flat structure for TOON
    let toon_flat = result.to_toon_flat();

    // Use shared helper for consistent TOON/JSON encoding
    create_toonable_result(
        &result,                 // JSON gets full metadata
        &toon_flat,              // TOON gets flat optimized array
        Some(effective_format),  // Pass resolved format (already handled "code" above)
        5,                       // Auto threshold: use TOON for 5+ symbols
        toon_flat.len(),
        "get_symbols",
    )
}
