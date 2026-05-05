// Dart Extractor - Identifiers Extraction
//
// Methods for extracting identifier usages (function calls, member access, etc.)

use super::helpers::{find_child_by_type, get_node_text};
use crate::base::{BaseExtractor, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

/// Walk the entire tree extracting identifier usages
pub(super) fn walk_tree_for_identifiers(
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

fn extract_identifier_from_node(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        "call_expression" => {
            if let Some(target_node) = call_target_name_node(node.child_by_field_name("function")) {
                let name = get_node_text(&target_node);
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
                base.create_identifier(
                    &target_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            }
        }

        "member_expression" | "null_aware_member_expression" => {
            if is_call_function_node(node) {
                return;
            }

            if let Some(property_node) = node.child_by_field_name("property") {
                let name = get_node_text(&property_node);
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
                base.create_identifier(
                    &property_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        "member_access" => {
            if let Some(id_node) = find_child_by_type(&node, "identifier") {
                let name = get_node_text(&id_node);

                let is_call = if let Some(selector_node) = find_child_by_type(&node, "selector") {
                    find_child_by_type(&selector_node, "argument_part").is_some()
                } else {
                    false
                };

                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
                let kind = if is_call {
                    crate::base::IdentifierKind::Call
                } else {
                    crate::base::IdentifierKind::MemberAccess
                };

                base.create_identifier(&id_node, name, kind, containing_symbol_id);
            }
        }

        // Type references: field types, parameter types, return types, generic args,
        // extends, implements, with clauses, mixin "on" constraints.
        // Dart tree-sitter uses `type_identifier` for type names. In Dart, class/enum/
        // mixin/extension declarations use `identifier` for their name (not type_identifier),
        // so the only declaration context where type_identifier IS the name is `type_alias`.
        "type_identifier" => {
            if is_type_declaration_name(&node) {
                return;
            }

            let name = get_node_text(&node);

            // Skip single-letter generic type parameters (T, K, V, E, S, R, etc.)
            if name.len() == 1
                && name
                    .chars()
                    .next()
                    .map_or(false, |c| c.is_ascii_uppercase())
            {
                return;
            }

            let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

            base.create_identifier(&node, name, IdentifierKind::TypeUsage, containing_symbol_id);
        }

        "unconditional_assignable_selector" => {
            if let Some(id_node) = find_child_by_type(&node, "identifier") {
                let name = get_node_text(&id_node);
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                base.create_identifier(
                    &id_node,
                    name,
                    crate::base::IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        _ => {}
    }
}

/// Check if a `type_identifier` node is a declaration name rather than a type reference.
///
/// In Dart's tree-sitter grammar, most declarations (class, enum, mixin, extension)
/// use `identifier` for their name, NOT `type_identifier`. The only declaration
/// context where `type_identifier` is the name is `type_alias`:
///
///   typedef Callback = void Function(Event event);
///          ^^^^^^^^ type_identifier (declaration name - skip)
///
/// Other type_identifier appearances are references (superclass, field types,
/// parameter types, generic args, etc.) and should be extracted as TypeUsage.
fn is_type_declaration_name(node: &Node) -> bool {
    if let Some(parent) = node.parent() {
        // type_alias: `typedef Callback = ...` - the first type_identifier is the name
        if parent.kind() == "type_alias" {
            // Check if this is the first type_identifier child of the type_alias
            let mut cursor = parent.walk();
            for child in parent.children(&mut cursor) {
                if child.kind() == "type_identifier" {
                    return child.id() == node.id();
                }
            }
        }
    }
    false
}

fn call_target_name_node(function_node: Option<Node>) -> Option<Node> {
    let function_node = function_node?;
    match function_node.kind() {
        "identifier" => Some(function_node),
        "member_expression" | "null_aware_member_expression" => {
            function_node.child_by_field_name("property")
        }
        "instantiation_expression" => {
            call_target_name_node(function_node.child_by_field_name("function"))
        }
        _ => None,
    }
}

fn is_call_function_node(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };

    if parent.kind() != "call_expression" {
        return false;
    }

    parent
        .child_by_field_name("function")
        .is_some_and(|function_node| function_node.id() == node.id())
}

fn find_containing_symbol_id(
    base: &BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    base.find_containing_symbol_from_map(&node, symbol_map)
        .map(|s| s.id.clone())
}
