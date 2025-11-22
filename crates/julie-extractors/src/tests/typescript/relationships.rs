//! Inline tests extracted from src/extractors/typescript/relationships.rs
//!
//! These tests verify the relationship extraction functionality for TypeScript code,
//! including function calls and inheritance relationships.

use crate::base::RelationshipKind;
use crate::typescript::TypeScriptExtractor;
use crate::typescript::relationships::extract_relationships;
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

    let mut extractor = TypeScriptExtractor::new(
        "typescript".to_string(),
        "test.ts".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extract_relationships(&extractor, &tree, &symbols);

    assert!(!relationships.is_empty());
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

    let mut extractor = TypeScriptExtractor::new(
        "typescript".to_string(),
        "test.ts".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extract_relationships(&extractor, &tree, &symbols);

    assert!(!relationships.is_empty());
    assert!(
        relationships
            .iter()
            .any(|r| r.kind == RelationshipKind::Extends)
    );
}
