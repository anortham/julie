use super::helpers::{extract_method_name_from_call, is_assignment_target};
/// Identifier extraction for Ruby symbols
/// Handles LSP-quality find_references functionality
use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
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
/// Ruby-specific: "call" nodes are used for both function calls and member access
fn extract_identifier_from_node(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Ruby uses "call" for both function calls and member access
        // The difference is whether there's a receiver field
        "call" => {
            // Check if this call has a receiver (member access)
            if let Some(_receiver) = node.child_by_field_name("receiver") {
                // This is member access like obj.method
                // Extract the method name (rightmost identifier)
                if let Some(method_node) = node.child_by_field_name("method") {
                    let name = base.get_node_text(&method_node);
                    let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                    base.create_identifier(
                        &method_node,
                        name,
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                }
            } else {
                // This is a simple function call (no receiver)
                // Extract the method/function name
                if let Some(name) = extract_method_name_from_call(node, |n| base.get_node_text(n)) {
                    // Find the identifier node for proper location
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        if child.kind() == "identifier" {
                            let containing_symbol_id =
                                find_containing_symbol_id(base, node, symbol_map);

                            base.create_identifier(
                                &child,
                                name.clone(),
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                            break;
                        }
                    }
                }
            }
        }

        // Type references: superclass, scope_resolution, include/extend args, etc.
        // Ruby constants are always PascalCase class/module names, so any constant
        // in a reference position (not a declaration name or assignment target) is
        // a type usage that contributes to centrality scoring.
        "constant" => {
            // Skip declaration names: the `name` field of class/module nodes
            if is_constant_declaration_name(&node) {
                return;
            }
            // Skip assignment LHS: `CONST = value` defines, not references
            if is_assignment_target(&node) {
                return;
            }

            let name = base.get_node_text(&node);
            let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

            base.create_identifier(&node, name, IdentifierKind::TypeUsage, containing_symbol_id);
        }

        _ => {
            // Skip other node types for now
        }
    }
}

/// Returns true if this constant node is the declaration name of a class or module.
/// Example: `class Foo` or `module Bar` — the `Foo`/`Bar` constant is a declaration,
/// not a reference. All other constant positions are type references.
fn is_constant_declaration_name(node: &Node) -> bool {
    if let Some(parent) = node.parent() {
        if let Some(name_node) = parent.child_by_field_name("name") {
            if name_node.id() == node.id() {
                return matches!(parent.kind(), "class" | "module");
            }
        }
    }
    false
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
