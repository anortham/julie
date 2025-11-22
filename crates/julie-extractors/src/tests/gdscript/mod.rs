use crate::base::{Relationship, Symbol};
use crate::gdscript::GDScriptExtractor;
use crate::tests::test_utils::init_parser;
use std::path::PathBuf;

pub fn extract_symbols(code: &str) -> Vec<Symbol> {
    let tree = init_parser(code, "gdscript");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = GDScriptExtractor::new(
        "gdscript".to_string(),
        "test.gd".to_string(),
        code.to_string(),
        &workspace_root,
    );
    extractor.extract_symbols(&tree)
}

pub fn extract_symbols_and_relationships(code: &str) -> (Vec<Symbol>, Vec<Relationship>) {
    let tree = init_parser(code, "gdscript");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = GDScriptExtractor::new(
        "gdscript".to_string(),
        "test.gd".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);
    (symbols, relationships)
}

pub mod classes;
pub mod cross_file_relationships;
pub mod functions;
pub mod identifier_extraction;
pub mod modern;
pub mod patterns;
pub mod resources;
pub mod scenes;
pub mod signals;
pub mod ui;
