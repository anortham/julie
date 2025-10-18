use crate::extractors::base::{BaseExtractor, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

/// Identifier extraction for LSP find_references functionality
pub(super) struct IdentifierExtractor;

impl IdentifierExtractor {
    /// Extract all identifier usages from HTML tree
    pub(super) fn extract_identifiers(
        base: &mut BaseExtractor,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        // Extract identifier from this node if applicable
        Self::extract_identifier_from_node(base, node, symbol_map);

        // Recursively walk children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::extract_identifiers(base, child, symbol_map);
        }
    }

    /// Extract identifier from a single node based on its kind
    fn extract_identifier_from_node(
        base: &mut BaseExtractor,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        match node.kind() {
            // HTML attributes: onclick, data-action (as "calls"), id, class (as "member access")
            "attribute" => {
                let mut cursor = node.walk();
                let mut attr_name = None;
                let mut attr_value = None;

                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "attribute_name" => {
                            attr_name = Some(base.get_node_text(&child));
                        }
                        "attribute_value" | "quoted_attribute_value" => {
                            let text = base.get_node_text(&child);
                            attr_value =
                                Some(text.trim_matches(|c| c == '"' || c == '\'').to_string());
                        }
                        _ => {}
                    }
                }

                if let (Some(name), Some(value)) = (attr_name, attr_value) {
                    // Event handlers and data-action attributes are "calls"
                    if name.starts_with("on") || name.starts_with("data-action") {
                        let containing_symbol_id =
                            Self::find_containing_symbol_id(base, node, symbol_map);

                        base.create_identifier(
                            &node,
                            value,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                    }
                    // id and class attributes are "member access"
                    else if name == "id" || name == "class" {
                        // For class, split by spaces and extract each class name
                        if name == "class" {
                            for class_name in value.split_whitespace() {
                                let containing_symbol_id =
                                    Self::find_containing_symbol_id(base, node, symbol_map);

                                base.create_identifier(
                                    &node,
                                    class_name.to_string(),
                                    IdentifierKind::MemberAccess,
                                    containing_symbol_id,
                                );
                            }
                        } else {
                            // id attribute
                            let containing_symbol_id =
                                Self::find_containing_symbol_id(base, node, symbol_map);

                            base.create_identifier(
                                &node,
                                value,
                                IdentifierKind::MemberAccess,
                                containing_symbol_id,
                            );
                        }
                    }
                }
            }

            _ => {
                // Skip other node types for now
                // Future: custom element names, template references, etc.
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
