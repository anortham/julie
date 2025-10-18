use crate::extractors::base::{Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

use super::SwiftExtractor;

/// Extracts identifier usages and definitions for LSP-quality find_references support
impl SwiftExtractor {
    /// Extract all identifier usages (function calls, member access, etc.)
    /// Following the Rust extractor reference implementation pattern
    pub fn extract_identifiers(
        &mut self,
        tree: &tree_sitter::Tree,
        symbols: &[Symbol],
    ) -> Vec<Identifier> {
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
                // Swift call_expression has the function as a child
                // For simple calls: identifier is direct child
                // For member calls: navigation_expression is child, then we get rightmost identifier
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "simple_identifier" {
                        let name = self.base.get_node_text(&child);
                        let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                        self.base.create_identifier(
                            &child,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                        break;
                    } else if child.kind() == "navigation_expression" {
                        // For member access calls, extract the rightmost identifier (the method name)
                        if let Some((name_node, name)) = self.extract_rightmost_identifier(&child) {
                            let containing_symbol_id =
                                self.find_containing_symbol_id(node, symbol_map);

                            self.base.create_identifier(
                                &name_node,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                        }
                        break;
                    }
                }
            }

            // Member access: object.property
            "navigation_expression" => {
                // Only extract if it's NOT part of a call_expression
                // (we handle those in the call_expression case above)
                if let Some(parent) = node.parent() {
                    if parent.kind() == "call_expression" {
                        return; // Skip - handled by call_expression
                    }
                }

                // Extract the rightmost identifier (the member name)
                if let Some((name_node, name)) = self.extract_rightmost_identifier(&node) {
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &name_node,
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

    /// Helper to extract the rightmost identifier in a navigation_expression
    /// Returns both the node and the extracted text to avoid lifetime issues
    fn extract_rightmost_identifier<'a>(&self, node: &Node<'a>) -> Option<(Node<'a>, String)> {
        // Swift navigation_expression structure: target.suffix
        // For chained access like user.account.balance:
        //   - Outermost: target=(user.account navigation_expression), suffix=balance
        //   - We always want the suffix of the CURRENT (outermost) node

        // Get the suffix (navigation_suffix node) from the CURRENT node
        // This handles chained access correctly by always taking the rightmost part
        if let Some(suffix_node) = node.child_by_field_name("suffix") {
            if suffix_node.kind() == "navigation_suffix" {
                // Get the suffix field of navigation_suffix (the identifier)
                if let Some(identifier_node) = suffix_node.child_by_field_name("suffix") {
                    if identifier_node.kind() == "simple_identifier" {
                        let name = self.base.get_node_text(&identifier_node);
                        return Some((identifier_node, name));
                    }
                }
            }
        }

        // Fallback: search for simple_identifier children (for backwards compatibility)
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "simple_identifier" {
                let name = self.base.get_node_text(&child);
                return Some((child, name));
            }
        }

        None
    }
}
