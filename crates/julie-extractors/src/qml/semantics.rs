use super::QmlExtractor;
use crate::base::{BaseExtractor, Symbol, SymbolKind, UnresolvedTarget, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

pub(super) fn extract_qml_doc_comment(extractor: &QmlExtractor, node: &Node) -> Option<String> {
    extractor.base.extract_documentation(node).or_else(|| {
        let mut comments = Vec::new();
        let mut current = node.prev_named_sibling();
        while let Some(sibling) = current {
            if sibling.kind().contains("comment") {
                let text = extractor.base.get_node_text(&sibling);
                if is_qml_doc_comment(text.trim_start()) {
                    comments.push(text);
                }
                current = sibling.prev_named_sibling();
            } else {
                break;
            }
        }
        comments.reverse();
        if comments.is_empty() {
            None
        } else {
            Some(comments.join("\n"))
        }
    })
}

pub(super) fn infer_visibility(name: &str, force_private: bool) -> Visibility {
    if force_private || name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Public
    }
}

pub(super) fn is_signal_handler_binding_name(name: &str) -> bool {
    if let Some(rest) = name.strip_prefix("on") {
        return rest.chars().next().is_some_and(char::is_uppercase);
    }
    if let Some((_, suffix)) = name.rsplit_once(".on") {
        return suffix.chars().next().is_some_and(char::is_uppercase);
    }
    false
}

pub(super) fn handled_signal_from_binding_name(name: &str) -> Option<String> {
    if let Some((_, suffix)) = name.rsplit_once(".on") {
        return lowercase_first(suffix);
    }
    name.strip_prefix("on").and_then(lowercase_first)
}

pub(super) fn function_signature(node_text: String) -> String {
    node_text
        .split('{')
        .next()
        .unwrap_or(node_text.as_str())
        .trim()
        .to_string()
}

pub(super) fn infer_types(symbols: &[Symbol]) -> HashMap<String, String> {
    let mut types = HashMap::new();

    for symbol in symbols {
        let inferred = match symbol.kind {
            SymbolKind::Property => symbol
                .signature
                .as_deref()
                .and_then(infer_property_type_from_signature),
            SymbolKind::Function => symbol
                .signature
                .as_deref()
                .and_then(infer_function_return_type_from_signature),
            _ => None,
        };

        if let Some(inferred_type) = inferred {
            types.insert(symbol.id.clone(), inferred_type);
        }
    }

    types
}

/// Returns `true` if `node` is inside a `ui_object_definition_binding` ancestor
/// (i.e. inside a property-value-source block like `PropertyAnimation on value { ... }`).
pub(super) fn is_inside_object_definition_binding(node: Node<'_>) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "ui_object_definition_binding" {
            return true;
        }
        current = parent;
    }
    false
}

pub(super) fn build_unresolved_target(
    base: &BaseExtractor,
    function_node: Node,
    fallback_name: &str,
) -> UnresolvedTarget {
    if function_node.kind() == "member_expression" {
        let receiver = function_node
            .child_by_field_name("object")
            .map(|node| base.get_node_text(&node));
        let property = function_node
            .child_by_field_name("property")
            .map(|node| base.get_node_text(&node))
            .unwrap_or_else(|| fallback_name.to_string());
        let display_name = receiver
            .as_ref()
            .map(|receiver| format!("{receiver}.{property}"))
            .unwrap_or_else(|| property.clone());
        return UnresolvedTarget {
            display_name,
            terminal_name: property,
            receiver,
            namespace_path: Vec::new(),
            import_context: None,
        };
    }

    UnresolvedTarget::simple(fallback_name.to_string())
}

fn is_qml_doc_comment(trimmed: &str) -> bool {
    trimmed.starts_with("/**") || trimmed.starts_with("///")
}

fn lowercase_first(value: &str) -> Option<String> {
    let mut chars = value.chars();
    let first = chars.next()?;
    Some(format!("{}{}", first.to_lowercase(), chars.as_str()))
}

fn infer_property_type_from_signature(signature: &str) -> Option<String> {
    let tokens = signature.split_whitespace().collect::<Vec<_>>();
    let property_idx = tokens.iter().position(|token| *token == "property")?;
    let property_type = *tokens.get(property_idx + 1)?;

    if property_type == "alias" {
        return None;
    }

    Some(property_type.trim_end_matches(':').to_string())
}

fn infer_function_return_type_from_signature(signature: &str) -> Option<String> {
    let trimmed = signature.trim();
    if !trimmed.starts_with("function") {
        return None;
    }

    let (_, return_type) = trimmed.rsplit_once(':')?;
    Some(return_type.trim().trim_end_matches('{').to_string())
}
