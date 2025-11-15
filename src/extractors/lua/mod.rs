/// Lua Extractor Implementation
///
/// Implementation of Lua extractor with idiomatic Rust patterns and modular architecture.
///
/// This module is organized into focused sub-modules:
/// - core: Symbol extraction and traversal orchestration
/// - functions: Function and method definition extraction
/// - variables: Local and global variable extraction
/// - tables: Table field extraction and handling
/// - classes: Lua class pattern detection (tables with metatables)
/// - identifiers: LSP identifier tracking for references
/// - helpers: Type inference and utility functions
pub(crate) mod classes;
mod core;
mod functions;
pub(crate) mod helpers;
mod identifiers;
mod relationships;
mod tables;
mod variables;

use crate::extractors::base::{BaseExtractor, Identifier, Relationship, Symbol};
use tree_sitter::Tree;

pub struct LuaExtractor {
    base: BaseExtractor,
    symbols: Vec<Symbol>,
    relationships: Vec<Relationship>,
}

impl LuaExtractor {
    pub fn new(
        language: String,
        file_path: String,
        content: String,
        workspace_root: &std::path::Path,
    ) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content, workspace_root),
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

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        self.relationships.clear();
        relationships::extract_relationships(&mut self.relationships, &self.base, tree, symbols);
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

    pub(crate) fn base(&self) -> &BaseExtractor {
        &self.base
    }

    pub(crate) fn base_mut(&mut self) -> &mut BaseExtractor {
        &mut self.base
    }
}
