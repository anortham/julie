//! Identifier and reference extraction for Scala
//!
//! Extracts function calls, member access, and other identifier usages
//! for LSP-quality find_references support.

use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract all identifier usages from a Scala file
pub(super) fn extract_identifiers(
    base: &mut BaseExtractor,
    tree: &tree_sitter::Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    walk_tree_for_identifiers(base, tree.root_node(), &symbol_map);

    base.identifiers.clone()
}

/// Recursively walk tree extracting identifiers
fn walk_tree_for_identifiers(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    extract_identifier_from_node(base, node, symbol_map);

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_identifiers(base, child, symbol_map);
    }
}

/// Extract identifier from a single node
fn extract_identifier_from_node(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Function/method calls
        "call_expression" => {
            for child in node.children(&mut node.walk()) {
                if child.kind() == "identifier" {
                    let name = base.get_node_text(&child);
                    let containing = find_containing_symbol_id(base, node, symbol_map);
                    base.create_identifier(&child, name, IdentifierKind::Call, containing);
                    return;
                } else if child.kind() == "field_expression" {
                    if let Some((name_node, name)) = extract_rightmost_identifier(base, &child) {
                        let containing = find_containing_symbol_id(base, node, symbol_map);
                        base.create_identifier(&name_node, name, IdentifierKind::Call, containing);
                    }
                    return;
                }
            }
        }

        // Member access: obj.field
        "field_expression" => {
            // Only extract if NOT part of a call_expression
            if let Some(parent) = node.parent() {
                if parent.kind() == "call_expression" {
                    return;
                }
            }

            if let Some((name_node, name)) = extract_rightmost_identifier(base, &node) {
                let containing = find_containing_symbol_id(base, node, symbol_map);
                base.create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing,
                );
            }
        }

        _ => {}
    }
}

/// Find the ID of the symbol that contains this node
fn find_containing_symbol_id(
    base: &BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    let file_symbols: Vec<Symbol> = symbol_map
        .values()
        .filter(|s| s.file_path == base.file_path)
        .map(|&s| s.clone())
        .collect();

    base.find_containing_symbol(&node, &file_symbols)
        .map(|s| s.id.clone())
}

/// Extract the rightmost identifier from a field_expression
fn extract_rightmost_identifier<'a>(
    base: &BaseExtractor,
    node: &Node<'a>,
) -> Option<(Node<'a>, String)> {
    let identifiers: Vec<Node> = node
        .children(&mut node.walk())
        .filter(|n| n.kind() == "identifier")
        .collect();

    identifiers
        .last()
        .map(|n| (*n, base.get_node_text(n)))
}
