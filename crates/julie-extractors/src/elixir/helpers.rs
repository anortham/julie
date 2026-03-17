/// Helper utilities for Elixir symbol extraction
use crate::base::BaseExtractor;
use tree_sitter::Node;

/// Find the first child of a given type
fn find_child_by_type<'a>(node: &Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
    }
    None
}

/// Extract the target name of a call node (e.g., "defmodule", "def", "use")
///
/// In tree-sitter-elixir, a call node has a `target` field which is typically
/// an `identifier` node containing the macro/function name.
pub(super) fn extract_call_target_name(base: &BaseExtractor, node: &Node) -> Option<String> {
    // `target` IS a named field in tree-sitter-elixir
    let target = node.child_by_field_name("target")?;
    if target.kind() == "identifier" {
        Some(base.get_node_text(&target))
    } else {
        None
    }
}

/// Extract module name from the first argument of defmodule.
/// The `arguments` node is a child type, NOT a named field.
pub(super) fn extract_module_name(base: &BaseExtractor, node: &Node) -> Option<String> {
    let args = find_child_by_type(node, "arguments")?;
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        if child.kind() == "alias" {
            return Some(base.get_node_text(&child));
        }
        if child.kind() == "dot" {
            return Some(base.get_node_text(&child));
        }
    }
    None
}

/// Extract function name and parameter string from a def/defp call.
///
/// The first argument of `def` is either:
/// - A `call` node (the function head): `def add(a, b)` -> call target="add", args="(a, b)"
/// - An `identifier` node for zero-arg functions: `def init` -> identifier "init"
/// - A `binary_operator` for guard clauses: `def validate(n) when is_number(n)` -> extract left
pub(super) fn extract_function_head(
    base: &BaseExtractor,
    node: &Node,
) -> Option<(String, Option<String>)> {
    let args = find_child_by_type(node, "arguments")?;
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        match child.kind() {
            "call" => {
                // The function head is itself a call: `add(a, b)`
                let fn_name_node = child.child_by_field_name("target")?;
                let fn_name = base.get_node_text(&fn_name_node);
                let params = find_child_by_type(&child, "arguments")
                    .map(|a| base.get_node_text(&a));
                return Some((fn_name, params));
            }
            "identifier" => {
                // Zero-arg function: `def init`
                return Some((base.get_node_text(&child), None));
            }
            "binary_operator" => {
                // Guard clause: `validate(n) when is_number(n)`
                // The left side is the function head
                if let Some(left) = child.child_by_field_name("left") {
                    match left.kind() {
                        "call" => {
                            let fn_name_node = left.child_by_field_name("target")?;
                            let fn_name = base.get_node_text(&fn_name_node);
                            let params = find_child_by_type(&left, "arguments")
                                .map(|a| base.get_node_text(&a));
                            return Some((fn_name, params));
                        }
                        "identifier" => {
                            return Some((base.get_node_text(&left), None));
                        }
                        _ => {}
                    }
                }
            }
            _ => continue,
        }
    }
    None
}

/// Find the do_block child of a call node
pub(super) fn extract_do_block<'a>(node: &Node<'a>) -> Option<Node<'a>> {
    find_child_by_type(node, "do_block")
}

/// Extract the value for a keyword argument from a keywords/keyword list.
/// Used for extracting `for: Bar` from `defimpl Foo, for: Bar`.
pub(super) fn extract_keyword_value(
    base: &BaseExtractor,
    node: &Node,
    key: &str,
) -> Option<String> {
    let args = find_child_by_type(node, "arguments")?;
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        if child.kind() == "keywords" {
            return extract_keyword_from_keywords(base, &child, key);
        }
    }
    None
}

fn extract_keyword_from_keywords(
    base: &BaseExtractor,
    keywords: &Node,
    key: &str,
) -> Option<String> {
    let mut cursor = keywords.walk();
    for pair in keywords.children(&mut cursor) {
        if pair.kind() == "pair" {
            // In tree-sitter-elixir, pair has `key` and `value` fields
            if let Some(pair_key) = pair.child_by_field_name("key") {
                let key_text = base.get_node_text(&pair_key);
                // Keywords in Elixir: "for:" or "for: " — strip colon and whitespace
                let cleaned = key_text.trim().trim_end_matches(':').trim();
                if cleaned == key {
                    if let Some(pair_val) = pair.child_by_field_name("value") {
                        return Some(base.get_node_text(&pair_val));
                    }
                }
            }
        }
    }
    None
}

/// Extract the protocol name from defimpl's first argument
pub(super) fn extract_impl_protocol_name(
    base: &BaseExtractor,
    node: &Node,
) -> Option<String> {
    let args = find_child_by_type(node, "arguments")?;
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        if child.kind() == "alias" {
            return Some(base.get_node_text(&child));
        }
    }
    None
}

/// Extract struct field names from defstruct's argument list.
/// Returns (field_name, start_byte, end_byte) tuples.
pub(super) fn extract_struct_fields(
    base: &BaseExtractor,
    node: &Node,
) -> Vec<(String, u32, u32)> {
    let mut fields = Vec::new();
    let Some(args) = find_child_by_type(node, "arguments") else {
        return fields;
    };

    collect_atom_fields(base, &args, &mut fields);
    fields
}

fn collect_atom_fields(
    base: &BaseExtractor,
    node: &Node,
    fields: &mut Vec<(String, u32, u32)>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "atom" {
            let text = base.get_node_text(&child);
            let name = text.trim_start_matches(':').to_string();
            if !name.is_empty() {
                fields.push((name, child.start_byte() as u32, child.end_byte() as u32));
            }
        } else {
            collect_atom_fields(base, &child, fields);
        }
    }
}

/// Extract the first argument of import/use/alias/require as the module name
pub(super) fn extract_import_target(base: &BaseExtractor, node: &Node) -> Option<String> {
    let args = find_child_by_type(node, "arguments")?;
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        match child.kind() {
            "alias" => return Some(base.get_node_text(&child)),
            "dot" => return Some(base.get_node_text(&child)),
            _ => continue,
        }
    }
    None
}
