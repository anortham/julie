//! TypeScript symbols module inline tests extracted from src/extractors/typescript/symbols.rs
//!
//! This module contains all test functions that were previously defined inline in the
//! TypeScript symbols extraction module. They test core functionality for symbol routing
//! and extraction including:
//! - Multi-symbol kind extraction (classes, functions, interfaces, etc.)
//! - Symbol type routing via visit_node
//! - Comprehensive symbol collection from mixed syntax

use crate::extractors::base::SymbolKind;
use crate::extractors::typescript::TypeScriptExtractor;
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
