// QML Identifier Extraction
// Extracts identifier usages: function calls, member access, variable references

use crate::extractors::base::{Identifier, IdentifierKind, Symbol};
use crate::extractors::qml::QmlExtractor;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all identifier usages from QML code
pub(super) fn extract_identifiers(
    extractor: &mut QmlExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    // Create symbol map for fast lookup
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    // Walk the tree and extract identifiers
    walk_tree_for_identifiers(extractor, tree.root_node(), &symbol_map);

    // Return the collected identifiers
    extractor.base.identifiers.clone()
}

/// Recursively walk the tree and extract identifiers
fn walk_tree_for_identifiers(
    extractor: &mut QmlExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    // Extract identifier from current node
    extract_identifier_from_node(extractor, node, symbol_map);

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_identifiers(extractor, child, symbol_map);
    }
}

/// Extract identifier from a single node based on its kind
fn extract_identifier_from_node(
    extractor: &mut QmlExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Function/method calls: foo(), object.method()
        "call_expression" => {
            if let Some(function_node) = node.child_by_field_name("function") {
                match function_node.kind() {
                    "identifier" => {
                        // Simple function call: foo()
                        let name = extractor.base.get_node_text(&function_node);
                        let containing_symbol_id =
                            find_containing_symbol_id(extractor, node, symbol_map);

                        extractor.base.create_identifier(
                            &function_node,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                    }
                    "member_expression" => {
                        // Member call: object.method()
                        if let Some(property_node) = function_node.child_by_field_name("property") {
                            let name = extractor.base.get_node_text(&property_node);
                            let containing_symbol_id =
                                find_containing_symbol_id(extractor, node, symbol_map);

                            extractor.base.create_identifier(
                                &property_node,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                        }
                    }
                    _ => {
                        // Other cases - skip for now
                    }
                }
            }
        }

        // Member access: object.property (not part of a call)
        "member_expression" => {
            // Only extract if NOT part of a call_expression
            if let Some(parent) = node.parent() {
                if parent.kind() == "call_expression" {
                    if let Some(function_node) = parent.child_by_field_name("function") {
                        if function_node.id() == node.id() {
                            return; // Skip - handled by call_expression
                        }
                    }
                }
            }

            // Extract the property being accessed
            if let Some(property_node) = node.child_by_field_name("property") {
                let name = extractor.base.get_node_text(&property_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base.create_identifier(
                    &property_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        // Variable references in expressions
        "identifier" => {
            // Only create variable reference if not already handled by call or member access
            if let Some(parent) = node.parent() {
                match parent.kind() {
                    "call_expression"
                    | "member_expression"
                    | "function_declaration"
                    | "ui_object_definition"
                    | "ui_property"
                    | "ui_signal" => {
                        return; // Skip - handled elsewhere or is a definition
                    }
                    _ => {
                        // This is a variable reference
                        let name = extractor.base.get_node_text(&node);
                        let containing_symbol_id =
                            find_containing_symbol_id(extractor, node, symbol_map);

                        extractor.base.create_identifier(
                            &node,
                            name,
                            IdentifierKind::VariableRef,
                            containing_symbol_id,
                        );
                    }
                }
            }
        }

        _ => {
            // Skip other node types
        }
    }
}

/// Find the containing symbol ID for a node
fn find_containing_symbol_id(
    _extractor: &QmlExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    let mut current = node;

    while let Some(parent) = current.parent() {
        let parent_line = parent.start_position().row + 1;

        // Check if this parent matches any symbol by line number
        for symbol in symbol_map.values() {
            if symbol.start_line == parent_line as u32 {
                return Some(symbol.id.clone());
            }
        }

        current = parent;
    }

    None
}
