/// Module attribute extraction for Elixir.
///
/// Handles @type, @typep, @opaque, @callback, @spec, @behaviour, @moduledoc, @doc.
/// In tree-sitter-elixir, attributes parse as `unary_operator` with `@` operator.
use super::ElixirExtractor;
use crate::base::{find_child_by_type, Symbol, SymbolKind, SymbolOptions, Visibility};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract a module attribute from a unary_operator node with `@` operator.
pub(super) fn extract_attribute(
    extractor: &mut ElixirExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Verify this is an @ operator
    let operator = node.child_by_field_name("operator")?;
    if extractor.base.get_node_text(&operator) != "@" {
        return None;
    }

    let operand = node.child_by_field_name("operand")?;

    match operand.kind() {
        "call" => extract_attribute_call(extractor, node, &operand, parent_id),
        _ => None,
    }
}

fn extract_attribute_call(
    extractor: &mut ElixirExtractor,
    attr_node: &Node,
    call_node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let target = call_node.child_by_field_name("target")?;
    let attr_name = extractor.base.get_node_text(&target);

    match attr_name.as_str() {
        "type" | "typep" | "opaque" => {
            extract_type_attribute(extractor, attr_node, call_node, parent_id, &attr_name)
        }
        "callback" => extract_callback_attribute(extractor, attr_node, call_node, parent_id),
        "spec" => {
            extract_spec_attribute(extractor, call_node);
            None // @spec doesn't create a standalone symbol
        }
        "behaviour" | "behavior" => {
            extract_behaviour_attribute(extractor, attr_node, call_node, parent_id)
        }
        "moduledoc" | "doc" => None,
        _ => None,
    }
}

fn extract_type_attribute(
    extractor: &mut ElixirExtractor,
    attr_node: &Node,
    call_node: &Node,
    parent_id: Option<&str>,
    attr_name: &str,
) -> Option<Symbol> {
    // NOTE: `arguments` is a child type, NOT a named field
    let args = find_child_by_type(call_node, "arguments")?;
    let type_text = extractor.base.get_node_text(&args);

    // Extract the type name (before ::)
    let type_name = type_text.split("::").next()?.trim().to_string();
    if type_name.is_empty() {
        return None;
    }

    let visibility = if attr_name == "typep" {
        Visibility::Private
    } else {
        Visibility::Public
    };

    let signature = format!("@{} {}", attr_name, extractor.base.get_node_text(call_node));

    Some(extractor.base.create_symbol(
        attr_node,
        type_name,
        SymbolKind::Type,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment: None,
        },
    ))
}

fn extract_callback_attribute(
    extractor: &mut ElixirExtractor,
    attr_node: &Node,
    call_node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let args = find_child_by_type(call_node, "arguments")?;
    let callback_text = extractor.base.get_node_text(&args);

    // Extract callback name (before `(` or `::`)
    let callback_name = callback_text
        .split(|c: char| c == '(' || c == ':')
        .next()?
        .trim()
        .to_string();
    if callback_name.is_empty() {
        return None;
    }

    let signature = format!("@callback {}", extractor.base.get_node_text(call_node));

    let mut metadata = HashMap::new();
    metadata.insert("callback".to_string(), Value::Bool(true));

    Some(extractor.base.create_symbol(
        attr_node,
        callback_name,
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: Some(metadata),
            doc_comment: None,
        },
    ))
}

fn extract_spec_attribute(extractor: &mut ElixirExtractor, call_node: &Node) {
    let Some(args) = find_child_by_type(call_node, "arguments") else {
        return;
    };
    let spec_text = extractor.base.get_node_text(&args);

    // Extract function name from spec text (before the `(`)
    let fn_name = spec_text
        .split('(')
        .next()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    if !fn_name.is_empty() {
        // Store the return type (after ::)
        if let Some(return_type) = spec_text.split("::").last() {
            extractor
                .specs
                .insert(fn_name, return_type.trim().to_string());
        }
    }
}

fn extract_behaviour_attribute(
    extractor: &mut ElixirExtractor,
    attr_node: &Node,
    call_node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let args = find_child_by_type(call_node, "arguments")?;
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        if child.kind() == "alias" {
            let behaviour_name = extractor.base.get_node_text(&child);
            let signature = format!("@behaviour {}", behaviour_name);
            return Some(extractor.base.create_symbol(
                attr_node,
                behaviour_name,
                SymbolKind::Import,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(Visibility::Public),
                    parent_id: parent_id.map(String::from),
                    metadata: None,
                    doc_comment: None,
                },
            ));
        }
    }
    None
}
