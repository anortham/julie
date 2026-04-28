//! Result formatting for search responses
//!
//! Provides formatting utilities for search tool responses.

use crate::extractors::Symbol;
use crate::search::index::FileMatchKind;
use crate::search::scoring::is_test_path;
use crate::tools::search::trace::SearchHit;
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

    // Group results by file path (preserving order of first appearance)
    // Build ordered list of groups: (file_path, Vec<&Symbol>)
    let mut groups: Vec<(&str, Vec<&Symbol>)> = Vec::new();
    for symbol in &response.results {
        let path = symbol.file_path.as_str();
        if let Some(group) = groups.iter_mut().find(|(p, _)| *p == path) {
            group.1.push(symbol);
        } else {
            groups.push((path, vec![symbol]));
        }
    }

    // Emit each group
    for (file_path, symbols) in &groups {
        if symbols.len() == 1 {
            // Single match in file: keep original file:line format
            let sym = symbols[0];
            output.push_str(&format!("{}:{}\n", file_path, sym.start_line));
            if let Some(ctx) = &sym.code_context {
                for line in ctx.lines() {
                    output.push_str(&format!("  {}\n", line));
                }
            }
        } else {
            // Multiple matches in file: group under file header
            output.push_str(&format!("{}:\n", file_path));
            for sym in symbols.iter() {
                output.push_str(&format!("  :{}\n", sym.start_line));
                if let Some(ctx) = &sym.code_context {
                    for line in ctx.lines() {
                        output.push_str(&format!("    {}\n", line));
                    }
                }
                output.push('\n');
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
    // Partition into name matches (exact or qualified component) and other matches.
    // "Router" matches both "Router" (exact) and "Phoenix.Router" (last component).
    let query_lower = query.to_lowercase();
    let (exact, others): (Vec<&Symbol>, Vec<&Symbol>) = response
        .results
        .iter()
        .partition(|s| is_definition_name_match(&s.name, &query_lower));

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

        // Group by file (preserving order of first appearance)
        let mut groups: Vec<(&str, Vec<&&Symbol>)> = Vec::new();
        for symbol in &others {
            let path = symbol.file_path.as_str();
            if let Some(group) = groups.iter_mut().find(|(p, _)| *p == path) {
                group.1.push(symbol);
            } else {
                groups.push((path, vec![symbol]));
            }
        }

        for (file_path, symbols) in &groups {
            if symbols.len() == 1 {
                let sym = symbols[0];
                output.push_str(&format!("{}:{}\n", file_path, sym.start_line));
                if let Some(ctx) = &sym.code_context {
                    for line in ctx.lines() {
                        output.push_str(&format!("  {}\n", line));
                    }
                }
            } else {
                output.push_str(&format!("{}:\n", file_path));
                for sym in symbols.iter() {
                    output.push_str(&format!("  :{}\n", sym.start_line));
                    if let Some(ctx) = &sym.code_context {
                        for line in ctx.lines() {
                            output.push_str(&format!("    {}\n", line));
                        }
                    }
                    output.push('\n');
                }
            }
            output.push('\n');
        }
    }

    output.trim_end().to_string()
}

/// Format search results as file:line locations only (no code context).
/// Saves 70-90% tokens compared to full format.
pub fn format_locations_only(query: &str, response: &OptimizedResponse<Symbol>) -> String {
    let mut output = String::new();
    let count = response.results.len();
    let total = response.total_found;

    if count == total {
        output.push_str(&format!("{} locations for \"{}\":\n", count, query));
    } else {
        output.push_str(&format!(
            "{} locations for \"{}\" (showing {} of {}):\n",
            count, query, count, total
        ));
    }

    for symbol in &response.results {
        let kind = symbol.kind.to_string();
        output.push_str(&format!(
            "  {}:{} ({})\n",
            symbol.file_path, symbol.start_line, kind
        ));
    }

    output.trim_end().to_string()
}

pub fn format_file_locations_only(query: &str, response: &OptimizedResponse<SearchHit>) -> String {
    let mut output = String::new();
    write_file_header(&mut output, "file matches", query, response);

    for hit in &response.results {
        output.push_str(&hit.file);
        output.push('\n');
    }

    output.trim_end().to_string()
}

pub fn format_content_locations_only(
    query: &str,
    response: &OptimizedResponse<SearchHit>,
) -> String {
    let mut output = String::new();
    let count = response.results.len();
    let total = response.total_found;

    if count == total {
        output.push_str(&format!("{count} locations for \"{query}\":\n"));
    } else {
        output.push_str(&format!(
            "{count} locations for \"{query}\" (showing {count} of {total}):\n"
        ));
    }

    for hit in &response.results {
        match hit.line {
            Some(line) => output.push_str(&format!("  {}:{}\n", hit.file, line)),
            None => output.push_str(&format!("  {}\n", hit.file)),
        }
    }

    output.trim_end().to_string()
}

pub fn format_file_search_results(query: &str, response: &OptimizedResponse<SearchHit>) -> String {
    let mut output = String::new();
    write_file_header(&mut output, "file matches", query, response);

    for hit in &response.results {
        output.push_str(&hit.file);

        let mut annotations = Vec::new();
        if !hit.language.is_empty() {
            annotations.push(hit.language.clone());
        }
        if let Some(file_result) = hit.as_file_result() {
            annotations.push(file_match_kind_label(file_result.match_kind).to_string());
        }
        if is_test_path(&hit.file) {
            annotations.push("test".to_string());
        }

        if !annotations.is_empty() {
            output.push_str(" (");
            output.push_str(&annotations.join(", "));
            output.push(')');
        }
        output.push('\n');
    }

    output.trim_end().to_string()
}

/// Check if a symbol name matches a query for definition formatting.
/// Matches exact name OR last component of a dot-qualified name.
fn is_definition_name_match(symbol_name: &str, query_lower: &str) -> bool {
    let name_lower = symbol_name.to_lowercase();
    if name_lower == query_lower {
        return true;
    }
    if let Some(last_component) = name_lower.rsplit('.').next() {
        if last_component == query_lower {
            return true;
        }
    }
    if query_lower.contains('.') && name_lower.ends_with(query_lower) {
        let prefix_len = name_lower.len() - query_lower.len();
        if prefix_len == 0 || name_lower.as_bytes()[prefix_len - 1] == b'.' {
            return true;
        }
    }
    false
}

fn write_file_header(
    output: &mut String,
    noun: &str,
    query: &str,
    response: &OptimizedResponse<SearchHit>,
) {
    let count = response.results.len();
    let total = response.total_found;
    if count == total {
        output.push_str(&format!("{count} {noun} for \"{query}\":\n\n"));
    } else {
        output.push_str(&format!(
            "{count} {noun} for \"{query}\" (showing {count} of {total}):\n\n"
        ));
    }
}

fn file_match_kind_label(match_kind: FileMatchKind) -> &'static str {
    match match_kind {
        FileMatchKind::ExactPath => "exact path",
        FileMatchKind::ExactBasename => "exact basename",
        FileMatchKind::PathFragment => "path fragment",
        FileMatchKind::Glob => "glob match",
    }
}
