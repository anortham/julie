//! Inline tests extracted from src/extractors/typescript/functions.rs
//!
//! This module contains unit tests for TypeScript function extraction functionality,
//! including function declarations, async functions, and their metadata handling.

use crate::base::SymbolKind;
use crate::typescript::TypeScriptExtractor;
use std::path::PathBuf;

#[test]
fn test_extract_function_with_signature() {
    let code = "function add(x: number, y: number): number { return x + y; }";
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

    let workspace_root = PathBuf::from("/tmp/test");

    let mut extractor = TypeScriptExtractor::new(
        "typescript".to_string(),
        "test.ts".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    let func_symbol = symbols.iter().find(|s| s.name == "fetchData").unwrap();
    let metadata = func_symbol.metadata.as_ref().unwrap();
    assert_eq!(
        metadata.get("isAsync").map(|v| v.as_bool()).flatten(),
        Some(true)
    );
}

#[test]
fn test_extract_function_with_jsdoc_comment() {
    let code = "/**\n * Validates user input\n * @param email - The email to validate\n * @returns True if valid\n */\nfunction validateEmail(email: string): boolean { return true; }";
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

    let func_symbol = symbols.iter().find(|s| s.name == "validateEmail").unwrap();
    assert!(
        func_symbol.doc_comment.is_some(),
        "JSDoc comment should be extracted"
    );
    assert!(
        func_symbol
            .doc_comment
            .as_ref()
            .unwrap()
            .contains("Validates user input")
    );
    assert!(
        func_symbol
            .doc_comment
            .as_ref()
            .unwrap()
            .contains("@param email")
    );
}

#[test]
fn test_extract_function_without_jsdoc_comment() {
    let code = "function add(x: number, y: number): number { return x + y; }";
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

    let func_symbol = symbols.iter().find(|s| s.name == "add").unwrap();
    assert!(
        func_symbol.doc_comment.is_none(),
        "Function without JSDoc should have None"
    );
}

#[test]
fn test_typescript_same_named_classes_keep_method_parent_ids_distinct() {
    let code = r#"
class A {
  first() {}
}

class A {
  second() {}
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

    let classes: Vec<_> = symbols
        .iter()
        .filter(|symbol| symbol.name == "A" && symbol.kind == SymbolKind::Class)
        .collect();
    assert_eq!(classes.len(), 2);
    assert_ne!(classes[0].id, classes[1].id);

    let first = symbols
        .iter()
        .find(|symbol| symbol.name == "first" && symbol.kind == SymbolKind::Method)
        .expect("first method should be extracted");
    let second = symbols
        .iter()
        .find(|symbol| symbol.name == "second" && symbol.kind == SymbolKind::Method)
        .expect("second method should be extracted");

    let first_class = classes
        .iter()
        .find(|class| class.start_line <= first.start_line && class.end_line >= first.end_line)
        .expect("first method should be contained by a class");
    let second_class = classes
        .iter()
        .find(|class| class.start_line <= second.start_line && class.end_line >= second.end_line)
        .expect("second method should be contained by a class");

    assert_eq!(first.parent_id.as_deref(), Some(first_class.id.as_str()));
    assert_eq!(second.parent_id.as_deref(), Some(second_class.id.as_str()));
    assert_ne!(first.parent_id, second.parent_id);
}
