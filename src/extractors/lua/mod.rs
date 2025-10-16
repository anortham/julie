/// Lua Extractor Implementation
///
/// Port of Miller's Lua extractor with idiomatic Rust patterns and modular architecture.
/// Original: /Users/murphy/Source/miller/src/extractors/lua-extractor.ts
///
/// This module is organized into focused sub-modules:
/// - core: Symbol extraction and traversal orchestration
/// - functions: Function and method definition extraction
/// - variables: Local and global variable extraction
/// - tables: Table field extraction and handling
/// - classes: Lua class pattern detection (tables with metatables)
/// - identifiers: LSP identifier tracking for references
/// - helpers: Type inference and utility functions

mod classes;
mod core;
mod functions;
mod helpers;
mod identifiers;
mod tables;
mod variables;

use crate::extractors::base::{
    BaseExtractor, Identifier, Relationship, Symbol,
};
use tree_sitter::Tree;

pub struct LuaExtractor {
    base: BaseExtractor,
    symbols: Vec<Symbol>,
    relationships: Vec<Relationship>,
}

impl LuaExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
            symbols: Vec::new(),
            relationships: Vec::new(),
        }
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        self.symbols.clear();
        self.relationships.clear();

        // Use core module to traverse and extract symbols
        core::traverse_tree(&mut self.symbols, &mut self.base, tree.root_node(), None);

        // Post-process to detect Lua class patterns
        classes::detect_lua_classes(&mut self.symbols);

        self.symbols.clone()
    }

    pub fn extract_relationships(
        &mut self,
        _tree: &Tree,
        _symbols: &[Symbol],
    ) -> Vec<Relationship> {
        self.relationships.clone()
    }

    /// Extract all identifier usages (function calls, member access, etc.)
    /// Following the Rust extractor reference implementation pattern
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(self, tree, symbols)
    }

    // ========================================================================
    // Accessors for sub-modules
    // ========================================================================

    pub(super) fn base(&self) -> &BaseExtractor {
        &self.base
    }

    pub(super) fn base_mut(&mut self) -> &mut BaseExtractor {
        &mut self.base
    }


}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lua_extractor_initialization() {
        let extractor = LuaExtractor::new(
            "lua".to_string(),
            "test.lua".to_string(),
            "function hello() end".to_string(),
        );
        assert_eq!(extractor.base.file_path, "test.lua");
    }
}
