use crate::extractors::base::{Relationship, Symbol};
use crate::extractors::sql::SqlExtractor;
use tree_sitter::Parser;

pub use crate::extractors::base::{RelationshipKind, SymbolKind};

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_sequel::LANGUAGE.into())
        .expect("Error loading SQL grammar");
    parser
}

pub fn extract_symbols(code: &str) -> Vec<Symbol> {
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let mut extractor =
        SqlExtractor::new("sql".to_string(), "test.sql".to_string(), code.to_string());
    extractor.extract_symbols(&tree)
}

pub fn extract_symbols_and_relationships(code: &str) -> (Vec<Symbol>, Vec<Relationship>) {
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let mut extractor =
        SqlExtractor::new("sql".to_string(), "test.sql".to_string(), code.to_string());
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);
    (symbols, relationships)
}

pub mod ddl;
pub mod dml;
pub mod doc_comments;
pub mod identifier_extraction;
pub mod indexes;
pub mod procedures;
pub mod relationships;
pub mod schema;
pub mod security;
pub mod transactions;
