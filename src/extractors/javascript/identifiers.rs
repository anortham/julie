//! Identifier extraction for JavaScript
//!
//! Handles extraction of all identifier usages including function calls,
//! member access, and other references used for LSP-quality find_references.

use crate::extractors::base::{Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

impl super::JavaScriptExtractor {
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

    /// Recursively walk tree extracting identifiers from each node
    fn walk_tree_for_identifiers(&mut self, node: Node, symbol_map: &HashMap<String, &Symbol>) {
        // Extract identifier from this node if applicable
        self.extract_identifier_from_node(node, symbol_map);

        // Recursively walk children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_identifiers(child, symbol_map);
        }
    }

    /// Extract identifier from a single node based on its kind
    fn extract_identifier_from_node(&mut self, node: Node, symbol_map: &HashMap<String, &Symbol>) {
        match node.kind() {
            // Function/method calls: foo(), bar.baz()
            "call_expression" => {
                // The function being called is in the "function" field
                if let Some(function_node) = node.child_by_field_name("function") {
                    match function_node.kind() {
                        "identifier" => {
                            // Simple function call: foo()
                            let name = self.base.get_node_text(&function_node);
                            let containing_symbol_id =
                                self.find_containing_symbol_id(node, symbol_map);

                            self.base.create_identifier(
                                &function_node,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                        }
                        "member_expression" => {
                            // Member call: object.method()
                            // Extract the rightmost identifier (the method name)
                            if let Some(property_node) =
                                function_node.child_by_field_name("property")
                            {
                                let name = self.base.get_node_text(&property_node);
                                let containing_symbol_id =
                                    self.find_containing_symbol_id(node, symbol_map);

                                self.base.create_identifier(
                                    &property_node,
                                    name,
                                    IdentifierKind::Call,
                                    containing_symbol_id,
                                );
                            }
                        }
                        _ => {
                            // Other cases like computed member expressions
                            // Skip for now
                        }
                    }
                }
            }

            // Member access: object.property
            "member_expression" => {
                // Only extract if it's NOT part of a call_expression
                // (we handle those in the call_expression case above)
                if let Some(parent) = node.parent() {
                    if parent.kind() == "call_expression" {
                        // Check if this member_expression is the function being called
                        if let Some(function_node) = parent.child_by_field_name("function") {
                            if function_node.id() == node.id() {
                                return; // Skip - handled by call_expression
                            }
                        }
                    }
                }

                // Extract the rightmost identifier (the property name)
                if let Some(property_node) = node.child_by_field_name("property") {
                    let name = self.base.get_node_text(&property_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &property_node,
                        name,
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                }
            }

            _ => {
                // Skip other node types for now
                // Future: type usage, constructor calls, etc.
            }
        }
    }

    /// Find the ID of the symbol that contains this node
    /// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
    fn find_containing_symbol_id(
        &self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) -> Option<String> {
        // CRITICAL FIX: Only search symbols from THIS FILE, not all files
        // Bug was: searching all symbols in DB caused wrong file symbols to match
        let file_symbols: Vec<Symbol> = symbol_map
            .values()
            .filter(|s| s.file_path == self.base.file_path)
            .map(|&s| s.clone())
            .collect();

        self.base
            .find_containing_symbol(&node, &file_symbols)
            .map(|s| s.id.clone())
    }
}
