//! Variable and constant extraction for GDScript

use super::helpers::{extract_variable_annotations, find_child_by_type};
use super::types::extract_variable_type;
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract variable statement (var declarations)
pub(super) fn extract_variable_statement(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
) -> Option<Symbol> {
    let parent_node = node.parent()?;
    let mut name_node = None;

    // Find var index and look for name after it
    for i in 0..parent_node.child_count() {
        if let Some(child) = parent_node.child(i) {
            if child.id() == node.id() {
                // Found var node, look for name after it
                for j in (i + 1)..parent_node.child_count() {
                    if let Some(sibling) = parent_node.child(j) {
                        if sibling.kind() == "name" {
                            name_node = Some(sibling);
                            break;
                        }
                    }
                }
                break;
            }
        }
    }

    let name_node = name_node?;
    let name = base.get_node_text(&name_node);
    let signature = base.get_node_text(&parent_node);

    // Extract annotations and determine properties
    let (annotations, full_signature) = extract_variable_annotations(base, parent_node, &signature);
    let is_exported = annotations.iter().any(|a| a.starts_with("@export"));
    let is_onready = annotations.iter().any(|a| a.starts_with("@onready"));

    // Determine data type
    let data_type = extract_variable_type(base, parent_node, &name_node)
        .unwrap_or_else(|| "unknown".to_string());

    // Determine visibility
    let visibility = if is_exported {
        Visibility::Public
    } else {
        Visibility::Private
    };

    let mut metadata = HashMap::new();
    metadata.insert("dataType".to_string(), Value::String(data_type));
    if !annotations.is_empty() {
        let annotations_json = annotations
            .iter()
            .map(|a| Value::String(a.clone()))
            .collect::<Vec<_>>();
        metadata.insert("annotations".to_string(), Value::Array(annotations_json));
    }
    metadata.insert("isExported".to_string(), Value::Bool(is_exported));
    metadata.insert("isOnReady".to_string(), Value::Bool(is_onready));

    // Extract doc comment
    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Field,
        SymbolOptions {
            signature: Some(full_signature),
            visibility: Some(visibility),
            parent_id: parent_id.cloned(),
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

/// Extract constant statement (const declarations)
pub(super) fn extract_constant_statement(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
) -> Option<Symbol> {
    let parent_node = node.parent()?;
    let mut name_node = None;

    // Find const index and look for name after it
    for i in 0..parent_node.child_count() {
        if let Some(child) = parent_node.child(i) {
            if child.kind() == "const" {
                // Found const node, look for name after it
                for j in (i + 1)..parent_node.child_count() {
                    if let Some(sibling) = parent_node.child(j) {
                        if sibling.kind() == "name" {
                            name_node = Some(sibling);
                            break;
                        }
                    }
                }
                break;
            }
        }
    }

    let name_node = name_node?;
    let name = base.get_node_text(&name_node);
    let signature = base.get_node_text(&parent_node);

    // Get type annotation
    let data_type = extract_variable_type(base, parent_node, &name_node)
        .unwrap_or_else(|| "unknown".to_string());

    let mut metadata = HashMap::new();
    metadata.insert("dataType".to_string(), Value::String(data_type));

    // Extract doc comment
    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Constant,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.cloned(),
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

/// Extract variable from variable_statement node
pub(super) fn extract_variable_from_statement(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
    symbols: &[Symbol],
) -> Option<Symbol> {
    // For variable_statement nodes, find the var child and extract from there
    let var_node = find_child_by_type(node, "var")?;

    // Check if we should use class_name class as parent instead of implicit class
    let actual_parent_id = if let Some(node_parent) = node.parent() {
        if node_parent.kind() == "source" {
            // Find the closest preceding class_name statement
            find_closest_class_name_parent(base, node, parent_id, symbols)
                .unwrap_or_else(|| parent_id.cloned().unwrap_or_default())
        } else {
            parent_id.cloned().unwrap_or_default()
        }
    } else {
        parent_id.cloned().unwrap_or_default()
    };

    extract_variable_statement(base, var_node, Some(&actual_parent_id))
}

/// Find the closest preceding class_name parent for proper scope assignment
pub(super) fn find_closest_class_name_parent(
    base: &mut BaseExtractor,
    node: Node,
    default_parent: Option<&String>,
    symbols: &[Symbol],
) -> Option<String> {
    let source_parent = node.parent()?;
    let class_name_classes: Vec<_> = symbols
        .iter()
        .filter(|s| {
            s.kind == SymbolKind::Class
                && s.signature
                    .as_ref()
                    .map(|sig| sig.contains("class_name"))
                    .unwrap_or(false)
                && s.parent_id == default_parent.cloned()
        })
        .collect();

    if class_name_classes.is_empty() {
        return None;
    }

    // Find variable's position in source children
    let mut var_index = None;
    for i in 0..source_parent.child_count() {
        if let Some(child) = source_parent.child(i) {
            if child.kind() == "variable_statement"
                && child.start_position().row == node.start_position().row
                && child.start_position().column == node.start_position().column
            {
                var_index = Some(i);
                break;
            }
        }
    }

    let var_index = var_index?;

    // Find the last class_name_statement before this variable
    for i in (0..var_index).rev() {
        if let Some(child) = source_parent.child(i) {
            if child.kind() == "class_name_statement" {
                if let Some(name_node) = find_child_by_type(child, "name") {
                    let class_name = base.get_node_text(&name_node);
                    if let Some(matching_class) =
                        class_name_classes.iter().find(|c| c.name == class_name)
                    {
                        return Some(matching_class.id.clone());
                    }
                }
            }
        }
    }

    None
}
