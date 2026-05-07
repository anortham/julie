// PHP Extractor - Property and constant extraction

use super::{
    PhpExtractor, determine_visibility, extract_modifiers, find_child,
    functions::extract_attribute_markers,
};
use crate::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract PHP property declarations
pub(super) fn extract_property(
    extractor: &mut PhpExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let is_promoted = node.kind() == "property_promotion_parameter";

    // Extract property name from either a property declaration element or promoted parameter field.
    let property_element = if is_promoted {
        None
    } else {
        Some(find_child(extractor, &node, "property_element")?)
    };
    let name_node = if is_promoted {
        promoted_parameter_name_node(extractor, &node)?
    } else {
        find_child(extractor, property_element.as_ref()?, "variable_name")?
    };
    let name = extractor.get_base().get_node_text(&name_node);

    let modifiers = extract_modifiers(extractor, &node);
    let annotations = extract_attribute_markers(extractor, &node);
    let type_node = if is_promoted {
        node.child_by_field_name("type")
            .or_else(|| find_type_node(extractor, &node))
    } else {
        find_type_node(extractor, &node)
    };
    let attribute_list = if is_promoted {
        node.child_by_field_name("attributes")
            .or_else(|| find_child(extractor, &node, "attribute_list"))
    } else {
        find_child(extractor, &node, "attribute_list")
    };

    // Check for default value assignment
    let property_value = if let Some(property_element) = property_element.as_ref() {
        extract_property_value(extractor, property_element)
    } else {
        node.child_by_field_name("default_value")
            .map(|default| extractor.get_base().get_node_text(&default))
    };

    // Build signature in correct order: attributes + modifiers + type + name + value
    let mut signature = String::new();

    // Add attributes if present
    if let Some(attr_node) = attribute_list {
        signature.push_str(&extractor.get_base().get_node_text(&attr_node));
        signature.push('\n');
    }

    if !modifiers.is_empty() {
        signature.push_str(&format!("{} ", modifiers.join(" ")));
    }

    if let Some(type_node) = type_node {
        signature.push_str(&format!(
            "{} ",
            extractor.get_base().get_node_text(&type_node)
        ));
    }

    signature.push_str(&name);

    if let Some(value) = property_value {
        signature.push_str(&format!(" = {}", value));
    }

    let mut metadata = HashMap::new();
    metadata.insert("type".to_string(), "property".to_string());
    metadata.insert("modifiers".to_string(), modifiers.join(","));

    if let Some(type_node) = type_node {
        metadata.insert(
            "propertyType".to_string(),
            extractor.get_base().get_node_text(&type_node),
        );
    }

    // Extract PHPDoc comment
    let doc_comment = extractor.get_base().find_doc_comment(&node);
    let resolved_parent_id = resolve_property_parent_id(extractor, parent_id);

    Some(
        extractor.get_base_mut().create_symbol(
            &node,
            name.replace('$', ""), // Remove $ from property name
            SymbolKind::Property,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(determine_visibility(&modifiers)),
                parent_id: resolved_parent_id,
                metadata: Some(
                    metadata
                        .into_iter()
                        .map(|(k, v)| (k, serde_json::Value::String(v)))
                        .collect(),
                ),
                doc_comment,
                annotations,
            },
        ),
    )
}

fn promoted_parameter_name_node<'a>(extractor: &PhpExtractor, node: &Node<'a>) -> Option<Node<'a>> {
    let name_node = node.child_by_field_name("name")?;
    if name_node.kind() == "variable_name" {
        return Some(name_node);
    }
    find_child(extractor, &name_node, "variable_name")
}

fn resolve_property_parent_id(extractor: &PhpExtractor, parent_id: Option<&str>) -> Option<String> {
    let parent_id = parent_id?;
    let Some(parent_symbol) = extractor.get_base().symbol_map.get(parent_id) else {
        return Some(parent_id.to_string());
    };

    if parent_symbol.kind == SymbolKind::Constructor {
        return parent_symbol
            .parent_id
            .clone()
            .or_else(|| Some(parent_id.to_string()));
    }

    Some(parent_id.to_string())
}

/// Extract PHP constant declarations
pub(super) fn extract_constant(
    extractor: &mut PhpExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // First pass: extract all text values we need before any borrowing operations
    let const_element = find_child(extractor, &node, "const_element")?;
    let name_node = find_child(extractor, &const_element, "name")?;

    // Extract all text values immediately
    let name = { extractor.get_base().get_node_text(&name_node) };

    // Extract value text immediately
    let value = {
        let mut cursor = const_element.walk();
        let mut found_assignment = false;
        let mut val = None;

        for child in const_element.children(&mut cursor) {
            if found_assignment {
                val = Some(extractor.get_base().get_node_text(&child));
                break;
            }
            if child.kind() == "=" {
                found_assignment = true;
            }
        }
        val
    };

    // Extract modifiers and visibility immediately
    let (visibility, annotations) = {
        let modifiers = extract_modifiers(extractor, &node);
        (
            determine_visibility(&modifiers),
            extract_attribute_markers(extractor, &node),
        )
    };

    // Now all borrows are complete - build the symbol
    let mut signature = format!(
        "{} const {}",
        match visibility {
            Visibility::Public => "public",
            Visibility::Private => "private",
            Visibility::Protected => "protected",
            Visibility::Internal => "internal",
            Visibility::FilePrivate => "fileprivate",
            Visibility::Open => "open",
        },
        name
    );

    if let Some(val) = &value {
        signature.push_str(&format!(" = {}", val));
    }

    let mut metadata = HashMap::new();
    metadata.insert("type".to_string(), "constant".to_string());
    if let Some(val) = value {
        metadata.insert("value".to_string(), val);
    }

    // Extract PHPDoc comment
    let doc_comment = extractor.get_base().find_doc_comment(&node);

    Some(
        extractor.get_base_mut().create_symbol(
            &node,
            name,
            SymbolKind::Constant,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(
                    metadata
                        .into_iter()
                        .map(|(k, v)| (k, serde_json::Value::String(v)))
                        .collect(),
                ),
                doc_comment,
                annotations,
            },
        ),
    )
}

/// Find type node in property declaration
pub(super) fn find_type_node<'a>(_extractor: &PhpExtractor, node: &Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "type" | "primitive_type" | "optional_type" | "named_type" => return Some(child),
            _ => {}
        }
    }
    None
}

/// Extract property default value
pub(super) fn extract_property_value(
    extractor: &PhpExtractor,
    property_element: &Node,
) -> Option<String> {
    let mut cursor = property_element.walk();
    let mut found_assignment = false;

    for child in property_element.children(&mut cursor) {
        if found_assignment {
            return Some(extractor.get_base().get_node_text(&child));
        }
        if child.kind() == "=" {
            found_assignment = true;
        }
    }
    None
}
