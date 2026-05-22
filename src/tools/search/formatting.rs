//! Result formatting for search responses
//!
//! Provides formatting utilities for search tool responses.

use crate::extractors::{Symbol, SymbolKind};
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
    let Some(DefinitionDisplayParts { exact, others }) =
        definition_display_parts(query, &response.results)
    else {
        return format_lean_search_results(query, response);
    };

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
                write_definition_other_match_snippet(&mut output, sym, "  ");
            } else {
                output.push_str(&format!("{}:\n", file_path));
                for sym in symbols.iter() {
                    output.push_str(&format!("  :{}\n", sym.start_line));
                    write_definition_other_match_snippet(&mut output, sym, "    ");
                    output.push('\n');
                }
            }
            output.push('\n');
        }
    }

    output.trim_end().to_string()
}

fn write_definition_other_match_snippet(output: &mut String, symbol: &Symbol, indent: &str) {
    if let Some(signature) = symbol
        .signature
        .as_deref()
        .filter(|signature| !signature.trim().is_empty())
    {
        output.push_str(&format!("{}{}\n", indent, signature));
        return;
    }

    if let Some(ctx) = &symbol.code_context {
        for line in ctx.lines() {
            output.push_str(&format!("{}{}\n", indent, line));
        }
    }
}

/// Format search results as file:line locations only (no code context).
/// Saves 70-90% tokens compared to full format.
pub fn format_locations_only(query: &str, response: &OptimizedResponse<Symbol>) -> String {
    let mut output = String::new();
    let display_results: Vec<&Symbol> = if let Some(DefinitionDisplayParts { exact, others }) =
        definition_display_parts(query, &response.results)
    {
        exact.into_iter().chain(others).collect()
    } else {
        response.results.iter().collect()
    };
    let count = display_results.len();
    let total = response.total_found;

    if count == total {
        output.push_str(&format!("{} locations for \"{}\":\n", count, query));
    } else {
        output.push_str(&format!(
            "{} locations for \"{}\" (showing {} of {}):\n",
            count, query, count, total
        ));
    }

    for symbol in display_results {
        let kind = symbol.kind.to_string();
        output.push_str(&format!(
            "  {}:{} ({})\n",
            symbol.file_path, symbol.start_line, kind
        ));
    }

    output.trim_end().to_string()
}

struct DefinitionDisplayParts<'a> {
    exact: Vec<&'a Symbol>,
    others: Vec<&'a Symbol>,
}

