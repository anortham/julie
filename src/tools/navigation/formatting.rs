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
    use crate::extractors::base::{RelationshipKind, SymbolKind, Visibility};
    use crate::tools::navigation::resolution::parse_qualified_name;

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
    fn test_truncate_signature() {
        let short = "fn foo()";
        assert_eq!(truncate_signature(short, 20), "fn foo()");

        let long =
            "pub fn very_long_function_name_with_many_parameters(a: i32, b: String, c: Vec<u8>)";
        let truncated = truncate_signature(long, 40);
        assert!(truncated.len() <= 40);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_lean_refs_separates_imports_from_definitions() {
        let class_def = make_test_symbol(
            "src/services/user.rs",
            15,
            SymbolKind::Struct,
            Some("pub struct UserService"),
        );
        let import1 = make_test_symbol("src/api/auth.rs", 3, SymbolKind::Import, None);
        let import2 = make_test_symbol("src/handlers/login.rs", 5, SymbolKind::Import, None);

        let defs = vec![class_def, import1, import2];
        let refs = vec![make_test_relationship(
            "src/api/auth.rs",
            42,
            RelationshipKind::Calls,
        )];

        let output = format_lean_refs_results("UserService", &defs, &refs);

        // Total should include all definitions + references
        assert!(
            output.contains("4 references to \"UserService\":"),
            "Should show total count of 4. Got:\n{}",
            output
        );
        // Real definition section
        assert!(
            output.contains("Definition:\n"),
            "Should have Definition section. Got:\n{}",
            output
        );
        assert!(
            output.contains("src/services/user.rs:15 (struct)"),
            "Should show struct definition"
        );
        // Imports section (separate from definitions)
        assert!(
            output.contains("Imports (2):\n"),
            "Should have Imports section with count. Got:\n{}",
            output
        );
        assert!(
            output.contains("src/api/auth.rs:3 (import)"),
            "Should show import"
        );
        assert!(
            output.contains("src/handlers/login.rs:5 (import)"),
            "Should show import"
        );
        // References section
        assert!(
            output.contains("References (1):"),
            "Should have References section"
        );
    }

    #[test]
    fn test_lean_refs_single_import_uses_singular() {
        let import = make_test_symbol("src/api/auth.rs", 3, SymbolKind::Import, None);
        let defs = vec![import];

        let output = format_lean_refs_results("UserService", &defs, &[]);

        assert!(
            output.contains("Import:\n"),
            "Should use singular 'Import:' for single import. Got:\n{}",
            output
        );
        // Should NOT show "Definition:" since there are no real definitions
        assert!(
            !output.contains("Definition:"),
            "Should not show Definition section when only imports exist"
        );
    }

    // --- Qualified name parsing tests ---

    #[test]
    fn test_parse_qualified_name_with_double_colon() {
        assert_eq!(
            parse_qualified_name("MyClass::method"),
            Some(("MyClass", "method"))
        );
    }

    #[test]
    fn test_parse_qualified_name_with_dot() {
        assert_eq!(
            parse_qualified_name("MyClass.method"),
            Some(("MyClass", "method"))
        );
    }

    #[test]
    fn test_parse_qualified_name_nested() {
        // Splits on LAST separator
        assert_eq!(
            parse_qualified_name("Namespace::Class::method"),
            Some(("Namespace::Class", "method"))
        );
    }

    #[test]
    fn test_parse_qualified_name_no_separator() {
        assert_eq!(parse_qualified_name("standalone_function"), None);
    }

    #[test]
    fn test_parse_qualified_name_trailing_separator() {
        assert_eq!(parse_qualified_name("MyClass::"), None);
        assert_eq!(parse_qualified_name("MyClass."), None);
    }

    #[test]
    fn test_parse_qualified_name_leading_separator() {
        assert_eq!(parse_qualified_name("::method"), None);
        assert_eq!(parse_qualified_name(".method"), None);
    }

    #[test]
    fn test_parse_qualified_name_dot_nested() {
        assert_eq!(
            parse_qualified_name("package.Class.method"),
            Some(("package.Class", "method"))
        );
    }
}
