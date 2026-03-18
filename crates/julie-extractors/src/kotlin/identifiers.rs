//! Identifier and reference extraction for Kotlin
//!
//! This module handles extraction of function calls, member access, and other
//! identifier usages for LSP-quality find_references support.

use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract all identifier usages from a Kotlin file
pub(super) fn extract_identifiers(
    base: &mut BaseExtractor,
    tree: &tree_sitter::Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    walk_tree_for_identifiers(base, tree.root_node(), &symbol_map);

    base.identifiers.clone()
}

/// Recursively walk tree extracting identifiers from each node
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

/// Extract identifier from a single node based on its kind
fn extract_identifier_from_node(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Function/method calls: foo(), bar.baz()
        "call_expression" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" || child.kind() == "simple_identifier" {
                    let name = base.get_node_text(&child);
                    let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                    base.create_identifier(
                        &child,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    );
                    return;
                } else if child.kind() == "navigation_expression" {
                    // For member access calls, extract the rightmost identifier
                    if let Some((name_node, name)) = extract_rightmost_identifier(base, &child) {
                        let containing_symbol_id =
                            find_containing_symbol_id(base, node, symbol_map);

                        base.create_identifier(
                            &name_node,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                    }
                    return;
                }
            }
        }

        // Type references in type positions: val x: Foo, fun f(a: Foo): Bar,
        // class Foo(service: Bar), typealias A = Foo
        // Kotlin uses `user_type` for type annotations. It contains an
        // `identifier` child for the type name. Unlike Scala/Java,
        // class/interface/object declaration names use `identifier`
        // directly (not inside `user_type`), so we don't need to filter
        // declaration names here.
        "user_type" => {
            // Extract the first identifier child — that's the type name.
            // Kotlin tree-sitter uses `identifier` (not `simple_identifier`)
            // inside `user_type` nodes.
            let name_node = node
                .children(&mut node.walk())
                .find(|n| n.kind() == "identifier" || n.kind() == "simple_identifier");

            if let Some(name_node) = name_node {
                let name = base.get_node_text(&name_node);

                if is_kotlin_noise_type(&name) {
                    return;
                }

                let containing = find_containing_symbol_id(base, node, symbol_map);
                base.create_identifier(&name_node, name, IdentifierKind::TypeUsage, containing);
            }
        }

        // Member access: object.property
        "navigation_expression" => {
            // Only extract if it's NOT part of a call_expression
            if let Some(parent) = node.parent() {
                if parent.kind() == "call_expression" {
                    return;
                }
            }

            // Extract the rightmost identifier (the member name)
            if let Some((name_node, name)) = extract_rightmost_identifier(base, &node) {
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                base.create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        _ => {
            // Skip other node types
        }
    }
}

/// Find the ID of the symbol that contains this node
fn find_containing_symbol_id(
    base: &BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    // Only search symbols from THIS FILE, not all files
    let file_symbols: Vec<Symbol> = symbol_map
        .values()
        .filter(|s| s.file_path == base.file_path)
        .map(|&s| s.clone())
        .collect();

    base.find_containing_symbol(&node, &file_symbols)
        .map(|s| s.id.clone())
}

/// Returns true for Kotlin types that are too common to be meaningful
/// type references for centrality scoring.
///
/// Includes:
/// - Single-letter type params (T, K, V, E, R) — generic type parameters
/// - Kotlin/JVM primitive and base types — ubiquitous in every file
fn is_kotlin_noise_type(name: &str) -> bool {
    // Single-letter uppercase names are almost always generic type parameters.
    if name.len() == 1
        && name
            .chars()
            .next()
            .map_or(false, |c| c.is_ascii_uppercase())
    {
        return true;
    }

    matches!(
        name,
        // Kotlin primitive types
        "Int"
            | "Long"
            | "Short"
            | "Byte"
            | "Float"
            | "Double"
            | "Char"
            | "Boolean"
            | "Unit"
            // Kotlin top types
            | "Any"
            | "Nothing"
            // JVM interop
            | "String"
            | "Object"
    )
}

/// Helper to extract the rightmost identifier in a navigation_expression
fn extract_rightmost_identifier<'a>(
    base: &BaseExtractor,
    node: &Node<'a>,
) -> Option<(Node<'a>, String)> {
    // Kotlin navigation_expression structure
    // For chained access like user.account.balance:
    // - We need to find the rightmost identifier

    // First, try to find identifier children (rightmost in chain)
    let identifiers: Vec<Node> = node
        .children(&mut node.walk())
        .filter(|n| n.kind() == "identifier" || n.kind() == "simple_identifier")
        .collect();

    if let Some(last_identifier) = identifiers.last() {
        let name = base.get_node_text(last_identifier);
        return Some((*last_identifier, name));
    }

    None
}
