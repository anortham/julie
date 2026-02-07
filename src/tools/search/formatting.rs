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

/// Format definition search results with exact-match promotion
///
/// When a result has `name == query` (exact, case-sensitive), it gets promoted
/// to the top with a "Definition found:" header showing kind, visibility, and signature.
/// Remaining results appear as "Other matches:".
///
/// If no exact match exists, falls back to `format_lean_search_results`.
pub fn format_definition_search_results(
    query: &str,
    response: &OptimizedResponse<Symbol>,
) -> String {
    // Partition into exact matches and other matches
    let (exact, others): (Vec<&Symbol>, Vec<&Symbol>) = response
        .results
        .iter()
        .partition(|s| s.name == query);

    // No exact match → standard format
    if exact.is_empty() {
        return format_lean_search_results(query, response);
    }

    let mut output = String::new();

    // === Promoted section ===
    output.push_str(&format!("Definition found: {}\n", query));

    for symbol in &exact {
        // Location + kind + visibility
        let kind = symbol.kind.to_string();
        let vis = symbol
            .visibility
            .as_ref()
            .map(|v| format!(", {}", v.to_string().to_lowercase()))
            .unwrap_or_default();
        output.push_str(&format!(
            "  {}:{} ({}{})\n",
            symbol.file_path, symbol.start_line, kind, vis
        ));

        // Signature (prefer it over code_context for the promoted view)
        if let Some(sig) = &symbol.signature {
            output.push_str(&format!("  {}\n", sig));
        } else if let Some(ctx) = &symbol.code_context {
            // Fallback: first non-empty line of code_context
            if let Some(first_line) = ctx.lines().find(|l| !l.trim().is_empty()) {
                output.push_str(&format!("  {}\n", first_line.trim()));
            }
        }
    }

    // === Other matches section ===
    if !others.is_empty() {
        output.push_str("\nOther matches:\n\n");

        for symbol in &others {
            output.push_str(&format!("{}:{}\n", symbol.file_path, symbol.start_line));
            if let Some(ctx) = &symbol.code_context {
                for line in ctx.lines() {
                    output.push_str(&format!("  {}\n", line));
                }
            }
            output.push('\n');
        }
    }

    output.trim_end().to_string()
}
