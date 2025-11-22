// Lua Extractor Tests - modularized structure

// Submodule declarations
pub mod classes;
pub mod core;
pub mod cross_file_relationships;
pub mod doc_comments;
pub mod extractor;
pub mod functions;
pub mod helpers;
pub mod identifiers;
pub mod relationships;
pub mod tables;
pub mod variables;

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
