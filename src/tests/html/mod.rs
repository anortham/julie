use crate::extractors::base::Symbol;
use crate::extractors::html::HTMLExtractor;
use tree_sitter::Parser;

pub use crate::extractors::base::SymbolKind;

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_html::LANGUAGE.into())
        .expect("Error loading HTML grammar");
    parser
}

pub fn extract_symbols(code: &str) -> Vec<Symbol> {
    let mut parser = init_parser();
    let tree = parser.parse(code, None).expect("Failed to parse HTML code");

    let mut extractor = HTMLExtractor::new(
        "html".to_string(),
        "test.html".to_string(),
        code.to_string(),
    );
    extractor.extract_symbols(&tree)
}

pub mod forms;
pub mod identifier_extraction;
pub mod media;
pub mod structure;
