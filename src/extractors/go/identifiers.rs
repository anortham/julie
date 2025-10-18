use crate::extractors::base::{IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

/// Identifier extraction for LSP-quality find_references
impl super::GoExtractor {
    /// Extract all identifier usages (function calls, member access, etc.)
    /// Following the Rust extractor reference implementation pattern
    pub(super) fn walk_tree_for_identifiers(
        &mut self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        // Extract identifier from this node if applicable
        self.extract_identifier_from_node(node, symbol_map);

        // Recursively walk children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_identifiers(child, symbol_map);
        }
    }

    /// Extract identifier from a single node based on its kind
    pub(super) fn extract_identifier_from_node(
        &mut self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        match node.kind() {
            // Function/method calls: foo(), bar.Baz()
            "call_expression" => {
                // The function being called is typically the first child or in a selector
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "identifier" => {
                            // Simple function call: foo()
                            let name = self.base.get_node_text(&child);
                            let containing_symbol_id =
                                self.find_containing_symbol_id(node, symbol_map);

                            self.base.create_identifier(
                                &child,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                            break;
                        }
                        "selector_expression" => {
                            // Method call: obj.Method()
                            // Extract the rightmost identifier (the method name)
                            if let Some(field_node) = child.child_by_field_name("field") {
                                let name = self.base.get_node_text(&field_node);
                                let containing_symbol_id =
                                    self.find_containing_symbol_id(node, symbol_map);

                                self.base.create_identifier(
                                    &field_node,
                                    name,
                                    IdentifierKind::Call,
                                    containing_symbol_id,
                                );
                            }
                            break;
                        }
                        _ => {}
                    }
                }
            }

            // Member access: object.Field
            "selector_expression" => {
                // Only extract if it's NOT part of a call_expression
                // (we handle those in the call_expression case above)
                if let Some(parent) = node.parent() {
                    if parent.kind() == "call_expression" {
                        return; // Skip - handled by call_expression
                    }
                }

                // Extract the rightmost identifier (the field name)
                if let Some(field_node) = node.child_by_field_name("field") {
                    let name = self.base.get_node_text(&field_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &field_node,
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
    pub(super) fn find_containing_symbol_id(
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
