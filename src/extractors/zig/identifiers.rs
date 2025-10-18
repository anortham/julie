use crate::extractors::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all identifier usages (function calls, member access, etc.)
/// Following the Rust extractor reference implementation pattern
pub(super) fn extract_identifiers(
    base: &mut BaseExtractor,
    tree: &Tree,
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
        // Function calls: calculate(), obj.method()
        "call_expression" => {
            // Try to get the function name from direct identifier child
            if let Some(name_node) = base.find_child_by_type(&node, "identifier") {
                let name = base.get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                base.create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            }
            // Check for field_expression (method calls like obj.method())
            else if let Some(field_expr) = base.find_child_by_type(&node, "field_expression") {
                // Extract the rightmost identifier (the method name)
                let mut cursor = field_expr.walk();
                let identifiers: Vec<Node> = field_expr
                    .children(&mut cursor)
                    .filter(|c| c.kind() == "identifier")
                    .collect();

                if let Some(last_identifier) = identifiers.last() {
                    let name = base.get_node_text(last_identifier);
                    let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                    base.create_identifier(
                        last_identifier,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    );
                }
            }
        }

        // Member access: point.x, user.account.balance
        "field_expression" => {
            // Only extract if it's NOT part of a call_expression
            // (we handle those in the call_expression case above)
            if let Some(parent) = node.parent() {
                if parent.kind() == "call_expression" {
                    return; // Skip - handled by call_expression
                }
            }

            // Extract the rightmost identifier (the member name)
            let mut cursor = node.walk();
            let identifiers: Vec<Node> = node
                .children(&mut cursor)
                .filter(|c| c.kind() == "identifier")
                .collect();

            if let Some(member_node) = identifiers.last() {
                let name = base.get_node_text(member_node);
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                base.create_identifier(
                    member_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        _ => {
            // Skip other node types for now
            // Future: type usage, import statements, etc.
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
