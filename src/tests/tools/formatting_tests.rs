//! Tests for navigation output formatting (format_lean_refs_results)

use std::collections::HashMap;

use crate::extractors::base::{RelationshipKind, SymbolKind, Visibility};
use crate::extractors::{Relationship, Symbol};
use crate::search::similarity::SimilarEntry;
use crate::tools::navigation::formatting::{format_lean_refs_results, format_semantic_fallback};
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
        annotations: Vec::new(),
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

    let output = format_lean_refs_results("UserService", &defs, &refs, &HashMap::new());

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
    let output = format_lean_refs_results("Unknown", &[], &[], &HashMap::new());
    assert!(
        output.contains("No references found for \"Unknown\""),
        "Should contain 'No references found' message, got: {}",
        output
    );
    assert!(
        output.contains("fast_search"),
        "Should contain recovery hint suggesting fast_search, got: {}",
        output
    );
}

#[test]
fn test_truncate_signature() {
    // truncate_signature is private, so we test it indirectly via format_lean_refs_results
    // with a very long signature
    let long_sig = "pub fn very_long_function_name_with_many_parameters(a: i32, b: String, c: Vec<u8>, d: HashMap<String, Vec<u8>>, e: Option<Box<dyn Fn()>>)";
    let defs = vec![make_test_symbol(
        "src/lib.rs",
        1,
        SymbolKind::Function,
        Some(long_sig),
    )];
    let output = format_lean_refs_results("test_fn", &defs, &[], &HashMap::new());
    // The output should contain the definition but signature should be truncated
    assert!(output.contains("src/lib.rs:1 (function)"));
    assert!(output.contains("→ "));
}

#[test]
fn test_lean_refs_uses_first_signature_line_only() {
    let multiline_sig = "fn example(\n    arg: String,\n)";
    let defs = vec![make_test_symbol(
        "src/lib.rs",
        1,
        SymbolKind::Function,
        Some(multiline_sig),
    )];

    let output = format_lean_refs_results("example", &defs, &[], &HashMap::new());

    assert!(output.contains("→ fn example("));
    assert!(
        !output.contains("arg: String"),
        "signature formatting should stay single-line. Got:\n{}",
        output
    );
}

