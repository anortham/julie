use crate::base::{BaseExtractor, SymbolKind, SymbolOptions};
use tree_sitter::Parser;

#[test]
fn create_symbol_records_body_span_for_tree_sitter_body_node() {
    let content = "fn hello() {\n    let value = 1;\n}\n";
    let mut extractor = rust_extractor(content);
    let tree = parse_rust(content);
    let function = tree
        .root_node()
        .child(0)
        .expect("rust fixture should contain a function item");

    let symbol = extractor.create_symbol(
        &function,
        "hello".to_string(),
        SymbolKind::Function,
        SymbolOptions::default(),
    );

    let body_span = symbol
        .body_span
        .expect("function symbols should carry a body span");
    assert_eq!(
        &content[body_span.start_byte as usize..body_span.end_byte as usize],
        "{\n    let value = 1;\n}"
    );
    assert!(symbol.body_hash.is_some(), "body span requires body hash");
}

#[test]
fn body_hash_ignores_whitespace_only_formatting_changes() {
    let compact = symbol_for_rust_function("fn hello(){let value=1;}\n");
    let spaced = symbol_for_rust_function("fn hello() {\n    let value = 1;\n}\n");

    assert_eq!(compact.body_hash, spaced.body_hash);
}

fn symbol_for_rust_function(content: &str) -> crate::base::Symbol {
    let mut extractor = rust_extractor(content);
    let tree = parse_rust(content);
    let function = tree
        .root_node()
        .child(0)
        .expect("rust fixture should contain a function item");

    extractor.create_symbol(
        &function,
        "hello".to_string(),
        SymbolKind::Function,
        SymbolOptions::default(),
    )
}

fn rust_extractor(content: &str) -> BaseExtractor {
    BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        std::path::Path::new("/tmp/test"),
    )
}

fn parse_rust(content: &str) -> tree_sitter::Tree {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("rust parser should load");
    parser
        .parse(content, None)
        .expect("rust fixture should parse")
}
