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
            // Phase 3: capture string-literal call-arguments (config-free; the
            // carrier classification + gate happen in the src/ pipeline).
            record_vbnet_call_arg_literals(base, node, symbol_map);
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

// ============================================================================
// String-literal call-argument capture (Miller bridge Phase 3)
// ============================================================================

/// Capture string-literal arguments of a VB.NET `invocation` as `Literal`
/// records. Config-free: `carrier` is the invoked method name (mirrors the C#
/// leg); the URL/SQL classification and the carrier gate run later in the
/// `src/` pipeline. VB wraps each call argument in an `argument` node, so the
/// value expression is the argument's last named child (after any `name:=` for a
/// named argument). `arg_position` is counted over the full argument list.
fn record_vbnet_call_arg_literals(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    let Some(target) = node.child_by_field_name("target") else {
        return;
    };
    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };
    let carrier = vbnet_carrier(base, target);
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
            if let Some(text) = decode_vbnet_literal(base, &value) {
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

/// Derive a VB.NET call's carrier: the invoked method name (generics, if any,
/// stripped). `target` is an `identifier` (bare call) or a `member_access`
/// whose `member` field is the method name. The receiver is dropped — .NET
/// HTTP/DB carriers are matched by bare method name via the gate's last-segment
/// rule (`conn.Execute` -> `execute`), and the receiver is usually a local var.
fn vbnet_carrier(base: &BaseExtractor, target: Node) -> Option<String> {
    let text = match target.kind() {
        "identifier" => base.get_node_text(&target),
        "member_access" | "member_access_expression" => target
            .child_by_field_name("member")
            .or_else(|| target.child_by_field_name("name"))
            .map(|n| base.get_node_text(&n))?,
        _ => base.get_node_text(&target),
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

/// Decode a VB.NET call-argument string for capture.
///
/// Plain strings (`"..."`) are a single flat `string_literal` token, handled by
/// the shared `decode_string_literal` delimiter-strip fallback. Interpolated
/// strings appear as an `interpolated_string_literal` (either directly as the
/// argument value or wrapped in a `string_literal` choice node). Their static
/// text segments are **anonymous** tokens the base decoder's named-children walk
/// cannot see, so they are decoded here to the shared `{}`-hole convention
/// (`$"u/{id}"` -> `u/{}`), with escaped `""`/`{{`/`}}` resolved.
fn decode_vbnet_literal(base: &BaseExtractor, value: &Node) -> Option<String> {
    let interp = if value.kind() == "interpolated_string_literal" {
        Some(*value)
    } else if value.kind() == "string_literal" {
        let mut cursor = value.walk();
        value
            .named_children(&mut cursor)
            .find(|n| n.kind() == "interpolated_string_literal")
    } else {
        None
    };
    if let Some(interp) = interp {
        return Some(decode_vbnet_interpolated(base, &interp));
    }
    base.decode_string_literal(value)
}

/// Decode a VB.NET `interpolated_string_literal` to the `{}`-hole convention by
/// reconstructing from source: the gaps between `interpolation` children are
/// filled verbatim from the file bytes and each interpolation becomes `{}`. This
/// is robust whether or not the grammar exposes the (anonymous) static text
/// segments as child nodes. The `$"` opener / `"` closer are then stripped and
/// `""`/`{{`/`}}` escapes resolved.
fn decode_vbnet_interpolated(base: &BaseExtractor, interp: &Node) -> String {
    let bytes = base.content.as_bytes();
    let total_end = interp.end_byte().min(bytes.len());
    let mut out = String::new();
    let mut pos = interp.start_byte().min(total_end);
    let mut cursor = interp.walk();
    for child in interp.named_children(&mut cursor) {
        if child.kind() != "interpolation" {
            continue;
        }
        let cs = child.start_byte().min(total_end);
        if cs > pos {
            out.push_str(&String::from_utf8_lossy(&bytes[pos..cs]));
        }
        out.push_str("{}");
        pos = child.end_byte().min(total_end);
    }
    if pos < total_end {
        out.push_str(&String::from_utf8_lossy(&bytes[pos..total_end]));
    }
    let mut s = out.as_str();
    s = s.strip_prefix("$\"").unwrap_or(s);
    s = s.strip_suffix('"').unwrap_or(s);
    s.replace("\"\"", "\"").replace("{{", "{").replace("}}", "}")
}
