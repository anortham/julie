// PHP Extractor for Julie - Modular structure
// Main orchestrator and public API

mod functions;
mod helpers;
mod identifiers;
mod members;
mod namespaces;
mod relationships;
mod types;

use crate::extractors::base::{BaseExtractor, Identifier, Relationship, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

// Import functions for use in this module
use functions::extract_function;
use helpers::{determine_visibility, extract_modifiers, find_child, find_child_text};
use identifiers::extract_identifier_from_node;
use members::{extract_constant, extract_property};
use namespaces::{extract_namespace, extract_use, extract_variable_assignment};
use relationships::{extract_class_relationships, extract_interface_relationships};
use types::{extract_class, extract_enum, extract_enum_case, extract_interface, extract_trait};

pub struct PhpExtractor {
    base: BaseExtractor,
}

impl PhpExtractor {
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

    /// Extract symbols from PHP code - main extraction method
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.visit_node(tree.root_node(), &mut symbols, None);
        symbols
    }

    /// Extract relationships from PHP code - relationship extraction
    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.visit_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    /// Infer types from PHP type declarations - type inference
    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();
        for symbol in symbols {
            let metadata = &symbol.metadata;
            if let Some(return_type) = metadata.as_ref().and_then(|m| m.get("returnType")) {
                if let Some(type_str) = return_type.as_str() {
                    types.insert(symbol.id.clone(), type_str.to_string());
                }
            } else if let Some(property_type) =
                metadata.as_ref().and_then(|m| m.get("propertyType"))
            {
                if let Some(type_str) = property_type.as_str() {
                    types.insert(symbol.id.clone(), type_str.to_string());
                }
            } else if let Some(type_val) = metadata.as_ref().and_then(|m| m.get("type")) {
                if let Some(type_str) = type_val.as_str() {
                    if !matches!(type_str, "function" | "property") {
                        types.insert(symbol.id.clone(), type_str.to_string());
                    }
                }
            }
        }
        types
    }

    /// Extract all identifier usages (function calls, member access, etc.)
    /// Following the Rust extractor reference implementation pattern
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        // Create symbol map for fast lookup
        let symbol_map: HashMap<String, &Symbol> =
            symbols.iter().map(|s| (s.id.clone(), s)).collect();

        // Walk the tree and extract identifiers
        self.walk_tree_for_identifiers(tree.root_node(), &symbol_map);

        // Return the collected identifiers
        self.base.identifiers.clone()
    }

    /// Recursive node visitor following visitNode pattern
    fn visit_node(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        if node.kind().is_empty() {
            return; // Skip invalid nodes
        }

        let mut current_parent_id = parent_id.clone();
        let symbol = match node.kind() {
            "class_declaration" => Some(extract_class(self, node, parent_id.as_deref())),
            "interface_declaration" => Some(extract_interface(self, node, parent_id.as_deref())),
            "trait_declaration" => Some(extract_trait(self, node, parent_id.as_deref())),
            "enum_declaration" => Some(extract_enum(self, node, parent_id.as_deref())),
            "function_definition" | "method_declaration" => {
                Some(extract_function(self, node, parent_id.as_deref()))
            }
            "property_declaration" => extract_property(self, node, parent_id.as_deref()),
            "const_declaration" => extract_constant(self, node, parent_id.as_deref()),
            "namespace_definition" => Some(extract_namespace(self, node, parent_id.as_deref())),
            "use_declaration" | "namespace_use_declaration" => {
                Some(extract_use(self, node, parent_id.as_deref()))
            }
            "enum_case" => extract_enum_case(self, node, parent_id.as_deref()),
            "assignment_expression" => {
                extract_variable_assignment(self, node, parent_id.as_deref())
            }
            _ => None,
        };

        if let Some(sym) = symbol {
            current_parent_id = Some(sym.id.clone());
            symbols.push(sym);
        }

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, current_parent_id.clone());
        }
    }

    /// Visit nodes for relationship extraction
    fn visit_relationships(
        &mut self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        match node.kind() {
            "class_declaration" => {
                extract_class_relationships(self, node, symbols, relationships);
            }
            "interface_declaration" => {
                extract_interface_relationships(self, node, symbols, relationships);
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_relationships(child, symbols, relationships);
        }
    }

    /// Recursively walk tree extracting identifiers from each node
    fn walk_tree_for_identifiers(&mut self, node: Node, symbol_map: &HashMap<String, &Symbol>) {
        // Extract identifier from this node if applicable
        extract_identifier_from_node(self, node, symbol_map);

        // Recursively walk children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_identifiers(child, symbol_map);
        }
    }

    // Expose internal methods that submodules need
    pub(super) fn get_base(&self) -> &BaseExtractor {
        &self.base
    }

    pub(super) fn get_base_mut(&mut self) -> &mut BaseExtractor {
        &mut self.base
    }
}
