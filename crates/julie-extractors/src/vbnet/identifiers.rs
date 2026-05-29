use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol, extract_type_arguments};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

pub fn extract_identifiers(
    base: &mut BaseExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();
    walk_tree_for_identifiers(base, tree.root_node(), &symbol_map);
    base.identifiers.clone()
}

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

fn extract_identifier_from_node(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        "invocation_expression" | "invocation" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" {
                    let name = base.get_node_text(&child);
                    let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
                    base.create_identifier(
                        &child,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    );
                    break;
                } else if child.kind() == "member_access_expression"
                    || child.kind() == "member_access"
                {
                    let mut mc = child.walk();
                    let children: Vec<_> = child.children(&mut mc).collect();
                    if let Some(name_node) =
                        children.iter().rev().find(|c| c.kind() == "identifier")
                    {
                        let name = base.get_node_text(name_node);
                        let containing_symbol_id =
                            find_containing_symbol_id(base, node, symbol_map);
                        base.create_identifier(
                            name_node,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                    }
                    break;
                }
            }
        }
        "member_access_expression" | "member_access" => {
            if let Some(parent) = node.parent() {
                if parent.kind() == "invocation_expression" || parent.kind() == "invocation" {
                    return;
                }
            }

            let mut cursor = node.walk();
            let children: Vec<_> = node.children(&mut cursor).collect();
            if let Some(name_node) = children.iter().rev().find(|c| c.kind() == "identifier") {
                let name = base.get_node_text(name_node);
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
                base.create_identifier(
                    name_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        // VB.NET generic type use site: `List(Of String)`, `Dictionary(Of String, Integer)`
        // Grammar: generic_type → namespace_name (base name) + type_argument_list (args)
        "generic_type" => {
            // Outermost-only rule: skip if this generic_type is a nested arg of another generic.
            if node
                .parent()
                .map(|p| p.kind() == "type_argument_list")
                .unwrap_or(false)
            {
                return;
            }
            let children: Vec<_> = {
                let mut cursor = node.walk();
                node.children(&mut cursor).collect()
            };
            let Some(name_node) = children.iter().find(|c| c.kind() == "namespace_name") else {
                return;
            };
            let name = base.get_node_text(name_node);
            let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
            let identifier = base.create_identifier(
                name_node,
                name,
                IdentifierKind::TypeUsage,
                containing_symbol_id,
            );
            if let Some(arg_list) = children.iter().find(|c| c.kind() == "type_argument_list") {
                let arguments = extract_type_arguments(base, *arg_list, decompose_vbnet_type_arg);
                base.record_type_arguments(&identifier, arguments);
            }
        }

        _ => {}
    }
}

/// `TypeArgDecomposer` for VB.NET: maps a named child of `type_argument_list` to its
/// applied argument. Nested `generic_type` children recurse; everything else is a leaf.
fn decompose_vbnet_type_arg<'a>(
    base: &BaseExtractor,
    node: Node<'a>,
) -> Option<(String, Option<Node<'a>>)> {
    if !node.is_named() {
        return None; // skip punctuation (commas, "Of" keyword, parens)
    }
    match node.kind() {
        "generic_type" => {
            // Nested generic: e.g. `List(Of User)` inside `Dictionary(Of String, List(Of User))`.
            let children: Vec<_> = {
                let mut cursor = node.walk();
                node.children(&mut cursor).collect()
            };
            let name_node = children.iter().find(|c| c.kind() == "namespace_name")?;
            let name = base.get_node_text(name_node);
            let nested = children
                .into_iter()
                .find(|c| c.kind() == "type_argument_list");
            Some((name, nested))
        }
        _ => {
            // Leaf: namespace_name ("String", "User"), primitive_type ("Integer"), etc.
            Some((base.get_node_text(&node), None))
        }
    }
}

fn find_containing_symbol_id(
    base: &BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    base.find_containing_symbol_from_map(&node, symbol_map)
        .map(|s| s.id.clone())
}
