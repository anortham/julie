// HTML Extractor
//
// Implementation of HTML extractor to idiomatic Rust

use crate::extractors::base::{BaseExtractor, Identifier, Relationship, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

// Private modules
mod attributes;
mod elements;
mod fallback;
mod helpers;
mod identifiers;
mod relationships;
mod scripts;
mod types;

// Public re-exports
pub use crate::extractors::base::{IdentifierKind, RelationshipKind};

pub struct HTMLExtractor {
    base: BaseExtractor,
}

impl HTMLExtractor {
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

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();

        // Check if tree is valid and has a root node - start from actual root standard format
        let root_node = tree.root_node();
        if root_node.child_count() > 0 {
            self.visit_node(root_node, &mut symbols, None);
        } else {
            // Fallback extraction when normal parsing fails
            return fallback::FallbackExtractor::extract_basic_structure(&mut self.base, tree);
        }

        // If we only extracted error symbols, try basic structure fallback
        let has_only_errors = !symbols.is_empty()
            && symbols.iter().all(|s| {
                s.metadata
                    .as_ref()
                    .and_then(|m| m.get("isError"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            });

        if has_only_errors || symbols.is_empty() {
            fallback::FallbackExtractor::extract_basic_structure(&mut self.base, tree)
        } else {
            symbols
        }
    }

    fn visit_node(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<&str>) {
        if let Some(symbol) = self.extract_node_symbol(node, parent_id) {
            let symbol_id = symbol.id.clone();
            symbols.push(symbol);

            // Recursively visit children with the new symbol as parent
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.visit_node(child, symbols, Some(&symbol_id));
            }
        } else {
            // If no symbol was extracted, continue with children using current parent
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.visit_node(child, symbols, parent_id);
            }
        }
    }

    fn extract_node_symbol(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        match node.kind() {
            "element" => Some(elements::ElementExtractor::extract_element(
                &mut self.base,
                node,
                parent_id,
            )),
            "script_element" => Some(scripts::ScriptStyleExtractor::extract_script_element(
                &mut self.base,
                node,
                parent_id,
            )),
            "style_element" => Some(scripts::ScriptStyleExtractor::extract_style_element(
                &mut self.base,
                node,
                parent_id,
            )),
            "doctype" => Some(elements::ElementExtractor::extract_doctype(
                &mut self.base,
                node,
                parent_id,
            )),
            "comment" => {
                elements::ElementExtractor::extract_comment(&mut self.base, node, parent_id)
            }
            _ => None,
        }
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();

        if let Some(root_node) = tree.root_node().child(0) {
            self.visit_node_for_relationships(root_node, symbols, &mut relationships);
        }

        relationships
    }

    fn visit_node_for_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        match node.kind() {
            "element" => {
                relationships::RelationshipExtractor::extract_element_relationships(
                    &self.base,
                    node,
                    symbols,
                    relationships,
                );
            }
            "script_element" => {
                relationships::RelationshipExtractor::extract_script_relationships(
                    &self.base,
                    node,
                    symbols,
                    relationships,
                );
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node_for_relationships(child, symbols, relationships);
        }
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        types::HTMLTypes::infer_types(symbols)
    }

    /// Extract all identifier usages (event handlers, id/class references)
    /// Following the Rust extractor reference implementation pattern
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        // Create symbol map for fast lookup
        let symbol_map: HashMap<String, &Symbol> =
            symbols.iter().map(|s| (s.id.clone(), s)).collect();

        // Walk the tree and extract identifiers
        identifiers::IdentifierExtractor::extract_identifiers(
            &mut self.base,
            tree.root_node(),
            &symbol_map,
        );

        // Return the collected identifiers
        self.base.identifiers.clone()
    }
}
