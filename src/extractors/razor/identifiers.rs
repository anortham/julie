/// LSP-quality identifier extraction for find_references support
use crate::extractors::base::{Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

impl super::RazorExtractor {
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
    /// Razor-specific: handles C# code within Razor directives and code blocks
    fn extract_identifier_from_node(&mut self, node: Node, symbol_map: &HashMap<String, &Symbol>) {
        match node.kind() {
            // Function/method calls: foo(), bar.Baz()
            // These appear in C# code blocks within Razor (@code {}, @{}, etc.)
            "invocation_expression" => {
                // The name is typically a child of the invocation_expression
                // Look for identifier or member_access_expression
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        let name = self.base.get_node_text(&child);
                        let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                        self.base.create_identifier(
                            &child,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                        break;
                    } else if child.kind() == "member_access_expression" {
                        // For member access, extract the rightmost identifier (the method name)
                        if let Some(name_node) = child.child_by_field_name("name") {
                            let name = self.base.get_node_text(&name_node);
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

            // Member access: object.field
            // These appear in C# code blocks and Razor expressions
            "member_access_expression" => {
                // Only extract if it's NOT part of an invocation_expression
                // (we handle those in the invocation_expression case above)
                if let Some(parent) = node.parent() {
                    if parent.kind() == "invocation_expression" {
                        return; // Skip - handled by invocation_expression
                    }
                }

                // Extract the rightmost identifier (the member name)
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = self.base.get_node_text(&name_node);
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
}
