/// Identifier extraction for LSP-quality find_references
/// Tracks function calls, member access, and other identifier usages
use super::PythonExtractor;
use crate::base::{Identifier, IdentifierKind, Symbol};
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
            if is_python_type_usage_node(node) {
                if let Some(attr_node) = node.child_by_field_name("attribute") {
                    let name = extractor.base_mut().get_node_text(&attr_node);
                    if !is_python_builtin_type(&name) {
                        let containing_symbol_id =
                            find_containing_symbol_id(extractor, node, symbol_map);

                        extractor.base_mut().create_identifier(
                            &attr_node,
                            name,
                            IdentifierKind::TypeUsage,
                            containing_symbol_id,
                        );
                    }
                }
                return;
            }

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

        "identifier" => {
            if is_python_type_usage_identifier(node) {
                let name = extractor.base_mut().get_node_text(&node);
                if !is_python_builtin_type(&name) {
                    let containing_symbol_id =
                        find_containing_symbol_id(extractor, node, symbol_map);

                    extractor.base_mut().create_identifier(
                        &node,
                        name,
                        IdentifierKind::TypeUsage,
                        containing_symbol_id,
                    );
                }
            }
        }

        _ => {}
    }
}

fn is_python_type_usage_identifier(node: Node) -> bool {
    if let Some(parent) = node.parent() {
        if parent.kind() == "attribute" {
            return false;
        }
    }

    is_python_type_usage_node(node)
}

fn is_python_type_usage_node(node: Node) -> bool {
    if is_python_declaration_name(node) {
        return false;
    }

    let mut current = node;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "type" | "generic_type" | "union_type" => return true,
            "call" | "argument_list" | "return_statement" | "block" | "module" => return false,
            _ => {}
        }

        current = parent;
    }

    false
}

fn is_python_declaration_name(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };

    if let Some(name_node) = parent.child_by_field_name("name") {
        return name_node.id() == node.id()
            && matches!(
                parent.kind(),
                "class_definition" | "function_definition" | "type_alias_statement"
            );
    }

    false
}

fn is_python_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "bytes"
            | "complex"
            | "dict"
            | "float"
            | "frozenset"
            | "int"
            | "list"
            | "None"
            | "object"
            | "set"
            | "str"
            | "tuple"
            | "type"
    )
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
fn find_containing_symbol_id(
    extractor: &PythonExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    let base = extractor.base();
    base.find_containing_symbol_from_map(&node, symbol_map)
        .map(|s| s.id.clone())
}
