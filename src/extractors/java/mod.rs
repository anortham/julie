/// Java extractor for extracting symbols and relationships from Java source code
/// Implementation of Java extractor with comprehensive Java feature support
///
/// This module is organized into focused sub-modules:
/// - helpers: Shared utility functions (modifiers, visibility, type parsing)
/// - classes: Class, interface, enum, record extraction
/// - methods: Method and constructor extraction
/// - fields: Field and property extraction
/// - annotations: Annotation extraction
/// - imports_packages: Import and package declaration extraction
/// - relationships: Inheritance and implementation relationship extraction
/// - types: Type inference from signatures
/// - identifiers: LSP identifier tracking for references
mod annotations;
mod classes;
mod fields;
mod helpers;
mod identifiers;
mod imports_packages;
mod methods;
mod relationships;
mod types;

use crate::extractors::base::{BaseExtractor, Identifier, Relationship, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Java extractor for extracting symbols and relationships from Java source code
pub struct JavaExtractor {
    base: BaseExtractor,
}

impl JavaExtractor {
    pub fn new(
        language: String,
        file_path: String,
        content: String,
        workspace_root: &std::path::Path,
    ) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content, workspace_root),
        }
    }

    /// Extract all symbols from Java source code
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.walk_tree(tree.root_node(), &mut symbols, None);
        symbols
    }

    fn walk_tree(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<&str>) {
        if let Some(symbol) = self.extract_symbol(node, parent_id) {
            let symbol_id = symbol.id.clone();
            symbols.push(symbol);

            // Walk children with this symbol as parent
            for child in node.children(&mut node.walk()) {
                self.walk_tree(child, symbols, Some(&symbol_id));
            }
        } else {
            // Walk children with the same parent
            for child in node.children(&mut node.walk()) {
                self.walk_tree(child, symbols, parent_id);
            }
        }
    }

    fn extract_symbol(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        match node.kind() {
            "package_declaration" => imports_packages::extract_package(self, node, parent_id),
            "import_declaration" => imports_packages::extract_import(self, node, parent_id),
            "class_declaration" => classes::extract_class(self, node, parent_id),
            "interface_declaration" => classes::extract_interface(self, node, parent_id),
            "method_declaration" => methods::extract_method(self, node, parent_id),
            "constructor_declaration" => methods::extract_constructor(self, node, parent_id),
            "field_declaration" => fields::extract_field(self, node, parent_id),
            "enum_declaration" => classes::extract_enum(self, node, parent_id),
            "enum_constant" => classes::extract_enum_constant(self, node, parent_id),
            "annotation_type_declaration" => annotations::extract_annotation(self, node, parent_id),
            "record_declaration" => classes::extract_record(self, node, parent_id),
            _ => None,
        }
    }

    /// Extract relationships from Java code
    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.visit_node_for_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    fn visit_node_for_relationships(
        &mut self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        match node.kind() {
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration" => {
                relationships::extract_inheritance_relationships(
                    self,
                    node,
                    symbols,
                    relationships,
                );
            }
            _ => {}
        }

        for child in node.children(&mut node.walk()) {
            self.visit_node_for_relationships(child, symbols, relationships);
        }
    }

    /// Infer types from Java type signatures
    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        types::infer_types(self, symbols)
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
