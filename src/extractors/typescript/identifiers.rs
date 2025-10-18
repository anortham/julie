//! Identifier extraction (function calls, member access, etc.)
//!
//! This module handles extraction of identifier usages for LSP-quality find_references functionality,
//! including function calls, member access, and other identifier references.

use crate::extractors::base::{Identifier, IdentifierKind, Symbol};
use crate::extractors::typescript::TypeScriptExtractor;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all identifier usages from the tree
pub(super) fn extract_identifiers(
    extractor: &mut TypeScriptExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    // Create symbol map for fast lookup
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    // Walk the tree and extract identifiers
    walk_tree_for_identifiers(extractor, tree.root_node(), &symbol_map);

    // Return the collected identifiers
    extractor.base().identifiers.clone()
}

/// Recursively walk tree extracting identifiers from each node
fn walk_tree_for_identifiers(
    extractor: &mut TypeScriptExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    // Extract identifier from this node if applicable
    extract_identifier_from_node(extractor, node, symbol_map);

    // Recursively walk children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_identifiers(extractor, child, symbol_map);
    }
}

/// Extract identifier from a single node based on its kind
fn extract_identifier_from_node(
    extractor: &mut TypeScriptExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Function/method calls: foo(), object.method()
        "call_expression" => {
            // The function being called is in the "function" field
            if let Some(function_node) = node.child_by_field_name("function") {
                match function_node.kind() {
                    "identifier" => {
                        // Simple function call: foo()
                        let name = extractor.base().get_node_text(&function_node);
                        let containing_symbol_id =
                            find_containing_symbol_id(extractor, node, symbol_map);

                        extractor.base_mut().create_identifier(
                            &function_node,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                    }
                    "member_expression" => {
                        // Member call: object.method()
                        // Extract the rightmost identifier (the method name)
                        if let Some(property_node) = function_node.child_by_field_name("property") {
                            let name = extractor.base().get_node_text(&property_node);
                            let containing_symbol_id =
                                find_containing_symbol_id(extractor, node, symbol_map);

                            extractor.base_mut().create_identifier(
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
                let name = extractor.base().get_node_text(&property_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base_mut().create_identifier(
                    &property_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        _ => {
            // Skip other node types for now
        }
    }
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
fn find_containing_symbol_id(
    extractor: &TypeScriptExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    // CRITICAL FIX: Only search symbols from THIS FILE
    let file_symbols: Vec<Symbol> = symbol_map
        .values()
        .filter(|s| s.file_path == extractor.base().file_path)
        .map(|&s| s.clone())
        .collect();

    extractor
        .base()
        .find_containing_symbol(&node, &file_symbols)
        .map(|s| s.id.clone())
}
