// C# Identifier Extraction

use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all identifier usages
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
        "invocation_expression" => {
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
                } else if child.kind() == "member_access_expression" {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = base.get_node_text(&name_node);
                        let containing_symbol_id =
                            find_containing_symbol_id(base, node, symbol_map);
                        base.create_identifier(
                            &name_node,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                    }
                    break;
                }
            }
        }
        "object_creation_expression" => {
            if let Some(type_node) = node.child_by_field_name("type") {
                if let Some((name_node, name)) = terminal_type_identifier(base, type_node) {
                    let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
                    base.create_identifier(
                        &name_node,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    );
                }
            }
        }
        "member_access_expression" => {
            if let Some(parent) = node.parent() {
                if parent.kind() == "invocation_expression" {
                    return;
                }
            }

            if let Some(name_node) = node.child_by_field_name("name") {
                let name = base.get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
                base.create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }
        "identifier" => {
            if is_csharp_type_usage_identifier(node) {
                let name = base.get_node_text(&node);
                if !is_csharp_builtin_type(&name) {
                    let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
                    base.create_identifier(
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

fn terminal_type_identifier<'a>(
    base: &BaseExtractor,
    node: Node<'a>,
) -> Option<(Node<'a>, String)> {
    match node.kind() {
        "identifier" => Some((node, base.get_node_text(&node))),
        "generic_name" => direct_identifier(base, node).or_else(|| {
            node.child_by_field_name("name")
                .and_then(|name_node| terminal_type_identifier(base, name_node))
        }),
        "qualified_name" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                terminal_type_identifier(base, name_node)
            } else {
                rightmost_identifier(base, node)
            }
        }
        _ => rightmost_identifier(base, node),
    }
}

fn direct_identifier<'a>(base: &BaseExtractor, node: Node<'a>) -> Option<(Node<'a>, String)> {
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return Some((child, base.get_node_text(&child)));
        }
    }

    None
}

fn rightmost_identifier<'a>(base: &BaseExtractor, node: Node<'a>) -> Option<(Node<'a>, String)> {
    let mut cursor = node.walk();
    let mut found = None;

    for child in node.children(&mut cursor) {
        if let Some(identifier) = terminal_type_identifier(base, child) {
            found = Some(identifier);
        }
    }

    found
}

fn is_csharp_type_usage_identifier(node: Node) -> bool {
    if is_csharp_declaration_name(node) {
        return false;
    }

    let mut current = node;
    while let Some(parent) = current.parent() {
        if let Some(type_node) = parent.child_by_field_name("type") {
            if contains_node(type_node, node) {
                return true;
            }
        }

        match parent.kind() {
            "generic_name" | "qualified_name" | "array_type" | "nullable_type" | "pointer_type"
            | "tuple_type" | "type_argument_list" => return true,
            "object_creation_expression" => {
                if let Some(type_node) = parent.child_by_field_name("type") {
                    if contains_node(type_node, node) {
                        return true;
                    }
                }
            }
            "invocation_expression"
            | "member_access_expression"
            | "argument_list"
            | "assignment_expression"
            | "return_statement"
            | "block"
            | "compilation_unit" => {
                return false;
            }
            _ => {}
        }

        current = parent;
    }

    false
}

fn is_csharp_declaration_name(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };

    if let Some(name_node) = parent.child_by_field_name("name") {
        if name_node.id() == node.id() {
            return matches!(
                parent.kind(),
                "class_declaration"
                    | "interface_declaration"
                    | "struct_declaration"
                    | "enum_declaration"
                    | "method_declaration"
                    | "property_declaration"
                    | "namespace_declaration"
                    | "type_parameter"
            );
        }
    }

    false
}

fn contains_node(parent: Node, child: Node) -> bool {
    child.start_byte() >= parent.start_byte() && child.end_byte() <= parent.end_byte()
}

fn is_csharp_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "byte"
            | "sbyte"
            | "char"
            | "decimal"
            | "double"
            | "float"
            | "int"
            | "uint"
            | "nint"
            | "nuint"
            | "long"
            | "ulong"
            | "short"
            | "ushort"
            | "object"
            | "string"
            | "void"
            | "dynamic"
    )
}

fn find_containing_symbol_id(
    base: &BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    base.find_containing_symbol_from_map(&node, symbol_map)
        .map(|s| s.id.clone())
}
