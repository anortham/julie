use super::helpers;
/// Identifier extraction for LSP-quality find_references
///
/// Extracts all identifier usages:
/// - Function calls: `foo()`, `require("module")`
/// - Method calls with colon syntax: `obj:method()`
/// - Member access: `obj.field`, `obj.field.nested`
use crate::extractors::base::{Identifier, IdentifierKind, Symbol};
use crate::extractors::lua::LuaExtractor;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all identifier usages (function calls, member access, etc.)
/// Following the Rust extractor reference implementation pattern
pub(super) fn extract_identifiers(
    extractor: &mut LuaExtractor,
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
    extractor: &mut LuaExtractor,
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
    extractor: &mut LuaExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Function calls: foo(), require("module")
        "function_call" => {
            // Try to get the function name from the identifier child
            if let Some(name_node) = helpers::find_child_by_type(node, "identifier") {
                let name = extractor.base().get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base_mut().create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            }
            // If no direct identifier, check for dot_index_expression (like math.sqrt())
            else if let Some(dot_index) =
                helpers::find_child_by_type(node, "dot_index_expression")
            {
                // Extract the rightmost identifier (the method name)
                if let Some(_method_node) = helpers::find_child_by_type(dot_index, "identifier") {
                    // Get all identifiers and use the last one (rightmost)
                    let mut cursor = dot_index.walk();
                    let identifiers: Vec<Node> = dot_index
                        .children(&mut cursor)
                        .filter(|c| c.kind() == "identifier")
                        .collect();

                    if let Some(last_identifier) = identifiers.last() {
                        let name = extractor.base().get_node_text(last_identifier);
                        let containing_symbol_id =
                            find_containing_symbol_id(extractor, node, symbol_map);

                        extractor.base_mut().create_identifier(
                            last_identifier,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                    }
                }
            }
        }

        // Method calls with colon syntax: obj:method()
        "method_index_expression" => {
            // Extract the method name (rightmost identifier)
            let mut cursor = node.walk();
            let identifiers: Vec<Node> = node
                .children(&mut cursor)
                .filter(|c| c.kind() == "identifier")
                .collect();

            if let Some(method_node) = identifiers.last() {
                let name = extractor.base().get_node_text(method_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base_mut().create_identifier(
                    method_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            }
        }

        // Member access with dot: obj.field, obj.field.nested
        "dot_index_expression" => {
            // Only extract if it's NOT part of a function_call or method_index_expression
            // (we handle those in the cases above)
            if let Some(parent) = node.parent() {
                if parent.kind() == "function_call" || parent.kind() == "method_index_expression" {
                    return; // Skip - handled by function/method call
                }
            }

            // Extract the rightmost identifier (the member name)
            let mut cursor = node.walk();
            let identifiers: Vec<Node> = node
                .children(&mut cursor)
                .filter(|c| c.kind() == "identifier")
                .collect();

            if let Some(member_node) = identifiers.last() {
                let name = extractor.base().get_node_text(member_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base_mut().create_identifier(
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
    extractor: &LuaExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    // CRITICAL FIX: Only search symbols from THIS FILE, not all files
    // Bug was: searching all symbols in DB caused wrong file symbols to match
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
