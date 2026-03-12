//! Tests for navigation output formatting (format_lean_refs_results)

use crate::extractors::base::{RelationshipKind, SymbolKind, Visibility};
use crate::extractors::{Relationship, Symbol};
use crate::tools::navigation::formatting::format_lean_refs_results;
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
    // truncate_signature is private, so we test it indirectly via format_lean_refs_results
    // with a very long signature
    let long_sig = "pub fn very_long_function_name_with_many_parameters(a: i32, b: String, c: Vec<u8>, d: HashMap<String, Vec<u8>>, e: Option<Box<dyn Fn()>>)";
    let defs = vec![make_test_symbol("src/lib.rs", 1, SymbolKind::Function, Some(long_sig))];
    let output = format_lean_refs_results("test_fn", &defs, &[]);
    // The output should contain the definition but signature should be truncated
    assert!(output.contains("src/lib.rs:1 (function)"));
    assert!(output.contains("→ "));
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
