// Lua Extractor Tests - modularized structure

// Submodule declarations
pub mod core;
pub mod classes;
pub mod extractor;
pub mod functions;
pub mod variables;
pub mod identifiers;
pub mod helpers;
pub mod tables;
pub mod relationships;

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
pub mod coroutines;
pub mod error_handling;
pub mod file_operations;
pub mod identifier_extraction;
pub mod metatables;
pub mod modules;
pub mod oop_patterns;
pub mod strings;
