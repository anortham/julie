// Lua Extractor Tests - modularized structure

use crate::extractors::base::{SymbolKind, Visibility};
use crate::extractors::lua::LuaExtractor;
use tree_sitter::Parser;

pub fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_lua::LANGUAGE.into())
        .expect("Error loading Lua grammar");
    parser
}

pub mod control_flow;
pub mod core;
pub mod coroutines;
pub mod metatables;
pub mod modules;
pub mod strings;
pub mod tables;
