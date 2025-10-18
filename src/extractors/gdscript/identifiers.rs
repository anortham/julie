//! Identifier extraction for GDScript (function calls, member access, etc.)

use crate::extractors::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract all identifier usages (function calls, member access, etc.)
pub(super) fn extract_identifiers(
    base: &mut BaseExtractor,
    tree: &tree_sitter::Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    // Create symbol map for fast lookup
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    // Walk the tree and extract identifiers
    walk_tree_for_identifiers(base, tree.root_node(), &symbol_map);

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
    extract_identifier_from_node(base, node, symbol_map);

    // Recursively walk children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_identifiers(base, child, symbol_map);
    }
}

/// Extract identifier from a single node based on its kind
fn extract_identifier_from_node(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // GDScript function calls: call nodes
        "call" => {
            // For call nodes, the function name is typically a child identifier or attribute
            // Find the first identifier or attribute child
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" || child.kind() == "attribute" {
                    let name = base.get_node_text(&child);

                    // Skip if this is part of a member access chain (parent is attribute)
                    if let Some(parent) = node.parent() {
                        if parent.kind() == "attribute" {
                            continue;
                        }
                    }

                    let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
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

        // GDScript get_node calls (special case)
        "get_node" => {
            // get_node is a special GDScript function for node access
            let name = "get_node".to_string();
            let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
            base.create_identifier(&node, name, IdentifierKind::Call, containing_symbol_id);
        }

        // GDScript member access: attribute and subscript nodes
        "attribute" => {
            // Skip if parent is a call (handled above)
            if let Some(parent) = node.parent() {
                if parent.kind() == "call" {
                    return;
                }
            }

            // For attribute nodes, the rightmost identifier is the member being accessed
            // Find the attribute child (rightmost in the chain)
            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();

            // The last identifier or name in an attribute is the member being accessed
            if let Some(last_child) = children.last() {
                if last_child.kind() == "identifier" {
                    let name = base.get_node_text(last_child);
                    let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
                    base.create_identifier(
                        last_child,
                        name,
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                }
            }
        }

        "subscript" => {
            // Skip if parent is invocation (call)
            if let Some(parent) = node.parent() {
                if parent.kind() == "call" {
                    return;
                }
            }

            // For subscript access like array[index], we extract the index as member access
            // This is less common but still valid
            if let Some(index_node) = node.child_by_field_name("index") {
                if index_node.kind() == "identifier" {
                    let name = base.get_node_text(&index_node);
                    let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
                    base.create_identifier(
                        &index_node,
                        name,
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                }
            }
        }

        _ => {}
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
