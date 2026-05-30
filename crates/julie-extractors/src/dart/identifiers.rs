// Dart Extractor - Identifiers Extraction
//
// Methods for extracting identifier usages (function calls, member access, etc.)

use super::helpers::{find_child_by_type, get_node_text};
use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
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
            // Phase 3b: capture string-literal call-arguments (config-free;
            // carrier classification + gate run later in the src/ pipeline).
            record_dart_call_arg_literals(base, node, symbol_map);
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
            let identifier = base.create_identifier(
                &node,
                name,
                IdentifierKind::TypeUsage,
                containing_symbol_id,
            );
            record_outermost_dart_type_arguments(base, node, &identifier);
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

/// If `name_node` is the `type_identifier` of an *outermost* generic use site,
/// records that generic's ordered/nested applied type arguments against `identifier`.
///
/// ## Grammar details
///
/// Dart represents generic types in two structurally different ways depending on
/// context:
///
/// **Annotation / nested-arg context** (`parent.kind() == "type"`):
/// A `type` wrapper node contains `type_identifier` (the base name) and a
/// `type_arguments` named child: `type { type_identifier, type_arguments { … } }`.
/// The outermost check: if the `type` wrapper is itself inside a `type_arguments`
/// node, it is a nested arg and must not produce a separate usage row.
///
/// **Construction / heritage context** (`grandparent.kind()` ∈
/// `{new_expression, superclass, interfaces, mixins, mixin_application}`):
/// The grammar splits the generic into TWO sibling `type` nodes:
/// - First `type { type_identifier("Foo") }` — the base type name
/// - Second `type { < type { … } , type { … } > }` — the angle-bracket arg list
///
/// There is NO `type_arguments` node here; instead the sibling `type` node IS
/// the arg container and its named children are individual `type` arg-wrappers.
/// `decompose_dart_type_arg` expects exactly that layout (it handles `type`
/// wrapper children), so we can reuse it unchanged.
fn record_outermost_dart_type_arguments(
    base: &mut BaseExtractor,
    name_node: Node,
    identifier: &Identifier,
) {
    let Some(parent) = name_node.parent() else {
        return;
    };
    if parent.kind() != "type" {
        return; // type_identifier not in a type wrapper — unexpected context
    }
    let Some(grandparent) = parent.parent() else {
        return;
    };

    match grandparent.kind() {
        // ── Nested arg: rides as child of outer usage ────────────────────────
        "type_arguments" => return,

        // ── Construction / Heritage ─────────────────────────────────────────
        // The arg list is the NEXT named sibling `type` node (the `<...>` part).
        "new_expression" | "superclass" | "interfaces" | "mixins" | "mixin_application" => {
            let Some(args_container) = parent.next_named_sibling() else {
                return; // non-generic — no sibling
            };
            if args_container.kind() != "type" {
                return; // sibling is arguments/class_body/etc. — not generic
            }
            // The args_container is the `type { < type{…} , type{…} > }` node.
            // Its named children are the individual arg-wrapper `type` nodes.
            let arguments =
                crate::base::extract_type_arguments(base, args_container, decompose_dart_type_arg);
            base.record_type_arguments(identifier, arguments);
        }

        // ── Standard annotation ──────────────────────────────────────────────
        // The `type` wrapper contains `type_identifier` + `type_arguments` sibling.
        _ => {
            let mut cursor = parent.walk();
            let Some(arg_list) = parent
                .named_children(&mut cursor)
                .find(|c| c.kind() == "type_arguments")
            else {
                return; // non-generic annotation
            };
            let arguments =
                crate::base::extract_type_arguments(base, arg_list, decompose_dart_type_arg);
            base.record_type_arguments(identifier, arguments);
        }
    }
}

