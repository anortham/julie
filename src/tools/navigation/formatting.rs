//! Lean output formatting for navigation tools
//!
//! Provides tool-specific text formats optimized for AI agent consumption:
//! - 70-80% fewer tokens than JSON
//! - Familiar grep-style output
//! - Zero parsing overhead

use crate::extractors::{Relationship, Symbol};

/// Format references in lean text format for AI agents
///
/// Output format:
/// ```text
/// 5 references to "UserService":
///
/// Definition:
///   src/services/user.rs:15 (struct) → pub struct UserService
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

    output.push_str(&format!("{} references to \"{}\":\n\n", total, symbol));

    // Definitions section
    if !definitions.is_empty() {
        if definitions.len() == 1 {
            output.push_str("Definition:\n");
        } else {
            output.push_str(&format!("Definitions ({}):\n", definitions.len()));
        }

        for def in definitions {
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

/// Format goto definitions in lean text format for AI agents
///
/// Output format:
/// ```text
/// Found 2 definitions for "UserService":
///
/// src/services/user.rs:15 (struct)
///   pub struct UserService {
///
/// src/services/user.rs:45 (impl)
///   impl UserService {
/// ```
pub fn format_lean_goto_results(symbol: &str, definitions: &[Symbol]) -> String {
    let mut output = String::new();

    if definitions.is_empty() {
        return format!("No definitions found for \"{}\"", symbol);
    }

    // Header
    if definitions.len() == 1 {
        output.push_str(&format!("Found 1 definition for \"{}\":\n\n", symbol));
    } else {
        output.push_str(&format!(
            "Found {} definitions for \"{}\":\n\n",
            definitions.len(),
            symbol
        ));
    }

    // Each definition
    for def in definitions {
        let kind = format!("{:?}", def.kind).to_lowercase();

        // File:line header with kind
        output.push_str(&format!("{}:{} ({})\n", def.file_path, def.start_line, kind));

        // Signature if available (indented)
        if let Some(sig) = &def.signature {
            let truncated = truncate_signature(sig, 80);
            output.push_str(&format!("  {}\n", truncated));
        }

        output.push('\n');
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractors::base::{RelationshipKind, SymbolKind};

    fn make_test_symbol(file_path: &str, line: u32, kind: SymbolKind, sig: Option<&str>) -> Symbol {
        Symbol {
            id: format!("test_{}_{}", file_path, line),
            name: "TestSymbol".to_string(),
            kind,
            language: "rust".to_string(),
            file_path: file_path.to_string(),
            start_line: line,
            end_line: line + 5,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 0,
            parent_id: None,
            signature: sig.map(|s| s.to_string()),
            doc_comment: None,
            visibility: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
        }
    }

    fn make_test_relationship(file_path: &str, line: u32, kind: RelationshipKind) -> Relationship {
        Relationship {
            id: format!("rel_{}_{}", file_path, line),
            from_symbol_id: "caller".to_string(),
            to_symbol_id: "target".to_string(),
            kind,
            file_path: file_path.to_string(),
            line_number: line,
            confidence: 1.0,
            metadata: None,
        }
    }

    #[test]
    fn test_lean_refs_single_definition() {
        let defs = vec![make_test_symbol(
            "src/user.rs",
            15,
            SymbolKind::Struct,
            Some("pub struct UserService"),
        )];
        let refs = vec![
            make_test_relationship("src/api.rs", 42, RelationshipKind::Calls),
            make_test_relationship("src/handler.rs", 55, RelationshipKind::Uses),
        ];

        let output = format_lean_refs_results("UserService", &defs, &refs);

        assert!(output.contains("3 references to \"UserService\":"));
        assert!(output.contains("Definition:"));
        assert!(output.contains("src/user.rs:15 (struct)"));
        assert!(output.contains("→ pub struct UserService"));
        assert!(output.contains("References (2):"));
        assert!(output.contains("src/api.rs:42 (Calls)"));
        assert!(output.contains("src/handler.rs:55 (Uses)"));
    }

    #[test]
    fn test_lean_refs_no_results() {
        let output = format_lean_refs_results("Unknown", &[], &[]);
        assert_eq!(output, "No references found for \"Unknown\"");
    }

    #[test]
    fn test_lean_goto_single_definition() {
        let defs = vec![make_test_symbol(
            "src/user.rs",
            15,
            SymbolKind::Struct,
            Some("pub struct UserService { db: Pool }"),
        )];

        let output = format_lean_goto_results("UserService", &defs);

        assert!(output.contains("Found 1 definition for \"UserService\":"));
        assert!(output.contains("src/user.rs:15 (struct)"));
        assert!(output.contains("pub struct UserService { db: Pool }"));
    }

    #[test]
    fn test_lean_goto_multiple_definitions() {
        let defs = vec![
            make_test_symbol("src/user.rs", 15, SymbolKind::Struct, Some("pub struct User")),
            make_test_symbol("src/user.rs", 45, SymbolKind::Method, Some("fn new() -> Self")),
        ];

        let output = format_lean_goto_results("User", &defs);

        assert!(output.contains("Found 2 definitions for \"User\":"));
        assert!(output.contains("src/user.rs:15 (struct)"));
        assert!(output.contains("src/user.rs:45 (method)"));
    }

    #[test]
    fn test_lean_goto_no_results() {
        let output = format_lean_goto_results("Unknown", &[]);
        assert_eq!(output, "No definitions found for \"Unknown\"");
    }

    #[test]
    fn test_truncate_signature() {
        let short = "fn foo()";
        assert_eq!(truncate_signature(short, 20), "fn foo()");

        let long = "pub fn very_long_function_name_with_many_parameters(a: i32, b: String, c: Vec<u8>)";
        let truncated = truncate_signature(long, 40);
        assert!(truncated.len() <= 40);
        assert!(truncated.ends_with("..."));
    }
}
