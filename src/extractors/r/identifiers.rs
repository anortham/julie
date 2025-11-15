// R Identifier Extraction
// Extracts identifier usages: function calls, variable references, member access

use crate::extractors::base::{Identifier, IdentifierKind, Symbol};
use crate::extractors::r::RExtractor;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all identifier usages from R code
pub(super) fn extract_identifiers(
    extractor: &mut RExtractor,
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
    extractor: &mut RExtractor,
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
    extractor: &mut RExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Function calls: foo(), library(dplyr), lapply(x, f)
        "call" => {
            if let Some(function_node) = node.child(0) {
                let name = match function_node.kind() {
                    "identifier" => extractor.base.get_node_text(&function_node),
                    "namespace_operator" => {
                        // Handle package::function syntax
                        if let Some(function_child) = function_node.child(2) {
                            extractor.base.get_node_text(&function_child)
                        } else {
                            extractor.base.get_node_text(&function_node)
                        }
                    }
                    "extract_operator" => {
                        // Handle object$method() syntax
                        if let Some(member) = function_node.child(2) {
                            extractor.base.get_node_text(&member)
                        } else {
                            extractor.base.get_node_text(&function_node)
                        }
                    }
                    _ => extractor.base.get_node_text(&function_node),
                };

                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base.create_identifier(
                    &function_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            }
        }

        // Member access: object$property, object@slot
        "extract_operator" => {
            // Skip if this is part of a call expression (handled above)
            if let Some(parent) = node.parent() {
                if parent.kind() == "call" {
                    return;
                }
            }

            // Extract the member being accessed
            if let Some(member_node) = node.child(2) {
                let name = extractor.base.get_node_text(&member_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base.create_identifier(
                    &member_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        // Variable references
        "identifier" => {
            // Only create variable reference if not already handled
            if let Some(parent) = node.parent() {
                match parent.kind() {
                    // Skip if this is the function being called
                    "call" if parent.child(0).map(|c| c.id()) == Some(node.id()) => {
                        return;
                    }
                    // Skip if this is in an extract operator (handled separately)
                    "extract_operator" => {
                        return;
                    }
                    // Skip if this is in a namespace operator
                    "namespace_operator" => {
                        return;
                    }
                    // Skip if this is a parameter name
                    "parameter" => {
                        return;
                    }
                    // Check if this is the left side of an assignment
                    "binary_operator" => {
                        if let Some(operator) = parent.child(1) {
                            let op_text = extractor.base.get_node_text(&operator);
                            // Skip if this is the target of an assignment
                            if (op_text == "<-" || op_text == "=" || op_text == "<<-")
                                && parent.child(0).map(|c| c.id()) == Some(node.id())
                            {
                                return;
                            }
                            if (op_text == "->" || op_text == "->>")
                                && parent.child(2).map(|c| c.id()) == Some(node.id())
                            {
                                return;
                            }
                        }
                        // This is a variable being used in a binary expression
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
                    _ => {
                        // This is likely a variable reference
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
    _extractor: &RExtractor,
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
