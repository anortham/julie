//! TypeScript symbols module inline tests extracted from src/extractors/typescript/symbols.rs
//!
//! This module contains all test functions that were previously defined inline in the
//! TypeScript symbols extraction module. They test core functionality for symbol routing
//! and extraction including:
//! - Multi-symbol kind extraction (classes, functions, interfaces, etc.)
//! - Symbol type routing via visit_node
//! - Comprehensive symbol collection from mixed syntax

use crate::base::SymbolKind;
use crate::typescript::TypeScriptExtractor;
use std::path::PathBuf;

#[test]
fn test_visit_all_symbol_kinds() {
    let code = r#"
    class MyClass {
        prop: string;
        method() {}
    }

    function myFunc() {}
    const myVar = 42;
    interface MyInterface {}
    type MyType = string;
    enum MyEnum { A, B }
    import { foo } from './bar';
    export { myVar };
    namespace MyNamespace {}
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

    assert!(!symbols.is_empty(), "Should extract some symbols");
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "MyClass" && s.kind == SymbolKind::Class),
        "Should extract class"
    );
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "myFunc" && s.kind == SymbolKind::Function),
        "Should extract function"
    );
    assert!(
        symbols.iter().any(|s| s.name == "myVar"),
        "Should extract variable"
    );
}

#[test]
fn test_enum_members_extracted() {
    let code = r#"
    enum Direction {
        Up,
        Down,
        Left,
        Right
    }
    "#;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
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

    // Should have the enum itself
    let direction_enum = symbols.iter().find(|s| s.name == "Direction" && s.kind == SymbolKind::Enum);
    assert!(direction_enum.is_some(), "Should extract Direction enum");
    let enum_id = &direction_enum.unwrap().id;

    // Should have enum members
    let members: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::EnumMember && s.parent_id.as_ref() == Some(enum_id))
        .collect();
    assert!(
        members.len() >= 2,
        "Should extract at least 2 enum members, got {}",
        members.len()
    );
}

#[test]
fn test_class_signature_with_extends() {
    let code = r#"
    class Animal {
        name: string;
    }

    class Dog extends Animal {
        breed: string;
    }
    "#;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
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

    let dog_class = symbols.iter().find(|s| s.name == "Dog" && s.kind == SymbolKind::Class);
    assert!(dog_class.is_some(), "Should extract Dog class");
    let dog = dog_class.unwrap();
    assert!(
        dog.signature.as_ref().map_or(false, |sig| sig.contains("extends Animal")),
        "Dog class signature should contain 'extends Animal', got: {:?}",
        dog.signature
    );
}