#[test]
fn test_truncate_signature_keeps_total_length_within_limit() {
    let long_sig =
        "fn this_signature_is_deliberately_long_enough_to_require_truncation_with_suffix()";
    let defs = vec![make_test_symbol(
        "src/lib.rs",
        1,
        SymbolKind::Function,
        Some(long_sig),
    )];

    let output = format_lean_refs_results("test_fn", &defs, &[], &HashMap::new());
    let signature = output
        .lines()
        .find_map(|line| line.split("→ ").nth(1))
        .expect("formatted output should include truncated signature");

    assert!(
        signature.ends_with("..."),
        "signature should be truncated: {signature}"
    );
    assert!(
        signature.len() <= 60,
        "truncated signature should respect the 60-char cap, got {} chars: {}",
        signature.len(),
        signature
    );
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

    let output = format_lean_refs_results("UserService", &defs, &refs, &HashMap::new());

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

    let output = format_lean_refs_results("UserService", &defs, &[], &HashMap::new());

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

// --- Reference line format tests (Step 3 + 4: names + unified format) ---

#[test]
fn test_lean_refs_includes_source_symbol_names() {
    let defs = vec![make_test_symbol(
        "src/user.rs",
        15,
        SymbolKind::Struct,
        Some("pub struct UserService"),
    )];
    let mut refs = vec![make_test_relationship(
        "src/api.rs",
        42,
        RelationshipKind::Calls,
    )];
    // Set a known from_symbol_id so we can provide a name for it
    refs[0].from_symbol_id = "caller_123".to_string();

    let mut source_names = HashMap::new();
    source_names.insert("caller_123".to_string(), "handle_request".to_string());

    let output = format_lean_refs_results("UserService", &defs, &refs, &source_names);

    // Unified format: file:line  name (Kind)
    assert!(
        output.contains("src/api.rs:42  handle_request (Calls)"),
        "Reference line should include source symbol name. Got:\n{}",
        output
    );
}

#[test]
fn test_lean_refs_graceful_without_source_names() {
    let refs = vec![make_test_relationship(
        "src/api.rs",
        42,
        RelationshipKind::Uses,
    )];

    // Empty source names — should fall back to file:line (Kind) without name
    let output = format_lean_refs_results("Foo", &[], &refs, &HashMap::new());

    assert!(
        output.contains("src/api.rs:42 (Uses)"),
        "Should fall back to kind-only when no source name available. Got:\n{}",
        output
    );
}

// --- Group-by-file references tests ---

#[test]
fn test_lean_refs_groups_same_file_references() {
    // Two refs in src/api/auth.rs, one in src/handlers/login.rs
    let defs = vec![make_test_symbol(
        "src/user.rs",
        15,
        SymbolKind::Struct,
        Some("pub struct UserService"),
    )];

    let mut refs = vec![
        make_test_relationship("src/api/auth.rs", 42, RelationshipKind::Calls),
        make_test_relationship("src/api/auth.rs", 78, RelationshipKind::Calls),
        make_test_relationship("src/handlers/login.rs", 55, RelationshipKind::Calls),
    ];
    refs[0].from_symbol_id = "caller_a".to_string();
    refs[1].from_symbol_id = "caller_b".to_string();
    refs[2].from_symbol_id = "caller_c".to_string();

    let mut source_names = HashMap::new();
    source_names.insert("caller_a".to_string(), "handle_request".to_string());
    source_names.insert("caller_b".to_string(), "validate_token".to_string());
    source_names.insert("caller_c".to_string(), "login".to_string());

    let output = format_lean_refs_results("UserService", &defs, &refs, &source_names);

    // The grouped file should appear only once as a header
    let auth_occurrences = output.matches("src/api/auth.rs").count();
    assert_eq!(
        auth_occurrences, 1,
        "src/api/auth.rs should appear once (grouped header). Got:\n{output}"
    );

    // Grouped header format: indented file path with colon
    assert!(
        output.contains("  src/api/auth.rs:\n"),
        "Multi-ref file should use grouped header format. Got:\n{output}"
    );

    // Grouped entries use :line format
    assert!(
        output.contains(":42  handle_request (Calls)"),
        "First grouped ref should use :line format. Got:\n{output}"
    );
    assert!(
        output.contains(":78  validate_token (Calls)"),
        "Second grouped ref should use :line format. Got:\n{output}"
    );

    // Single-file ref stays inline
    assert!(
        output.contains("src/handlers/login.rs:55  login (Calls)"),
        "Single-file ref should stay inline. Got:\n{output}"
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

#[test]
fn test_format_semantic_fallback_with_results() {
    let entries = vec![SimilarEntry {
        symbol: Symbol {
            id: "s1".to_string(),
            name: "UserDto".to_string(),
            kind: SymbolKind::Class,
            language: "csharp".to_string(),
            file_path: "src/api/models.cs".to_string(),
            start_line: 45,
            end_line: 80,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            parent_id: None,
            signature: None,
            visibility: Some(Visibility::Public),
            doc_comment: None,
            content_type: None,
            confidence: None,
            semantic_group: None,
            metadata: None,
            code_context: None,
            annotations: Vec::new(),
        },
        score: 0.82,
    }];

    let output = format_semantic_fallback("IUser", &entries);
    assert!(
        output.contains("Related symbols (semantic)"),
        "Should contain header. Got:\n{}",
        output
    );
    assert!(
        output.contains("UserDto"),
        "Should contain symbol name. Got:\n{}",
        output
    );
    assert!(
        output.contains("0.82"),
        "Should contain score. Got:\n{}",
        output
    );
    assert!(
        output.contains("src/api/models.cs:45"),
        "Should contain file:line. Got:\n{}",
        output
    );
}

#[test]
fn test_format_semantic_fallback_empty() {
    let output = format_semantic_fallback("IUser", &[]);
    assert!(
        output.is_empty(),
        "Should return empty string for no results"
    );
}
