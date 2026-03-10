//! Lean output formatting for navigation tools
//!
//! Provides tool-specific text formats optimized for AI agent consumption:
//! - 70-80% fewer tokens than JSON
//! - Familiar grep-style output
//! - Zero parsing overhead

use crate::extractors::{Relationship, Symbol, SymbolKind};

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
///   src/api/auth.rs:42 (Calls)
///   src/api/profile.rs:28 (Uses)
///   src/handlers/login.rs:55 (Calls)
///   src/tests/user_test.rs:12 (Uses)
/// ```
pub fn format_lean_refs_results(
    symbol: &str,
    definitions: &[Symbol],
    references: &[Relationship],
) -> String {
    let mut output = String::new();
    let total = definitions.len() + references.len();

    // Header
    if total == 0 {
        return format!("No references found for \"{}\"", symbol);
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

    // References section
    if !references.is_empty() {
        output.push_str(&format!("References ({}):\n", references.len()));

        for rel in references {
            let kind = format!("{:?}", rel.kind);
            output.push_str(&format!(
                "  {}:{} ({})\n",
                rel.file_path, rel.line_number, kind
            ));
        }
    }

    output.trim_end().to_string()
}

/// A definition or reference tagged with its source project name.
///
/// Used by federated fast_refs (`workspace="all"`) to group results
/// by project before formatting.
pub struct ProjectTaggedResult<'a> {
    pub project_name: &'a str,
    pub definitions: &'a [Symbol],
    pub references: &'a [Relationship],
}

/// Format federated references across multiple projects in lean text format.
///
/// Output format:
/// ```text
/// 8 references to "UserService" across 2 projects:
///
/// [project: backend]
/// Definition:
///   src/services/user.rs:15 (struct) → pub struct UserService
///
/// References (3):
///   src/api/auth.rs:42 (Calls)
///   src/handlers/login.rs:55 (Calls)
///   src/tests/user_test.rs:12 (Uses)
///
/// [project: frontend]
/// References (2):
///   src/api/client.ts:28 (Calls)
///   src/hooks/useUser.ts:5 (Imports)
/// ```
pub fn format_federated_refs_results(
    symbol: &str,
    tagged_results: &[ProjectTaggedResult<'_>],
) -> String {
    let total: usize = tagged_results
        .iter()
        .map(|t| t.definitions.len() + t.references.len())
        .sum();

    if total == 0 {
        return format!("No references found for \"{}\"", symbol);
    }

    let project_count = tagged_results.len();
    let mut output = format!(
        "{} references to \"{}\" across {} project{}:\n",
        total,
        symbol,
        project_count,
        if project_count == 1 { "" } else { "s" }
    );

    for tagged in tagged_results {
        let project_total = tagged.definitions.len() + tagged.references.len();
        if project_total == 0 {
            continue;
        }

        output.push_str(&format!("\n[project: {}]\n", tagged.project_name));

        // Partition definitions into real definitions and imports
        let (real_definitions, import_definitions): (Vec<_>, Vec<_>) = tagged
            .definitions
            .iter()
            .partition(|d| d.kind != SymbolKind::Import);

        // Definitions section
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
                    output.push_str(&format!("  {}:{} ({})\n", def.file_path, def.start_line, kind));
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
                    output.push_str(&format!("  {}:{} (import)\n", def.file_path, def.start_line));
                } else {
                    output.push_str(&format!(
                        "  {}:{} (import) → {}\n",
                        def.file_path, def.start_line, sig
                    ));
                }
            }
            output.push('\n');
        }

        // References section
        if !tagged.references.is_empty() {
            output.push_str(&format!("References ({}):\n", tagged.references.len()));
            for rel in tagged.references {
                let kind = format!("{:?}", rel.kind);
                output.push_str(&format!("  {}:{} ({})\n", rel.file_path, rel.line_number, kind));
            }
        }
    }

    output.trim_end().to_string()
}

/// Truncate a signature to max length, preserving meaningful content
fn truncate_signature(sig: &str, max_len: usize) -> String {
    // Take first line only
    let first_line = sig.lines().next().unwrap_or(sig);

    if first_line.len() <= max_len {
        first_line.to_string()
    } else {
        format!("{}...", &first_line[..max_len - 3])
    }
}