//! Lean output formatting for navigation tools
//!
//! Provides tool-specific text formats optimized for AI agent consumption:
//! - 70-80% fewer tokens than JSON
//! - Familiar grep-style output
//! - Zero parsing overhead

use std::collections::HashMap;

use crate::extractors::{Relationship, Symbol, SymbolKind};
use crate::search::similarity::SimilarEntry;

/// Truncate a signature to `max_len` characters, appending "..." if trimmed.
fn truncate_signature(sig: &str, max_len: usize) -> String {
    let first_line = sig.lines().next().unwrap_or(sig).trim();

    if first_line.len() <= max_len {
        first_line.to_string()
    } else if max_len <= 3 {
        ".".repeat(max_len)
    } else {
        format!("{}...", &first_line[..max_len - 3])
    }
}

/// Format references in lean text format for AI agents
///
/// Output format:
/// ```text
/// 5 references to "UserService":
///
/// Definition:
///   src/services/user.rs:15 (struct) → pub struct UserService
///
/// Imports (2):
///   src/api/auth.rs:3 (import)
///   src/handlers/login.rs:5 (import)
///
/// References (4):
///   src/api/auth.rs:42  handle_request (Calls)
///   src/api/profile.rs:28  get_profile (Uses)
///   src/handlers/login.rs:55  login (Calls)
///   src/tests/user_test.rs:12  test_user (Uses)
/// ```
pub fn format_lean_refs_results(
    symbol: &str,
    definitions: &[Symbol],
    references: &[Relationship],
    source_names: &HashMap<String, String>,
) -> String {
    let mut output = String::new();
    let total = definitions.len() + references.len();

    // Header
    if total == 0 {
        return format!(
            "No references found for \"{}\"\nCheck spelling, or try fast_search(query=\"{}\", search_target=\"definitions\") to verify the symbol exists",
            symbol, symbol
        );
    }

    // Partition definitions into real definitions and imports
    let (real_definitions, import_definitions): (Vec<_>, Vec<_>) = definitions
        .iter()
        .partition(|d| d.kind != SymbolKind::Import);

    output.push_str(&format!("{} references to \"{}\":\n\n", total, symbol));

    // Definitions section (non-import symbols)
    if !real_definitions.is_empty() {
        if real_definitions.len() == 1 {
            output.push_str("Definition:\n");
        } else {
            output.push_str(&format!("Definitions ({}):\n", real_definitions.len()));
        }

        for def in &real_definitions {
            let kind = format!("{:?}", def.kind).to_lowercase();
            let sig = def
                .signature
                .as_ref()
                .map(|s| truncate_signature(s, 60))
                .unwrap_or_default();

            if sig.is_empty() {
                output.push_str(&format!(
                    "  {}:{} ({})\n",
                    def.file_path, def.start_line, kind
                ));
            } else {
                output.push_str(&format!(
                    "  {}:{} ({}) → {}\n",
                    def.file_path, def.start_line, kind, sig
                ));
            }
        }
        output.push('\n');
    }

    // Imports section
    if !import_definitions.is_empty() {
        if import_definitions.len() == 1 {
            output.push_str("Import:\n");
        } else {
            output.push_str(&format!("Imports ({}):\n", import_definitions.len()));
        }

        for def in &import_definitions {
            let sig = def
                .signature
                .as_ref()
                .map(|s| truncate_signature(s, 60))
                .unwrap_or_default();

            if sig.is_empty() {
                output.push_str(&format!(
                    "  {}:{} (import)\n",
                    def.file_path, def.start_line
                ));
            } else {
                output.push_str(&format!(
                    "  {}:{} (import) → {}\n",
                    def.file_path, def.start_line, sig
                ));
            }
        }
        output.push('\n');
    }

    // References section — group by file when multiple refs share a file
    if !references.is_empty() {
        output.push_str(&format!("References ({}):\n", references.len()));

        // Group references by file path (preserving order of first appearance)
        let mut groups: Vec<(&str, Vec<&Relationship>)> = Vec::new();
        for rel in references {
            let path = rel.file_path.as_str();
            if let Some(group) = groups.iter_mut().find(|(p, _)| *p == path) {
                group.1.push(rel);
            } else {
                groups.push((path, vec![rel]));
            }
        }

        for (file_path, rels) in &groups {
            if rels.len() == 1 {
                // Single ref in file: keep inline format
                let rel = rels[0];
                let kind = format!("{:?}", rel.kind);
                let name = source_names.get(&rel.from_symbol_id);
                if let Some(name) = name {
                    output.push_str(&format!(
                        "  {}:{}  {} ({})\n",
                        file_path, rel.line_number, name, kind
                    ));
                } else {
                    output.push_str(&format!("  {}:{} ({})\n", file_path, rel.line_number, kind));
                }
            } else {
                // Multiple refs in same file: group under file header
                output.push_str(&format!("  {}:\n", file_path));
                for rel in rels.iter() {
                    let kind = format!("{:?}", rel.kind);
                    let name = source_names.get(&rel.from_symbol_id);
                    if let Some(name) = name {
                        output
                            .push_str(&format!("    :{}  {} ({})\n", rel.line_number, name, kind));
                    } else {
                        output.push_str(&format!("    :{} ({})\n", rel.line_number, kind));
                    }
                }
            }
        }
    }

    output.trim_end().to_string()
}

/// Format semantic similarity results for the zero-ref fallback in fast_refs.
pub fn format_semantic_fallback(symbol: &str, similar: &[SimilarEntry]) -> String {
    if similar.is_empty() {
        return String::new();
    }

    let mut out = String::from("\nRelated symbols (semantic):\n");

    for entry in similar {
        let kind = entry.symbol.kind.to_string();
        let vis = entry
            .symbol
            .visibility
            .as_ref()
            .map(|v| v.to_string().to_lowercase())
            .unwrap_or_default();
        let kind_vis = if vis.is_empty() {
            kind
        } else {
            format!("{}, {}", kind, vis)
        };

        out.push_str(&format!(
            "  {:<25} {:.2}  {}:{} ({})\n",
            entry.symbol.name,
            entry.score,
            entry.symbol.file_path,
            entry.symbol.start_line,
            kind_vis,
        ));
    }

    out.push_str(&format!(
        "\nThese are semantically similar to \"{}\", not exact references",
        symbol
    ));

    out
}
