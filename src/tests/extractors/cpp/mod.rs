use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::extractors::cpp::CppExtractor;
use std::path::PathBuf;
use tree_sitter::{Parser, Tree};

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_cpp::LANGUAGE.into())
        .expect("Error loading C++ grammar");
    parser
}

pub fn parse_cpp(code: &str) -> (CppExtractor, Tree) {
    let mut parser = init_parser();
    let tree = parser.parse(code, None).expect("Failed to parse C++ code");
    let workspace_root = PathBuf::from("/tmp/test");
    let extractor = CppExtractor::new("test.cpp".to_string(), code.to_string(), &workspace_root);
    (extractor, tree)
}

pub fn extract_symbols(code: &str) -> Vec<Symbol> {
    let (mut extractor, tree) = parse_cpp(code);
    extractor.extract_symbols(&tree)
}

pub fn extract_symbols_and_relationships(code: &str) -> (Vec<Symbol>, Vec<Relationship>) {
    let (mut extractor, tree) = parse_cpp(code);
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);
    (symbols, relationships)
}

pub mod classes;
pub mod concurrency;
pub mod doxygen_comments;
pub mod exceptions;
pub mod functions;
pub mod identifier_extraction;
pub mod modern;
pub mod namespaces;
pub mod robustness;
pub mod templates;
pub mod testing;
pub mod types;