/// `TypeArgDecomposer` for Dart: maps a child of a `type_arguments` node to its
/// applied argument. Dart's `type_arguments` children are `type` wrapper nodes
/// (each containing a `type_identifier` and optionally nested `type_arguments`).
/// Unnamed punctuation (`<`, `,`, `>`) is skipped by the `!is_named()` guard.
fn decompose_dart_type_arg<'a>(
    base: &BaseExtractor,
    node: Node<'a>,
) -> Option<(String, Option<Node<'a>>)> {
    if !node.is_named() {
        return None; // skip punctuation: <, >, ,
    }
    if node.kind() != "type" {
        return None; // defensive skip
    }
    // Find the type_identifier child for the type name.
    let mut cursor1 = node.walk();
    let type_id = node
        .named_children(&mut cursor1)
        .find(|c| c.kind() == "type_identifier")?;
    let name = base.get_node_text(&type_id);
    // Find optional type_arguments child to recurse into for nested generics.
    let mut cursor2 = node.walk();
    let nested = node
        .named_children(&mut cursor2)
        .find(|c| c.kind() == "type_arguments");
    Some((name, nested))
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

// ============================================================================
// String-literal call-argument capture (Miller bridge Phase 3b)
// ============================================================================

/// Capture string-literal arguments of a Dart `call_expression` as `Literal`
/// records.
///
/// Config-free: `carrier` is the verbatim callee text; the URL/SQL
/// classification and the carrier gate run later in the `src/` pipeline. The
/// call has a `function` callee and an `arguments` node; a `named_argument`
/// (`body: "..."`) carries a leading `label`, so the value is its last non-label
/// child. `arg_position` is counted over the full argument list.
///
/// NOTE: Dart string interpolation (`$x` / `${x}`) nests its text as
/// `template_chars_*` content, which the shared `decode_string_literal` does not
/// recognize, so interpolated literals decode via the delimiter-strip fallback
/// (text preserved verbatim, no `{}` normalization). Plain string literals — the
/// common URL/SQL case — decode correctly. Flagged to the lead.
fn record_dart_call_arg_literals(
    base: &mut BaseExtractor,
    call_node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    let Some(function_node) = call_node.child_by_field_name("function") else {
        return;
    };
    let Some(args_node) = call_node.child_by_field_name("arguments") else {
        return;
    };
    let carrier = dart_carrier(function_node);
    let containing_symbol_id = find_containing_symbol_id(base, call_node, symbol_map);

    let arg_nodes: Vec<Node> = {
        let mut cursor = args_node.walk();
        args_node.named_children(&mut cursor).collect()
    };
    for (pos, arg) in arg_nodes.into_iter().enumerate() {
        // Named args (`name: value`) carry a leading `label`; the value is the
        // last non-label child.
        let value = if arg.kind() == "named_argument" {
            dart_named_arg_value(arg)
        } else {
            Some(arg)
        };
        if let Some(value) = value {
            if let Some(text) = base.decode_string_literal(&value) {
                base.record_literal(
                    &value,
                    text,
                    carrier.clone(),
                    pos as u32,
                    containing_symbol_id.clone(),
                );
            }
        }
    }
}

/// The value expression of a Dart `named_argument` (`label: value`): the last
/// named child that is not the `label`.
fn dart_named_arg_value(arg: Node) -> Option<Node> {
    let mut cursor = arg.walk();
    arg.named_children(&mut cursor)
        .filter(|c| c.kind() != "label")
        .last()
}

/// Derive a Dart call's carrier from its callee.
///
/// Plain `identifier` → its text (`fetch`). A `member_expression` /
/// `null_aware_member_expression` (`dio.get`, `db.rawQuery`) → the
/// `object.property` join so dotted client APIs match config (`dio.get`) while
/// bare DB verbs (`rawQuery`/`execute`) match any receiver via the gate's
/// last-segment rule. `instantiation_expression` (`foo<T>(...)`) unwraps to its
/// inner callee.
fn dart_carrier(function_node: Node) -> Option<String> {
    match function_node.kind() {
        "identifier" => Some(get_node_text(&function_node)),
        "member_expression" | "null_aware_member_expression" => {
            let object = function_node
                .child_by_field_name("object")
                .map(|n| get_node_text(&n));
            let property = function_node
                .child_by_field_name("property")
                .map(|n| get_node_text(&n));
            match (object, property) {
                (Some(o), Some(p)) => Some(format!("{o}.{p}")),
                (None, Some(p)) => Some(p),
                _ => None,
            }
        }
        "instantiation_expression" => function_node
            .child_by_field_name("function")
            .and_then(dart_carrier),
        _ => {
            let text = get_node_text(&function_node);
            if text.is_empty() { None } else { Some(text) }
        }
    }
}
