//! C++ extractor - Implementation of comprehensive C++ extraction logic
//! Modularized architecture with clear separation of concerns:
//! - helpers: Template parameters, base classes, node utilities
//! - types: Class/struct/union/enum extraction
//! - functions: Function/method/constructor/destructor extraction
//! - declarations: Variable/field/friend declarations
//! - relationships: Inheritance and identifier usage tracking
//! - type_inference: Return type and variable type inference

mod declarations;
mod functions;
mod helpers;
mod identifiers;
mod relationships;
mod type_inference;
mod types;

use crate::extractors::base::{
    BaseExtractor, Relationship, Symbol, SymbolKind, SymbolOptions, Visibility,
};
use std::collections::{HashMap, HashSet};
use tree_sitter::{Node, Tree};

/// C++ extractor for extracting symbols and relationships from C++ source code
/// Direct Implementation of CppExtractor with all advanced C++ features
pub struct CppExtractor {
    base: BaseExtractor,
    processed_nodes: HashSet<String>,
    additional_symbols: Vec<Symbol>,
}

impl CppExtractor {
    pub fn new(file_path: String, content: String, workspace_root: &std::path::Path) -> Self {
        Self {
            base: BaseExtractor::new("cpp".to_string(), file_path, content, workspace_root),
            processed_nodes: HashSet::new(),
            additional_symbols: Vec::new(),
        }
    }

    /// Extract all symbols from C++ source code
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.processed_nodes.clear();
        self.additional_symbols.clear();

        self.walk_tree(tree.root_node(), &mut symbols, None);

        // Add any additional symbols collected from ERROR nodes
        symbols.extend(self.additional_symbols.clone());

        symbols
    }

    /// Extract relationships from C++ code
    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        relationships::extract_relationships(&mut self.base, tree, symbols)
    }

    /// Extract identifiers (function calls, member access, etc.)
    pub fn extract_identifiers(
        &mut self,
        tree: &Tree,
        symbols: &[Symbol],
    ) -> Vec<crate::extractors::base::Identifier> {
        // Create symbol map for fast lookup
        let symbol_map: HashMap<String, &Symbol> =
            symbols.iter().map(|s| (s.id.clone(), s)).collect();

        // Walk the tree and extract identifiers
        self.walk_tree_for_identifiers(tree.root_node(), &symbol_map);

        // Return the collected identifiers from the base extractor
        self.base.identifiers.clone()
    }

    /// Infer types from C++ type annotations and declarations
    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        type_inference::infer_types(symbols)
    }

    // ========================================================================
    // Private methods: Tree walking and symbol extraction
    // ========================================================================

    /// Walk the tree recursively
    fn walk_tree(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        // Handle field_declaration specially (can produce multiple symbols)
        if node.kind() == "field_declaration" {
            let field_symbols =
                declarations::extract_field(&mut self.base, node, parent_id.as_deref());
            if !field_symbols.is_empty() {
                symbols.extend(field_symbols);
                // Don't walk children - field declarations are leaf nodes
                return;
            }
            // If extract_field returned empty, this might be a method declaration
            // Fall through to normal handling
        }

        // Extract symbol from current node
        if let Some(symbol) = self.extract_symbol(node, parent_id.as_deref()) {
            let current_parent_id = Some(symbol.id.clone());
            symbols.push(symbol);

            // Continue with children using this symbol as parent
            self.walk_children(node, symbols, current_parent_id);
        } else {
            // No symbol extracted, continue with same parent
            self.walk_children(node, symbols, parent_id);
        }
    }

    fn walk_children(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree(child, symbols, parent_id.clone());
        }
    }

    /// Generate unique key for node
    fn get_node_key(&self, node: Node) -> String {
        format!(
            "{}:{}:{}:{}:{}",
            node.start_position().row,
            node.start_position().column,
            node.end_position().row,
            node.end_position().column,
            node.kind()
        )
    }

    /// Extract symbol from a single node
    fn extract_symbol(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let node_key = self.get_node_key(node);

        // Track specific node types to prevent duplicates
        let should_track = matches!(
            node.kind(),
            "function_declarator" | "function_definition" | "declaration" | "class_specifier"
        );

        if should_track && self.processed_nodes.contains(&node_key) {
            return None;
        }

        let symbol = match node.kind() {
            "namespace_definition" => {
                declarations::extract_namespace(&mut self.base, node, parent_id)
            }
            "using_declaration" | "namespace_alias_definition" => {
                declarations::extract_using(&mut self.base, node, parent_id)
            }
            "class_specifier" => types::extract_class(&mut self.base, node, parent_id),
            "struct_specifier" => types::extract_struct(&mut self.base, node, parent_id),
            "union_specifier" => types::extract_union(&mut self.base, node, parent_id),
            "enum_specifier" => types::extract_enum(&mut self.base, node, parent_id),
            "enumerator" => types::extract_enum_member(&mut self.base, node, parent_id),
            "function_definition" => functions::extract_function(&mut self.base, node, parent_id),
            "function_declarator" => {
                // Only extract standalone function declarators
                if node.parent().map(|p| p.kind()) != Some("function_definition") {
                    functions::extract_function(&mut self.base, node, parent_id)
                } else {
                    None
                }
            }
            "declaration" => declarations::extract_declaration(&mut self.base, node, parent_id),
            "field_declaration" => {
                // Field declarations with function_declarators (method declarations) fall through here
                declarations::extract_declaration(&mut self.base, node, parent_id)
            }
            "friend_declaration" => {
                declarations::extract_friend_declaration(&mut self.base, node, parent_id)
            }
            "template_declaration" => {
                declarations::extract_template(&mut self.base, node, parent_id)
            }
            "ERROR" => self.extract_from_error_node(node, parent_id),
            _ => None,
        };

        // Mark node as processed if we successfully extracted a symbol and should track
        if symbol.is_some() && should_track {
            self.processed_nodes.insert(node_key);
        }

        symbol
    }

    /// Extract from ERROR node - handle malformed code gracefully
    fn extract_from_error_node(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        // Look for class/struct patterns: "class" + type_identifier
        for i in 0..children.len().saturating_sub(1) {
            let current = children[i];
            let next = children[i + 1];

            if current.kind() == "class" && next.kind() == "type_identifier" {
                let name = self.base.get_node_text(&next);
                let signature = format!("class {}", name);

                let doc_comment = self.base.find_doc_comment(&node);

                return Some(self.base.create_symbol(
                    &node,
                    name,
                    SymbolKind::Class,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(Visibility::Public),
                        parent_id: parent_id.map(String::from),
                        metadata: None,
                        doc_comment,
                    },
                ));
            } else if current.kind() == "struct" && next.kind() == "type_identifier" {
                let name = self.base.get_node_text(&next);
                let signature = format!("struct {}", name);

                let doc_comment = self.base.find_doc_comment(&node);

                return Some(self.base.create_symbol(
                    &node,
                    name,
                    SymbolKind::Struct,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(Visibility::Public),
                        parent_id: parent_id.map(String::from),
                        metadata: None,
                        doc_comment,
                    },
                ));
            }
        }

        // No reconstructible symbol found
        None
    }
}
