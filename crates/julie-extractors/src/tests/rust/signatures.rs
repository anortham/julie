/// Tests for Rust signature extraction: use declarations, macro invocations,
/// associated types, and function signatures.
///
/// Covers grouped/glob imports, aliased imports, simple imports,
/// macro invocation name handling, and static/const kind correctness.
use crate::base::{SymbolKind, Visibility};
use crate::rust::RustExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

/// Initialize Rust parser
fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("Error loading Rust grammar");
    parser
}

/// Get test workspace root
fn test_workspace_root() -> PathBuf {
    PathBuf::from("/tmp/test")
}

// ========================================================================
// Grouped and glob import tests
// ========================================================================

#[test]
fn test_grouped_use_declaration() {
    let code = r#"use std::collections::{HashMap, BTreeMap, HashSet};"#;

    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = test_workspace_root();
    let mut extractor = RustExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let imports: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Import)
        .collect();

    // Should extract the grouped import (at least one Import symbol)
    assert!(
        !imports.is_empty(),
        "Grouped import should produce at least one Import symbol"
    );

    // The combined name+signature text should mention all imported names
    let all_text: String = imports
        .iter()
        .map(|i| format!("{} {}", i.name, i.signature.as_deref().unwrap_or("")))
        .collect();
    assert!(
        all_text.contains("HashMap"),
        "Should include HashMap in import text, got: {}",
        all_text
    );
    assert!(
        all_text.contains("BTreeMap"),
        "Should include BTreeMap in import text, got: {}",
        all_text
    );
    assert!(
        all_text.contains("HashSet"),
        "Should include HashSet in import text, got: {}",
        all_text
    );
}

#[test]
fn test_glob_import() {
    let code = r#"use std::collections::*;"#;

    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = test_workspace_root();
    let mut extractor = RustExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let imports: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Import)
        .collect();

    assert!(
        !imports.is_empty(),
        "Glob import should be extracted as at least one Import symbol"
    );

    // The signature should contain the glob import text
    let sig = imports[0].signature.as_deref().unwrap_or("");
    assert!(
        sig.contains("std::collections::*"),
        "Glob import signature should contain the full path with *, got: {}",
        sig
    );
}

#[test]
fn test_pub_grouped_import() {
    let code = r#"pub use crate::models::{User, Account};"#;

    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = test_workspace_root();
    let mut extractor = RustExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let imports: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Import)
        .collect();

    assert!(
        !imports.is_empty(),
        "pub use grouped import should be extracted"
    );

    let all_text: String = imports
        .iter()
        .map(|i| format!("{} {}", i.name, i.signature.as_deref().unwrap_or("")))
        .collect();
    assert!(
        all_text.contains("User"),
        "Should include User, got: {}",
        all_text
    );
    assert!(
        all_text.contains("Account"),
        "Should include Account, got: {}",
        all_text
    );
}

#[test]
fn test_nested_grouped_import() {
    // Nested groups like use std::{fmt, collections::{HashMap, HashSet}}
    let code = r#"use std::{fmt, collections::{HashMap, HashSet}};"#;

    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = test_workspace_root();
    let mut extractor = RustExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let imports: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Import)
        .collect();

    assert!(
        !imports.is_empty(),
        "Nested grouped import should produce at least one Import symbol"
    );

    let all_text: String = imports
        .iter()
        .map(|i| format!("{} {}", i.name, i.signature.as_deref().unwrap_or("")))
        .collect();
    assert!(
        all_text.contains("HashMap"),
        "Should include HashMap, got: {}",
        all_text
    );
}

// ========================================================================
// Static item kind tests
// ========================================================================

#[test]
fn test_static_is_constant() {
    let code = r#"static MAX_SIZE: usize = 1024;"#;

    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = test_workspace_root();
    let mut extractor = RustExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let max = symbols
        .iter()
        .find(|s| s.name == "MAX_SIZE")
        .expect("Should find MAX_SIZE symbol");

    assert_eq!(
        max.kind,
        SymbolKind::Constant,
        "Non-mut static should be Constant, not Variable"
    );
}

#[test]
fn test_static_mut_is_variable() {
    let code = r#"static mut COUNTER: u32 = 0;"#;

    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = test_workspace_root();
    let mut extractor = RustExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let counter = symbols
        .iter()
        .find(|s| s.name == "COUNTER")
        .expect("Should find COUNTER symbol");

    assert_eq!(
        counter.kind,
        SymbolKind::Variable,
        "static mut should remain Variable since it's mutable"
    );
}

#[test]
fn test_pub_static_is_constant() {
    let code = r#"pub static GLOBAL_CONFIG: &str = "default";"#;

    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = test_workspace_root();
    let mut extractor = RustExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let config = symbols
        .iter()
        .find(|s| s.name == "GLOBAL_CONFIG")
        .expect("Should find GLOBAL_CONFIG symbol");

    assert_eq!(
        config.kind,
        SymbolKind::Constant,
        "pub static (non-mut) should be Constant"
    );
    assert_eq!(config.visibility.as_ref().unwrap(), &Visibility::Public,);
}

// ========================================================================
// Macro invocation name handling tests
// ========================================================================

#[test]
fn test_expression_macros_are_not_extracted() {
    // Expression macros inside function bodies are noise — they pollute
    // the symbol index, waste embedding budget, and degrade search quality.
    let code = r#"
fn main() {
    println!("hello");
    vec![1, 2, 3];
    let x = format!("foo {}", bar);
    matches!(val, Some(_));
    assert_eq!(a, b);
    bail!("oops");
    info!("starting up");
}
"#;

    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = test_workspace_root();
    let mut extractor = RustExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);

    // None of these expression macros should be extracted
    let macro_names = [
        "println",
        "vec",
        "format",
        "matches",
        "assert_eq",
        "bail",
        "info",
    ];
    for name in &macro_names {
        let found = symbols.iter().find(|s| s.name == *name);
        assert!(
            found.is_none(),
            "expression macro {}! should NOT be extracted, but was",
            name
        );
    }

    // main function should still be extracted
    assert!(
        symbols.iter().any(|s| s.name == "main"),
        "main function should still be extracted"
    );
}

#[test]
fn test_item_position_macros_are_extracted() {
    // Declarative macros at item position (top-level or in impl/mod) define
    // named things and ARE worth extracting.
    let code = r#"
thread_local! {
    static CACHE: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

lazy_static! {
    static ref CONFIG: Config = Config::new();
}

bitflags! {
    struct Flags: u32 {
        const A = 0b0001;
    }
}
"#;

    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = test_workspace_root();
    let mut extractor = RustExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);

    // Item-position macros should be extracted
    for name in &["thread_local", "lazy_static", "bitflags"] {
        let found = symbols.iter().find(|s| s.name == *name);
        assert!(
            found.is_some(),
            "item-position macro {}! should be extracted, but wasn't. Symbols: {:?}",
            name,
            symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_no_empty_name_symbols() {
    // Ensure no symbols with empty names are ever produced
    let code = r#"
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::io::*;

static MAX: usize = 100;
static mut COUNT: u32 = 0;
const PI: f64 = 3.14;

fn main() {
    println!("hello");
}
"#;

    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = test_workspace_root();
    let mut extractor = RustExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);

    let empty_names: Vec<_> = symbols.iter().filter(|s| s.name.is_empty()).collect();
    assert!(
        empty_names.is_empty(),
        "No symbols should have empty names, but found {} with empty names",
        empty_names.len()
    );
}
