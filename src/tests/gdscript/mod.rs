use crate::extractors::base::{Relationship, Symbol};
use crate::extractors::gdscript::GDScriptExtractor;
use crate::tests::test_utils::init_parser;

pub fn extract_symbols(code: &str) -> Vec<Symbol> {
    let tree = init_parser(code, "gdscript");
    let mut extractor = GDScriptExtractor::new(
        "gdscript".to_string(),
        "test.gd".to_string(),
        code.to_string(),
    );
    extractor.extract_symbols(&tree)
}

pub fn extract_symbols_and_relationships(code: &str) -> (Vec<Symbol>, Vec<Relationship>) {
    let tree = init_parser(code, "gdscript");
    let mut extractor = GDScriptExtractor::new(
        "gdscript".to_string(),
        "test.gd".to_string(),
        code.to_string(),
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);
    (symbols, relationships)
}

pub mod classes;
pub mod functions;
pub mod identifier_extraction;
pub mod modern;
pub mod patterns;
pub mod resources;
pub mod scenes;
pub mod signals;
pub mod ui;
