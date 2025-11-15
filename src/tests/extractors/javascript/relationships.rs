//! Inline tests extracted from src/extractors/javascript/relationships.rs
//!
//! Tests for JavaScript relationship extraction (calls, inheritance)

use crate::extractors::base::RelationshipKind;
use std::path::PathBuf;

#[test]
fn test_extract_call_relationships() {
    let code = r#"
    function caller() {
        callee();
    }
    function callee() {}
    "#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = crate::extractors::javascript::JavaScriptExtractor::new(
        "javascript".to_string(),
        "test.js".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    assert!(
        !relationships.is_empty(),
        "Should extract call relationships"
    );
    assert!(
        relationships
            .iter()
            .any(|r| r.kind == RelationshipKind::Calls)
    );
}

#[test]
fn test_extract_inheritance_relationships() {
    let code = r#"
    class Animal {}
    class Dog extends Animal {}
    "#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = crate::extractors::javascript::JavaScriptExtractor::new(
        "javascript".to_string(),
        "test.js".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    assert!(
        !relationships.is_empty(),
        "Should extract inheritance relationships"
    );
    assert!(
        relationships
            .iter()
            .any(|r| r.kind == RelationshipKind::Extends)
    );
}
