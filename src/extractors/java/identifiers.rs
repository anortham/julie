/// Identifier extraction for LSP-quality find_references
use crate::extractors::base::{Identifier, IdentifierKind, Symbol};
use crate::extractors::java::JavaExtractor;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all identifier usages (function calls, member access, etc.)
/// Following the Rust extractor reference implementation pattern
pub(super) fn extract_identifiers(
    extractor: &mut JavaExtractor,
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
    extractor: &mut JavaExtractor,
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
    extractor: &mut JavaExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Method calls: foo(), bar.baz(), System.out.println()
        "method_invocation" => {
            // Try to get the method name from the "name" field (standard tree-sitter pattern)
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = extractor.base().get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base_mut().create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            } else {
                // Fallback: look for identifier children
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        let name = extractor.base().get_node_text(&child);
                        let containing_symbol_id =
                            find_containing_symbol_id(extractor, node, symbol_map);

                        extractor.base_mut().create_identifier(
                            &child,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                        break;
                    }
                }
            }
        }

        // Field access: object.field
        "field_access" => {
            // Only extract if it's NOT part of a method_invocation
            // (we handle those in the method_invocation case above)
            if let Some(parent) = node.parent() {
                if parent.kind() == "method_invocation" {
                    return; // Skip - handled by method_invocation
                }
            }

            // Extract the rightmost identifier (the field name)
            if let Some(name_node) = node.child_by_field_name("field") {
                let name = extractor.base().get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base_mut().create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        _ => {
            // Skip other node types for now
            // Future: type usage, constructor calls, etc.
        }
    }
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
fn find_containing_symbol_id(
    extractor: &JavaExtractor,
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
