//! Result formatting for search responses
//!
//! Provides formatting utilities for search tool responses.

use crate::extractors::Symbol;
use crate::tools::shared::OptimizedResponse;

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

/// Format search results in lean text format - optimal for AI agent consumption
///
/// Output format:
/// ```text
/// 5 matches for "query":
///
/// src/file.rs:42
///   41: // context before
///   42→ fn matched_line() {
///   43:     // context after
///
/// src/other.rs:100
///   99: // context
///   100→ matched_code
/// ```
///
/// Benefits:
/// - Minimal tokens — no wasted structural overhead
/// - Zero parsing overhead — just read the text
/// - Grep-style output familiar to developers
pub fn format_lean_search_results(query: &str, response: &OptimizedResponse<Symbol>) -> String {
    let mut output = String::new();

    // Header with count and query
    let count = response.results.len();
    let total = response.total_found;
    if count == total {
        output.push_str(&format!("{} matches for \"{}\":\n\n", count, query));
    } else {
        output.push_str(&format!(
            "{} matches for \"{}\" (showing {} of {}):\n\n",
            count, query, count, total
        ));
    }

    // Each result: file:line header + indented code context
    for symbol in &response.results {
        // File:line header
        output.push_str(&format!("{}:{}\n", symbol.file_path, symbol.start_line));

        // Indented code context (already has line numbers and arrow)
        if let Some(ctx) = &symbol.code_context {
            for line in ctx.lines() {
                output.push_str(&format!("  {}\n", line));
            }
        }
        output.push('\n');
    }

    // Trim trailing whitespace but keep structure
    output.trim_end().to_string()
}
