use crate::extractors::base::{Symbol, SymbolKind};
use crate::extractors::c::CExtractor;
use tree_sitter::{Parser, Tree};

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .expect("Error loading C grammar");
    parser
}

pub fn parse_c(code: &str, file_name: &str) -> (CExtractor, Tree) {
    let mut parser = init_parser();
    let tree = parser.parse(code, None).expect("Failed to parse C code");
    let extractor = CExtractor::new("c".to_string(), file_name.to_string(), code.to_string());
    (extractor, tree)
}

pub fn extract_symbols_with_name(code: &str, file_name: &str) -> Vec<Symbol> {
    let (mut extractor, tree) = parse_c(code, file_name);
    extractor.extract_symbols(&tree)
}

pub fn extract_symbols(code: &str) -> Vec<Symbol> {
    extract_symbols_with_name(code, "test.c")
}

pub mod advanced;
pub mod basics;
pub mod identifier_extraction;
pub mod relationships;
pub mod pointers;
pub mod preprocessor;
