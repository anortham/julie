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
