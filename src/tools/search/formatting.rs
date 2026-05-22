//! Result formatting for search responses
//!
//! Provides formatting utilities for search tool responses.

use crate::tools::search::trace::{SearchHit, SearchHitBacking};
use crate::tools::shared::OptimizedResponse;

fn write_definition_other_match_snippet(output: &mut String, hit: &SearchHit, indent: &str) {
    if let Some(snippet) = hit
        .snippet
        .as_deref()
        .filter(|snippet| !snippet.trim().is_empty())
    {
        for line in snippet.lines() {
            output.push_str(&format!("{}{}\n", indent, line.trim()));
        }
        return;
    }

    let SearchHitBacking::Symbol(symbol) = &hit.backing else {
        return;
    };

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
pub fn format_unified_locations(query: &str, hits: &[SearchHit], total_found: usize) -> String {
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
                write_definition_other_match_snippet(output, hit, "  ");
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
                    write_definition_other_match_snippet(output, hit, "    ");
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
    let stem = basename
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(basename);
    // Match on basename, stem, or full path compact-form.  Full-path matching
    // handles multi-token queries like "openclaw plugin index.ts" matching
    // the file `openclaw-plugin/index.ts` end-to-end — the basename alone
    // (`index.ts`) does not compact-equal the multi-word query.
    compact_alnum_lc(basename) == query_compact
        || compact_alnum_lc(stem) == query_compact
        || compact_alnum_lc(&hit.file) == query_compact
}
