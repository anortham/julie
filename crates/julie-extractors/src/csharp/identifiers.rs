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
            // Phase 3: capture string-literal call-arguments (config-free; the
            // carrier classification + gate happen in the src/ pipeline).
            record_csharp_call_arg_literals(base, node, symbol_map);
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
                    let identifier = base.create_identifier(
                        &node,
                        name,
                        IdentifierKind::TypeUsage,
                        containing_symbol_id,
                    );
                    record_outermost_generic_type_arguments(base, node, &identifier);
                }
            }
        }
        _ => {}
    }
}

/// If `name_node` is the base identifier of an *outermost* generic type use
/// (e.g. the `Dictionary` of `Dictionary<string, List<int>>`), record that
/// generic's ordered/nested applied type arguments against `identifier`.
///
/// Fires from the universal `identifier` arm so it uniformly covers member
/// types, `new T<...>()`, and generic invocations (`CreateMap<A,B>()`,
/// `AddScoped<IFoo,Foo>()`) without a method-name allowlist. Nested generics
/// are skipped here because they are captured as `children` of the enclosing
/// usage — recording them again would double-count.
fn record_outermost_generic_type_arguments(
    base: &mut BaseExtractor,
    name_node: Node,
    identifier: &Identifier,
) {
    let Some(generic_name) = name_node.parent() else {
        return;
    };
    if generic_name.kind() != "generic_name" {
        return;
    }
    // A generic_name whose parent is a type_argument_list is itself nested
    // inside another generic — its args ride along under the outer usage.
    if generic_name
        .parent()
        .map(|p| p.kind() == "type_argument_list")
        .unwrap_or(false)
    {
        return;
    }
    let Some(arg_list) = type_argument_list_child(generic_name) else {
        return;
    };
    let arguments = crate::base::extract_type_arguments(base, arg_list, decompose_csharp_type_arg);
    base.record_type_arguments(identifier, arguments);
}

/// `TypeArgDecomposer` for C#: maps a child of a `type_argument_list` to its
/// applied argument. Skips punctuation (`<`, `,`, `>`); for a nested
/// `generic_name` returns the base name plus its inner `type_argument_list` to
/// recurse into; for every other type node returns its source text as a leaf.
fn decompose_csharp_type_arg<'a>(
    base: &BaseExtractor,
    node: Node<'a>,
) -> Option<(String, Option<Node<'a>>)> {
    if !node.is_named() {
        return None;
    }
    match node.kind() {
        "generic_name" => {
            let name = direct_identifier(base, node)
                .map(|(_, name)| name)
                .unwrap_or_else(|| base.get_node_text(&node));
            Some((name, type_argument_list_child(node)))
        }
        _ => Some((base.get_node_text(&node), None)),
    }
}

/// First `type_argument_list` child of a `generic_name` (its `<...>`), if any.
fn type_argument_list_child(generic_name: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = generic_name.walk();
    generic_name
        .children(&mut cursor)
        .find(|child| child.kind() == "type_argument_list")
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

// ============================================================================
// String-literal call-argument capture helpers (Miller bridge Phase 3)
// ============================================================================

/// Capture string-literal arguments of a C# `invocation_expression` as `Literal`
/// records. Config-free: `carrier` is the method name (generics stripped); the
/// URL/SQL classification and the carrier gate run later in the `src/` pipeline.
///
/// C# wraps each call argument in an `argument` node, so the value expression is
/// the argument's last named child (after any `name:` for a named argument).
/// `arg_position` is counted over the full argument list.
fn record_csharp_call_arg_literals(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    let Some(function) = node.child_by_field_name("function") else {
        return;
    };
    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };
    let carrier = csharp_carrier(base, function);
    let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

    let mut cursor = args.walk();
    for (pos, arg) in args.named_children(&mut cursor).enumerate() {
        let value = if arg.kind() == "argument" {
            let mut vc = arg.walk();
            arg.named_children(&mut vc).last()
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

/// Derive a C# call's carrier: the method name with generic type arguments
/// stripped (`conn.Query<User>` -> `Query`, `Foo<T>` -> `Foo`, `Execute` ->
/// `Execute`). The receiver is intentionally dropped — Dapper/ADO carriers are
/// matched by method name, and the receiver is usually a local variable.
fn csharp_carrier(base: &BaseExtractor, function: Node) -> Option<String> {
    let text = match function.kind() {
        "identifier" | "generic_name" => base.get_node_text(&function),
        "member_access_expression" => function
            .child_by_field_name("name")
            .map(|n| base.get_node_text(&n))?,
        _ => base.get_node_text(&function),
    };
    let stripped = match text.find('<') {
        Some(i) => text[..i].to_string(),
        None => text,
    };
    if stripped.is_empty() {
        None
    } else {
        Some(stripped)
    }
}