fn definition_display_parts<'a>(
    query: &str,
    symbols: &'a [Symbol],
) -> Option<DefinitionDisplayParts<'a>> {
    // Partition into name matches (exact or qualified component) and other matches.
    // "Router" matches both "Router" (exact) and "Phoenix.Router" (last component).
    let query_lower = query.to_lowercase();
    let (mut exact, others): (Vec<&Symbol>, Vec<&Symbol>) = symbols
        .iter()
        .partition(|s| is_definition_name_match(&s.name, &query_lower));

    if exact.is_empty() {
        return None;
    }

    let mut other_candidates = others;
    if exact
        .iter()
        .any(|symbol| is_promotable_definition_match(symbol))
    {
        let (promoted, demoted): (Vec<&Symbol>, Vec<&Symbol>) = exact
            .into_iter()
            .partition(|symbol| is_promotable_definition_match(symbol));
        exact = promoted;
        other_candidates.extend(demoted);
    }

    let query_key = normalized_definition_match_key(&query_lower);
    let others = other_candidates
        .into_iter()
        .filter(|symbol| is_related_definition_other_match(symbol, &query_lower, &query_key))
        .collect();

    Some(DefinitionDisplayParts { exact, others })
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

    for group in grouped_content_locations(&response.results) {
        if !group.lines.is_empty() {
            let line_numbers = group
                .lines
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            if group.lines.len() == 1 {
                output.push_str(&format!("  {}:{}\n", group.file, line_numbers));
            } else {
                output.push_str(&format!("  {}: {}\n", group.file, line_numbers));
            }
        }
        for _ in 0..group.file_only_hits {
            output.push_str(&format!("  {}\n", group.file));
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
pub(crate) fn is_definition_name_match(symbol_name: &str, query_lower: &str) -> bool {
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

fn is_related_definition_other_match(symbol: &Symbol, query_lower: &str, query_key: &str) -> bool {
    definition_text_matches_query(&symbol.name, query_lower, query_key)
        || symbol.signature.as_deref().is_some_and(|signature| {
            definition_text_matches_query(signature, query_lower, query_key)
        })
}

fn is_promotable_definition_match(symbol: &Symbol) -> bool {
    !matches!(symbol.kind, SymbolKind::Import | SymbolKind::Export)
}

fn definition_text_matches_query(text: &str, query_lower: &str, query_key: &str) -> bool {
    let text_lower = text.to_lowercase();
    text_lower.contains(query_lower)
        || (!query_key.is_empty()
            && normalized_definition_match_key(&text_lower).contains(query_key))
}

fn normalized_definition_match_key(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
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

struct ContentLocationGroup<'a> {
    file: &'a str,
    lines: Vec<u32>,
    file_only_hits: usize,
}

fn grouped_content_locations(hits: &[SearchHit]) -> Vec<ContentLocationGroup<'_>> {
    let mut groups: Vec<ContentLocationGroup<'_>> = Vec::new();

    for hit in hits {
        let Some(group) = groups.iter_mut().find(|group| group.file == hit.file) else {
            let mut group = ContentLocationGroup {
                file: &hit.file,
                lines: Vec::new(),
                file_only_hits: 0,
            };
            add_hit_to_content_location_group(&mut group, hit);
            groups.push(group);
            continue;
        };
        add_hit_to_content_location_group(group, hit);
    }

    groups
}

fn add_hit_to_content_location_group(group: &mut ContentLocationGroup<'_>, hit: &SearchHit) {
    match hit.line {
        Some(line) => group.lines.push(line),
        None => group.file_only_hits += 1,
    }
}

/// Format mixed symbol+file search hits in rank order.
///
/// Replaces the symbol-only `format_definition_search_results` for the unified
/// search path so that file-kind hits (kind == "file") are rendered alongside
/// symbol hits.  Without this the formatter silently dropped file rows via
/// `as_symbol()` filtering, causing path-shaped queries to never surface their
/// target file in the output.
///
/// Behaviour:
///   * If the top-scoring hit (or any earlier hit) is an "exact match"
///     (symbol name matches query, OR file basename/stem matches query in
///     compact form), promote it under a `Definition found:` header.
///   * Otherwise fall through to the lean-style `N matches for "q":` grouping
///     where every hit (symbol or file) is rendered in rank order.
pub fn format_unified_search_results(
    query: &str,
    hits: &[SearchHit],
    total_found: usize,
) -> String {
    use crate::search::index::compact_alnum_lc;

    let query_lower = query.to_lowercase();
    let query_compact = compact_alnum_lc(query);

    // Find the first hit that's an exact match (symbol name, file basename, or
    // file stem).  We use `position` so rank order is preserved — the promoted
    // hit can come from any position in the rank-sorted list, not just index 0.
    let exact_idx = hits.iter().position(|hit| {
        if let Some(symbol) = hit.as_symbol() {
            is_definition_name_match(&symbol.name, &query_lower)
        } else if hit.as_file_result().is_some() {
            file_hit_matches_query(hit, &query_compact)
        } else {
            false
        }
    });

    let Some(exact_idx) = exact_idx else {
        return format_unified_lean(query, hits, total_found);
    };

    let mut output = String::new();
    output.push_str(&format!("Definition found: {}\n", query));
    write_unified_hit_promoted(&mut output, &hits[exact_idx]);

    let others: Vec<&SearchHit> = hits
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx != exact_idx)
        .map(|(_, hit)| hit)
        .collect();

    if !others.is_empty() {
        output.push_str("\nOther matches:\n\n");
        write_unified_hits_grouped(&mut output, &others);
    }

    output.trim_end().to_string()
}

/// Format-only `format_locations_only` analogue for the unified path.  Renders
/// every hit (symbol + file) as a single path line — file:line for symbols,
/// just path for files — preserving rank order.
pub fn format_unified_locations(
    query: &str,
    hits: &[SearchHit],
    total_found: usize,
) -> String {
    let mut output = String::new();
    let count = hits.len();
    if count == total_found {
        output.push_str(&format!("{} locations for \"{}\":\n", count, query));
    } else {
        output.push_str(&format!(
            "{} locations for \"{}\" (showing {} of {}):\n",
            count, query, count, total_found
        ));
    }
    for hit in hits {
        if let Some(symbol) = hit.as_symbol() {
            let kind = symbol.kind.to_string();
            output.push_str(&format!(
                "  {}:{} ({})\n",
                symbol.file_path, symbol.start_line, kind
            ));
        } else {
            output.push_str(&format!("  {} (file)\n", hit.file));
        }
    }
    output.trim_end().to_string()
}

/// Lean-format fallback for the unified path: no exact-match promotion, every
/// hit rendered in rank order grouped by file.
fn format_unified_lean(query: &str, hits: &[SearchHit], total_found: usize) -> String {
    let mut output = String::new();
    let count = hits.len();
    if count == total_found {
        output.push_str(&format!("{} matches for \"{}\":\n\n", count, query));
    } else {
        output.push_str(&format!(
            "{} matches for \"{}\" (showing {} of {}):\n\n",
            count, query, count, total_found
        ));
    }
    let refs: Vec<&SearchHit> = hits.iter().collect();
    write_unified_hits_grouped(&mut output, &refs);
    output.trim_end().to_string()
}

fn write_unified_hits_grouped(output: &mut String, hits: &[&SearchHit]) {
    // Group by file path preserving order of first appearance.  Symbol and
    // file hits sharing the same path collapse into a single group, with the
    // file row contributing a leading `(file, lang)` marker.
    let mut groups: Vec<(String, Vec<&SearchHit>)> = Vec::new();
    for hit in hits {
        let path = hit.file.clone();
        if let Some(group) = groups.iter_mut().find(|(p, _)| p == &path) {
            group.1.push(hit);
        } else {
            groups.push((path, vec![hit]));
        }
    }

    for (file_path, group_hits) in &groups {
        if group_hits.len() == 1 {
            let hit = group_hits[0];
            if let Some(symbol) = hit.as_symbol() {
                output.push_str(&format!("{}:{}\n", file_path, symbol.start_line));
                write_definition_other_match_snippet(output, symbol, "  ");
            } else {
                // File row: render as a single path line with file marker.
                let lang = if hit.language.is_empty() {
                    String::new()
                } else {
                    format!(", {}", hit.language)
                };
                output.push_str(&format!("{} (file{})\n", file_path, lang));
            }
        } else {
            output.push_str(&format!("{}:\n", file_path));
            for hit in group_hits {
                if let Some(symbol) = hit.as_symbol() {
                    output.push_str(&format!("  :{}\n", symbol.start_line));
                    write_definition_other_match_snippet(output, symbol, "    ");
                    output.push('\n');
                } else {
                    let lang = if hit.language.is_empty() {
                        String::new()
                    } else {
                        format!(", {}", hit.language)
                    };
                    output.push_str(&format!("  (file{})\n", lang));
                }
            }
        }
        output.push('\n');
    }
}

fn write_unified_hit_promoted(output: &mut String, hit: &SearchHit) {
    if let Some(symbol) = hit.as_symbol() {
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
        if let Some(sig) = &symbol.signature {
            output.push_str(&format!("  {}\n", sig));
        } else if let Some(ctx) = &symbol.code_context {
            if let Some(first_line) = ctx.lines().find(|l| !l.trim().is_empty()) {
                output.push_str(&format!("  {}\n", first_line.trim()));
            }
        }
    } else {
        let lang = if hit.language.is_empty() {
            String::new()
        } else {
            format!(", {}", hit.language)
        };
        output.push_str(&format!("  {} (file{})\n", hit.file, lang));
    }
}

fn file_hit_matches_query(hit: &SearchHit, query_compact: &str) -> bool {
    use crate::search::index::compact_alnum_lc;
    if query_compact.is_empty() {
        return false;
    }
    let basename = hit.file.rsplit('/').next().unwrap_or(&hit.file);
    let stem = basename.rsplit_once('.').map(|(s, _)| s).unwrap_or(basename);
    // Match on basename, stem, or full path compact-form.  Full-path matching
    // handles multi-token queries like "openclaw plugin index.ts" matching
    // the file `openclaw-plugin/index.ts` end-to-end — the basename alone
    // (`index.ts`) does not compact-equal the multi-word query.
    compact_alnum_lc(basename) == query_compact
        || compact_alnum_lc(stem) == query_compact
        || compact_alnum_lc(&hit.file) == query_compact
}
