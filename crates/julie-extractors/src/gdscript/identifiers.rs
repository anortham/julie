//! Identifier extraction for GDScript (function calls, member access, type annotations, etc.)

use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol, extract_type_arguments};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract all identifier usages (function calls, member access, etc.)
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
        "call" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" {
                    if let Some(parent) = node.parent() {
                        if parent.kind() == "attribute" {
                            continue;
                        }
                    }

                    let name = base.get_node_text(&child);
                    let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
                    base.create_identifier(
                        &child,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    );
                    break;
                }

                if child.kind() == "attribute" {
                    if let Some(name_node) = attribute_call_name_node(child)
                        .or_else(|| rightmost_identifier_descendant(child))
                    {
                        if let Some(parent) = node.parent() {
                            if parent.kind() == "attribute" {
                                continue;
                            }
                        }

                        let name = base.get_node_text(&name_node);
                        let containing_symbol_id =
                            find_containing_symbol_id(base, node, symbol_map);
                        base.create_identifier(
                            &name_node,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                        break;
                    }
                }
            }
        }

        "get_node" => {
            let name = "get_node".to_string();
            let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
            base.create_identifier(&node, name, IdentifierKind::Call, containing_symbol_id);
        }

        "attribute" => {
            if let Some(name_node) = attribute_call_name_node(node) {
                let name = base.get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
                base.create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
                return;
            }

            if let Some(last_child) = rightmost_identifier_descendant(node) {
                let name = base.get_node_text(&last_child);
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
                base.create_identifier(
                    &last_child,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        "subscript" => {
            if let Some(parent) = node.parent() {
                if parent.kind() == "call" {
                    return;
                }
            }

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

        "type" => {
            // Collect children once to avoid cursor borrow conflicts.
            let children: Vec<_> = {
                let mut cursor = node.walk();
                node.children(&mut cursor).collect()
            };
            if let Some(id_child) = children.iter().find(|c| c.kind() == "identifier") {
                // Plain type reference: `var x: Foo`
                let name = base.get_node_text(id_child);
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
                base.create_identifier(
                    id_child,
                    name,
                    IdentifierKind::TypeUsage,
                    containing_symbol_id,
                );
            } else if let Some(subscript_child) = children.iter().find(|c| c.kind() == "subscript")
            {
                // Generic type: `var x: Array[String]`, `Dictionary[String, int]`, etc.
                record_gdscript_subscript_as_type(base, node, *subscript_child, symbol_map);
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
    base.find_containing_symbol_from_map(&node, symbol_map)
        .map(|s| s.id.clone())
}

/// Record a GDScript generic type annotation (`Array[String]`, `Dictionary[String,int]`).
///
/// Called from the `"type"` arm when the type node's child is a `subscript`
/// (e.g. `Array[String]`). Extracts the base type name from the subscript's
/// primary-expression child (an `identifier`), creates a TypeUsage identifier
/// for it, and records the ordered type arguments from `subscript_arguments`.
fn record_gdscript_subscript_as_type(
    base: &mut BaseExtractor,
    type_node: Node,
    subscript: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    // The base type name is the subscript's primary_expression child
    // (an identifier or attribute — not the subscript_arguments field).
    let mut cursor = subscript.walk();
    let Some(base_name_node) = subscript
        .named_children(&mut cursor)
        .find(|c| c.kind() == "identifier" || c.kind() == "attribute")
    else {
        return;
    };
    let name = base.get_node_text(&base_name_node);
    let containing_symbol_id = find_containing_symbol_id(base, type_node, symbol_map);
    let identifier = base.create_identifier(
        &base_name_node,
        name,
        IdentifierKind::TypeUsage,
        containing_symbol_id,
    );
    // `subscript_arguments` is the `arguments` named field of the subscript node.
    let Some(arg_list) = subscript.child_by_field_name("arguments") else {
        return;
    };
    let arguments = extract_type_arguments(base, arg_list, decompose_gdscript_type_arg);
    base.record_type_arguments(&identifier, arguments);
}

/// `TypeArgDecomposer` for GDScript: maps a child of a `subscript_arguments`
/// node to its applied argument.
///
/// GDScript type arguments are `identifier` nodes for leaf types, or `subscript`
/// nodes for nested generics (`Array[Array[int]]`). Unnamed nodes (commas,
/// brackets) return `None` and are skipped.
fn decompose_gdscript_type_arg<'a>(
    base: &BaseExtractor,
    node: Node<'a>,
) -> Option<(String, Option<Node<'a>>)> {
    if !node.is_named() {
        return None; // skip commas and punctuation
    }
    match node.kind() {
        "identifier" => Some((base.get_node_text(&node), None)),
        "subscript" => {
            // Nested generic: `Array[Array[int]]` — extract base name + nested args.
            let mut cursor = node.walk();
            let base_node = node
                .named_children(&mut cursor)
                .find(|c| c.kind() == "identifier" || c.kind() == "attribute")?;
            let name = base.get_node_text(&base_node);
            let nested = node.child_by_field_name("arguments");
            Some((name, nested))
        }
        _ => Some((base.get_node_text(&node), None)),
    }
}

fn rightmost_identifier_descendant(node: Node) -> Option<Node> {
    if node.kind() == "attribute_call" {
        return None;
    }

    if node.kind() == "identifier" {
        return Some(node);
    }

    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    for child in children.into_iter().rev() {
        if let Some(found) = rightmost_identifier_descendant(child) {
            return Some(found);
        }
    }

    None
}

fn attribute_call_name_node(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();

    let attribute_call = children
        .iter()
        .find(|child| child.kind() == "attribute_call")?;

    let mut call_cursor = attribute_call.walk();
    attribute_call
        .children(&mut call_cursor)
        .find(|child| child.kind() == "identifier")
}
