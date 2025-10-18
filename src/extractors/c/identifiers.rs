//! Identifier extraction for function calls and member access
//!
//! This module handles extraction of identifier usages within C code, such as function calls
//! and member/field access operations.

use crate::extractors::base::{Identifier, IdentifierKind, Symbol};
use crate::extractors::c::CExtractor;
use std::collections::HashMap;

/// Extract all identifiers from the syntax tree
pub(super) fn extract_identifiers(
    extractor: &mut CExtractor,
    tree: &tree_sitter::Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    // Create symbol map for fast lookup
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    // Walk the tree and extract identifiers
    walk_tree_for_identifiers(extractor, tree.root_node(), &symbol_map);

    // Return the collected identifiers
    extractor.base.identifiers.clone()
}

/// Recursively walk tree extracting identifiers from each node
fn walk_tree_for_identifiers(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
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
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Function calls: add(), printf()
        "call_expression" => {
            if let Some(func_node) = node.child_by_field_name("function") {
                let name = extractor.base.get_node_text(&func_node);

                // Find containing symbol (which function contains this call)
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                // Create identifier for this function call
                extractor.base.create_identifier(
                    &func_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            }
        }

        // Member/field access: p->x, obj.field
        "field_expression" => {
            // Skip if parent is a call_expression (will be handled as function call)
            if let Some(parent) = node.parent() {
                if parent.kind() == "call_expression" {
                    return;
                }
            }

            // Extract field name from field_expression
            if let Some(field_node) = node.child_by_field_name("field") {
                let name = extractor.base.get_node_text(&field_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base.create_identifier(
                    &field_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        _ => {}
    }
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
fn find_containing_symbol_id(
    extractor: &CExtractor,
    node: tree_sitter::Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    // CRITICAL FIX: Only search symbols from THIS FILE, not all files
    // Bug was: searching all symbols in DB caused wrong file symbols to match
    let file_symbols: Vec<Symbol> = symbol_map
        .values()
        .filter(|s| s.file_path == extractor.base.file_path)
        .map(|&s| s.clone())
        .collect();

    extractor
        .base
        .find_containing_symbol(&node, &file_symbols)
        .map(|s| s.id.clone())
}
