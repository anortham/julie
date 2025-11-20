//! Result formatting for search responses
//!
//! Provides formatting utilities for converting search results into human and AI-friendly formats.

use crate::extractors::Symbol;
use crate::tools::shared::OptimizedResponse;

/// Format minimal summary for AI agents (structured_content has all data)
///
/// AI agents parse structured_content (JSON), not text output.
/// Keep text minimal to save massive context tokens.
pub fn format_optimized_results(query: &str, optimized: &OptimizedResponse<Symbol>) -> String {
    // Line 1: Summary with count and confidence
    let summary = format!(
        "Found {} results for '{}' (confidence: {:.1})",
        optimized.total_found, query, optimized.confidence
    );

    // Line 2: Top 5 result names (quick scan)
    let top_names: Vec<String> = optimized
        .results
        .iter()
        .take(5)
        .map(|s| s.name.clone())
        .collect();

    if top_names.is_empty() {
        summary
    } else {
        format!("{}\nTop results: {}", summary, top_names.join(", "))
    }
}

/// Ultra-compact output for MCP responses (no JSON, token-lean)
///
/// Format:
/// `<file_path>:<start_line>-<end_line>`
/// `<code snippet>`
pub fn format_dense_results(optimized: &OptimizedResponse<Symbol>) -> String {
    if optimized.results.is_empty() {
        return "No results".to_string();
    }

    let mut lines = Vec::new();

    for symbol in &optimized.results {
        // Always include full path and the span so the client can locate the code.
        // Include name/kind inline to retain useful metadata without a JSON blob.
        lines.push(format!(
            "{}:{}-{} | {} | {}",
            symbol.file_path, symbol.start_line, symbol.end_line, symbol.name, symbol.kind
        ));

        if let Some(ctx) = &symbol.code_context {
            let snippet = ctx.trim();

            // Avoid empty context lines; keep as few tokens as possible.
            if !snippet.is_empty() {
                lines.push(snippet.to_string());
            }
        }

        // Blank line between entries for easy parsing
        lines.push(String::new());
    }

    // Avoid trailing blank line
    lines.join("\n").trim_end().to_string()
}

/// Truncate code_context field to save massive tokens in search results
///
/// Formula: max_lines = context_lines * 2 + 1
/// - context_lines=0 → 1 line total (just match)
/// - context_lines=1 → 3 lines total (1 before + match + 1 after) [DEFAULT]
/// - context_lines=3 → 7 lines total (grep default)
pub fn truncate_code_context(symbols: Vec<Symbol>, context_lines: Option<u32>) -> Vec<Symbol> {
    let context_lines = context_lines.unwrap_or(1) as usize;
    let max_lines = context_lines * 2 + 1; // before + match + after

    symbols
        .into_iter()
        .map(|mut symbol| {
            if let Some(code_context) = symbol.code_context.take() {
                let lines: Vec<&str> = code_context.lines().collect();

                if lines.len() > max_lines {
                    // Truncate to max_lines and add indicator
                    let truncated: Vec<&str> = lines.into_iter().take(max_lines).collect();
                    symbol.code_context = Some(format!("{}...", truncated.join("\n")));
                } else {
                    // Keep as-is (within limit)
                    symbol.code_context = Some(code_context);
                }
            }
            symbol
        })
        .collect()
}
