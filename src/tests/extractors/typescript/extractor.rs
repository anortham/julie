//! TypeScript extractor inline tests extracted from src/extractors/typescript/mod.rs
//!
//! This module contains all test functions that were previously defined inline in the
//! TypeScript extractor module. They test core functionality for TypeScript/JavaScript
//! symbol extraction including:
//! - Function and class declaration extraction
//! - Variable and property declaration extraction
//! - Function call relationship tracking
//! - Symbol position accuracy
//! - Inheritance relationship extraction
//! - Basic type inference
//! - Function return type handling

use crate::extractors::base::SymbolKind;
use crate::extractors::typescript::TypeScriptExtractor;
use std::path::PathBuf;

#[test]
fn test_extract_function_declarations() {
    let code = "function getUserData() { return data; }";
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

    assert!(!symbols.is_empty());
    assert!(symbols.iter().any(|s| s.name == "getUserData"));
}

#[test]
fn test_extract_class_declarations() {
    let code = r#"
    class User {
        name: string;
        constructor(name: string) {
            this.name = name;
        }
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

    assert!(
        symbols
            .iter()
            .any(|s| s.name == "User" && s.kind == SymbolKind::Class)
    );
}

#[test]
fn test_extract_variable_and_property_declarations() {
    let code = r#"
    const myVar = 42;
    const myArrowFunc = () => "hello";
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

    assert!(symbols.iter().any(|s| s.name == "myVar"));
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "myArrowFunc" && s.kind == SymbolKind::Function)
    );
}

#[test]
fn test_extract_function_call_relationships() {
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
    let relationships = extractor.extract_relationships(&tree, &symbols);

    assert!(!relationships.is_empty());
}

#[test]
fn test_track_accurate_symbol_positions() {
    let code = r#"
    function foo() {}
    function bar() {}
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

    let foo_symbol = symbols.iter().find(|s| s.name == "foo").unwrap();
    let bar_symbol = symbols.iter().find(|s| s.name == "bar").unwrap();

    assert!(foo_symbol.start_line < bar_symbol.start_line);
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
    let relationships = extractor.extract_relationships(&tree, &symbols);

    assert!(!relationships.is_empty());
}

#[test]
fn test_infer_basic_types() {
    let code = r#"
    const str = "hello";
    const num = 42;
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
    let types = extractor.infer_types(&symbols);

    assert!(!types.is_empty());
}

#[test]
fn test_handle_function_return_types() {
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

    let func_symbol = symbols.iter().find(|s| s.name == "getString").unwrap();
    assert!(func_symbol.metadata.is_some());
}
