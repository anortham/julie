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

        // Type references in type positions: val x: Foo, def f(a: Foo): Bar,
        // class Foo extends Bar, type A = Foo
        // Scala uses `type_identifier` for both declaration names and references.
        // We filter out declaration names via parent context.
        "type_identifier" => {
            if is_type_declaration_name(&node) {
                return;
            }

            let name = base.get_node_text(&node);

            if is_scala_noise_type(&name) {
                return;
            }

            let containing = find_containing_symbol_id(base, node, symbol_map);
            base.create_identifier(&node, name, IdentifierKind::TypeUsage, containing);
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
                base.create_identifier(&name_node, name, IdentifierKind::MemberAccess, containing);
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

/// Check if a `type_identifier` node is a declaration name rather than a type reference.
///
/// In Scala, `type_identifier` appears as the `name` field of:
/// - `type_definition` → `type Foo = ...` (declaration)
///
/// Class/trait/object names use `identifier`, not `type_identifier`, so they
/// don't need to be filtered here.
fn is_type_declaration_name(node: &Node) -> bool {
    if let Some(parent) = node.parent() {
        if let Some(name_node) = parent.child_by_field_name("name") {
            if name_node.id() == node.id() {
                return parent.kind() == "type_definition";
            }
        }
    }
    false
}

/// Returns true for Scala types that are too common to be meaningful
/// type references for centrality scoring.
///
/// Includes:
/// - Single-letter type params (T, A, B, etc.) — generic type parameters used in scope
/// - Scala primitive/base types — ubiquitous in every file
fn is_scala_noise_type(name: &str) -> bool {
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
        // Scala AnyVal types
        "Int"
            | "Long"
            | "Short"
            | "Byte"
            | "Float"
            | "Double"
            | "Char"
            | "Boolean"
            | "Unit"
            // Scala top types
            | "Any"
            | "AnyRef"
            | "AnyVal"
            | "Nothing"
            | "Null"
            // Java interop
            | "String"
            | "Object"
    )
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

    identifiers.last().map(|n| (*n, base.get_node_text(n)))
}
