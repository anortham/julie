//! Inline tests extracted from extractors/typescript/inference.rs
//!
//! These tests verify type inference from variable assignments and function returns.
//! Tests are extracted from the original inline #[cfg(test)] module in inference.rs
//! to centralize test infrastructure and improve code organization.

use crate::extractors::typescript::TypeScriptExtractor;
use crate::extractors::typescript::inference::*;
use std::path::PathBuf;

#[test]
fn test_infer_basic_types() {
    let code = r#"
    const str = "hello";
    const num = 42;
    const bool = true;
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
    let types = infer_types(&extractor, &symbols);

    assert!(!types.is_empty());
}

#[test]
fn test_infer_function_return_type() {
    let code = r#"
    function getString(): string {
        return "hello";
    }
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
    let types = infer_types(&extractor, &symbols);

    assert!(!types.is_empty());
}

#[test]
fn test_infer_async_function() {
    let code = r#"
    async function fetchData() {
        return await fetch('/api');
    }
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
    let types = infer_types(&extractor, &symbols);

    assert!(!types.is_empty());
}
