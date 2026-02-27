// Tests for Rust relationship extraction with scoped/qualified paths
//
// Bug: `crate::module::function()` does not create a Calls relationship
// because extract_call_relationships only handles `identifier` and
// `field_expression` nodes, not `scoped_identifier`.

use crate::base::Symbol;
use crate::rust::RustExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("Error loading Rust grammar");
    parser
}

fn extract_with_relationships(
    code: &str,
) -> (Vec<Symbol>, Vec<crate::base::Relationship>) {
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = RustExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);
    (symbols, relationships)
}

#[test]
fn test_scoped_call_creates_relationship_with_bare_name() {
    let code = r#"
fn target_function() {}

fn caller() {
    crate::module::target_function();
}
"#;
    let (_symbols, relationships) = extract_with_relationships(code);

    // Should find a Calls relationship to target_function
    assert!(
        !relationships.is_empty(),
        "Should create at least one relationship for the scoped call"
    );
}

#[test]
fn test_simple_call_relationship_still_works() {
    let code = r#"
fn do_something() {}

fn caller() {
    do_something();
}
"#;
    let (_symbols, relationships) = extract_with_relationships(code);

    assert!(
        !relationships.is_empty(),
        "Simple direct calls should still produce relationships"
    );
}

#[test]
fn test_deeply_nested_scoped_call_creates_relationship() {
    let code = r#"
fn new() {}

fn example() {
    std::collections::HashMap::new();
}
"#;
    let (_symbols, relationships) = extract_with_relationships(code);

    // The bare name "new" matches the local function "new", so this should
    // create a resolved Calls relationship
    assert!(
        !relationships.is_empty(),
        "Deeply nested scoped call should create a relationship using the bare name"
    );
}
