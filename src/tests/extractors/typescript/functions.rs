//! Inline tests extracted from src/extractors/typescript/functions.rs
//!
//! This module contains unit tests for TypeScript function extraction functionality,
//! including function declarations, async functions, and their metadata handling.

use crate::extractors::typescript::TypeScriptExtractor;

#[test]
fn test_extract_function_with_signature() {
    let code = "function add(x: number, y: number): number { return x + y; }";
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

    assert!(symbols.iter().any(|s| s.name == "add"));
    let add_symbol = symbols.iter().find(|s| s.name == "add").unwrap();
    assert!(add_symbol.signature.is_some());
}

#[test]
fn test_extract_async_function() {
    let code = "async function fetchData() { return await fetch('/api'); }";
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

    let func_symbol = symbols.iter().find(|s| s.name == "fetchData").unwrap();
    let metadata = func_symbol.metadata.as_ref().unwrap();
    assert_eq!(
        metadata.get("isAsync").map(|v| v.as_bool()).flatten(),
        Some(true)
    );
}
