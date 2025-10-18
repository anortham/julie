/// Identifier extraction for LSP-quality find_references
/// Tracks function calls, member access, and other identifier usages
use super::PythonExtractor;
use crate::extractors::base::{Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all identifier usages (function calls, member access, etc.)
/// Following the Rust extractor reference implementation pattern
pub fn extract_identifiers(
    extractor: &mut PythonExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    // Create symbol map for fast lookup
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    // Walk the tree and extract identifiers
    walk_tree_for_identifiers(extractor, tree.root_node(), &symbol_map);

    // Return the collected identifiers
    extractor.base_mut().identifiers.clone()
}

/// Recursively walk tree extracting identifiers from each node
fn walk_tree_for_identifiers(
    extractor: &mut PythonExtractor,
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
    extractor: &mut PythonExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Function/method calls: foo(), bar.baz()
        // Python uses "call" node type
        "call" => {
            // The function being called is in the "function" field
            if let Some(function_node) = node.child_by_field_name("function") {
                match function_node.kind() {
                    "identifier" => {
                        // Simple function call: foo()
                        let name = extractor.base_mut().get_node_text(&function_node);
                        let containing_symbol_id =
                            find_containing_symbol_id(extractor, node, symbol_map);

                        extractor.base_mut().create_identifier(
                            &function_node,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                    }
                    "attribute" => {
                        // Member call: object.method()
                        // Extract the rightmost identifier (the method name)
                        if let Some(attr_node) = function_node.child_by_field_name("attribute") {
                            let name = extractor.base_mut().get_node_text(&attr_node);
                            let containing_symbol_id =
                                find_containing_symbol_id(extractor, node, symbol_map);

                            extractor.base_mut().create_identifier(
                                &attr_node,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                        }
                    }
                    _ => {
                        // Other cases like subscript expressions
                        // Skip for now
                    }
                }
            }
        }

        // Member access: object.property
        // Python uses "attribute" node type
        "attribute" => {
            // Only extract if it's NOT part of a call
            // (we handle those in the call case above)
            if let Some(parent) = node.parent() {
                if parent.kind() == "call" {
                    // Check if this attribute is the function being called
                    if let Some(function_node) = parent.child_by_field_name("function") {
                        if function_node.id() == node.id() {
                            return; // Skip - handled by call
                        }
                    }
                }
            }

            // Extract the attribute name
            if let Some(attr_node) = node.child_by_field_name("attribute") {
                let name = extractor.base_mut().get_node_text(&attr_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base_mut().create_identifier(
                    &attr_node,
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
    extractor: &PythonExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    let base = extractor.base();

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
