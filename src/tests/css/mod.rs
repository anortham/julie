use crate::extractors::base::Symbol;
use crate::extractors::css::CSSExtractor;
use tree_sitter::Parser;

pub fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_css::LANGUAGE.into())
        .expect("Error loading CSS grammar");
    parser
}

pub fn extract_symbols(code: &str) -> Vec<Symbol> {
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();
    let mut extractor =
        CSSExtractor::new("css".to_string(), "test.css".to_string(), code.to_string());
    extractor.extract_symbols(&tree)
}

pub mod advanced;
pub mod at_rules;
pub mod basic;
pub mod custom;
pub mod identifier_extraction;
pub mod modern;
