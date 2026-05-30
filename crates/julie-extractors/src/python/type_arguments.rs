use super::PythonExtractor;
use crate::base::{BaseExtractor, Identifier, TypeArgument};
use tree_sitter::Node;

/// If `value_node` is the type-name identifier of an outermost generic, record
/// the generic's ordered/nested type arguments against `identifier`.
pub(super) fn record_outermost_python_type_arguments(
    extractor: &mut PythonExtractor,
    value_node: Node<'_>,
    identifier: &Identifier,
) {
    let Some(parent) = value_node.parent() else {
        return;
    };

    let arguments = match parent.kind() {
        "generic_type" => {
            if is_generic_type_nested_in_args(parent) {
                return;
            }
            collect_generic_type_args(extractor.base(), parent)
        }
        "subscript" => {
            let is_value = parent
                .child_by_field_name("value")
                .map(|v| v.id() == value_node.id())
                .unwrap_or(false);
            if !is_value || is_subscript_nested_in_args(parent) {
                return;
            }
            collect_subscript_type_args(extractor.base(), parent)
        }
        _ => return,
    };

    extractor
        .base_mut()
        .record_type_arguments(identifier, arguments);
}

fn is_generic_type_nested_in_args(generic_type_node: Node<'_>) -> bool {
    let Some(type_wrapper) = generic_type_node.parent() else {
        return false;
    };
    if type_wrapper.kind() != "type" {
        return false;
    }
    type_wrapper
        .parent()
        .map(|p| p.kind() == "type_parameter")
        .unwrap_or(false)
}

fn is_subscript_nested_in_args(subscript_node: Node<'_>) -> bool {
    subscript_node
        .parent()
        .map(|p| p.kind() == "subscript")
        .unwrap_or(false)
}

fn collect_generic_type_args(
    base: &BaseExtractor,
    generic_type_node: Node<'_>,
) -> Vec<TypeArgument> {
    let mut outer_cursor = generic_type_node.walk();
    let type_param = match generic_type_node
        .named_children(&mut outer_cursor)
        .find(|n| n.kind() == "type_parameter")
    {
        Some(tp) => tp,
        None => return Vec::new(),
    };

    let mut result = Vec::new();
    let mut ordinal = 0u32;
    let mut cursor = type_param.walk();

    for type_wrapper in type_param.named_children(&mut cursor) {
        if type_wrapper.kind() != "type" {
            continue;
        }
        let mut inner_cursor = type_wrapper.walk();
        let Some(inner) = type_wrapper.named_children(&mut inner_cursor).next() else {
            continue;
        };
        if let Some(arg) = python_type_expr_to_type_arg(base, inner, ordinal) {
            ordinal += 1;
            result.push(arg);
        }
    }
    result
}

fn collect_subscript_type_args(
    base: &BaseExtractor,
    subscript_node: Node<'_>,
) -> Vec<TypeArgument> {
    let mut result = Vec::new();
    let mut ordinal = 0u32;
    let mut cursor = subscript_node.walk();

    for child in subscript_node.children_by_field_name("subscript", &mut cursor) {
        if let Some(arg) = python_subscript_arg_to_type_arg(base, child, ordinal) {
            ordinal += 1;
            result.push(arg);
        }
    }
    result
}

fn python_type_expr_to_type_arg(
    base: &BaseExtractor,
    node: Node<'_>,
    ordinal: u32,
) -> Option<TypeArgument> {
    match node.kind() {
        "generic_type" => {
            let mut cursor = node.walk();
            let name_node = node
                .named_children(&mut cursor)
                .find(|n| n.kind() == "identifier")?;
            let type_name = base.get_node_text(&name_node);
            let children = collect_generic_type_args(base, node);
            Some(TypeArgument {
                ordinal,
                type_name,
                children,
            })
        }
        _ => Some(TypeArgument {
            ordinal,
            type_name: base.get_node_text(&node),
            children: Vec::new(),
        }),
    }
}

fn python_subscript_arg_to_type_arg(
    base: &BaseExtractor,
    node: Node<'_>,
    ordinal: u32,
) -> Option<TypeArgument> {
    if !node.is_named() {
        return None;
    }
    match node.kind() {
        "subscript" => {
            let value = node.child_by_field_name("value")?;
            let type_name = base.get_node_text(&value);
            let children = collect_subscript_type_args(base, node);
            Some(TypeArgument {
                ordinal,
                type_name,
                children,
            })
        }
        _ => Some(TypeArgument {
            ordinal,
            type_name: base.get_node_text(&node),
            children: Vec::new(),
        }),
    }
}
