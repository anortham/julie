use crate::base::{Symbol, SymbolKind, Visibility};
use crate::vbnet::VbNetExtractor;
use tree_sitter::Parser;

pub fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_vb_dotnet::LANGUAGE.into())
        .expect("Error loading VB.NET grammar");
    parser
}

pub mod core;
pub mod members;
