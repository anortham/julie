// CSS Extractor Identifiers - Extract identifier usages (function calls, classes, IDs)

use crate::extractors::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

pub(super) struct IdentifierExtractor;

impl IdentifierExtractor {
    /// Extract all identifier usages (CSS functions, class/id selectors)
    pub(super) fn extract_identifiers(
        base: &mut BaseExtractor,
        tree: &Tree,
        symbols: &[Symbol],
    ) -> Vec<Identifier> {
        // Create symbol map for fast lookup
        let symbol_map: HashMap<String, &Symbol> =
            symbols.iter().map(|s| (s.id.clone(), s)).collect();

        // Walk the tree and extract identifiers
        Self::walk_tree_for_identifiers(base, tree.root_node(), &symbol_map);

        // Return the collected identifiers
        base.identifiers.clone()
    }

    /// Recursively walk tree extracting identifiers from each node
    fn walk_tree_for_identifiers(
        base: &mut BaseExtractor,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        // Extract identifier from this node if applicable
        Self::extract_identifier_from_node(base, node, symbol_map);

        // Recursively walk children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::walk_tree_for_identifiers(base, child, symbol_map);
        }
    }

    /// Extract identifier from a single node based on its kind
    fn extract_identifier_from_node(
        base: &mut BaseExtractor,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        match node.kind() {
            // CSS function calls: calc(), var(), rgb(), etc.
            "call_expression" => {
                // Extract function name
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "function_name" {
                        let name = base.get_node_text(&child);
                        let containing_symbol_id =
                            Self::find_containing_symbol_id(base, node, symbol_map);

                        base.create_identifier(
                            &child,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                        break;
                    }
                }
            }

            // Class selectors: .button, .nav-item (treated as member access for HTML tracking)
            "class_selector" => {
                let text = base.get_node_text(&node);
                // Remove the leading dot from class name
                let class_name = text.strip_prefix('.').unwrap_or(&text);

                if !class_name.is_empty() {
                    let containing_symbol_id =
                        Self::find_containing_symbol_id(base, node, symbol_map);

                    base.create_identifier(
                        &node,
                        class_name.to_string(),
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                }
            }

            // ID selectors: #header, #main-content (treated as member access for HTML tracking)
            "id_selector" => {
                let text = base.get_node_text(&node);
                // Remove the leading hash from ID name
                let id_name = text.strip_prefix('#').unwrap_or(&text);

                if !id_name.is_empty() {
                    let containing_symbol_id =
                        Self::find_containing_symbol_id(base, node, symbol_map);

                    base.create_identifier(
                        &node,
                        id_name.to_string(),
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                }
            }

            _ => {
                // Skip other node types for now
                // Future: pseudo_class_selector, attribute_selector, etc.
            }
        }
    }

    /// Find the ID of the symbol that contains this node
    /// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
    fn find_containing_symbol_id(
        base: &BaseExtractor,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) -> Option<String> {
        // CRITICAL FIX: Only search symbols from THIS FILE, not all files
        // Bug was: searching all symbols in DB caused wrong file symbols to match
        let file_symbols: Vec<Symbol> = symbol_map
            .values()
            .filter(|s| s.file_path == base.file_path)
            .map(|&s| s.clone())
            .collect();

        base.find_containing_symbol(&node, &file_symbols)
            .map(|s| s.id.clone())
    }
}
