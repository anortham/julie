//! Tests extracted from src/extractors/typescript/identifiers.rs
//!
//! This module contains inline tests that were previously embedded in the identifiers.rs module.
//! They test the identifier extraction functionality for TypeScript/JavaScript code, including
//! function calls, member access, and chained member access patterns.

use crate::extractors::base::IdentifierKind;
use crate::extractors::typescript::TypeScriptExtractor;

#[test]
fn test_extract_function_calls() {
    let code = r#"
    function foo() {}
    function bar() {
        foo();
    }
    "#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let mut extractor = TypeScriptExtractor::new(
        "typescript".to_string(),
        "test.ts".to_string(),
        code.to_string(),
    );
    let symbols = extractor.extract_symbols(&tree);
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    assert!(!identifiers.is_empty());
    assert!(identifiers
        .iter()
        .any(|id| id.name == "foo" && id.kind == IdentifierKind::Call));
}

#[test]
fn test_extract_member_access() {
    let code = r#"
    const obj = { prop: 42 };
    console.log(obj.prop);
    "#;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let mut extractor = TypeScriptExtractor::new(
        "typescript".to_string(),
        "test.ts".to_string(),
        code.to_string(),
    );
    let symbols = extractor.extract_symbols(&tree);
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    assert!(!identifiers.is_empty());
    assert!(identifiers
        .iter()
        .any(|id| id.kind == IdentifierKind::MemberAccess));
}

#[test]
fn test_extract_chained_member_access() {
    let code = "const value = obj.foo.bar.baz;";
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let mut extractor = TypeScriptExtractor::new(
        "typescript".to_string(),
        "test.ts".to_string(),
        code.to_string(),
    );
    let symbols = extractor.extract_symbols(&tree);
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    assert!(!identifiers.is_empty());
}
