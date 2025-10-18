//! Type extraction and inference for GDScript

use super::helpers::find_child_by_type;
use crate::extractors::base::BaseExtractor;
use tree_sitter::Node;

/// Extract data type from a variable node
pub(super) fn extract_variable_type(
    base: &mut BaseExtractor,
    parent_node: Node,
    name_node: &Node,
) -> Option<String> {
    // Look for type annotation as sibling after the name
    let mut name_index = None;
    for i in 0..parent_node.child_count() {
        if let Some(child) = parent_node.child(i) {
            if child.id() == name_node.id() {
                name_index = Some(i);
                break;
            }
        }
    }

    let name_index = name_index?;

    // Look for type annotation after name
    for i in (name_index + 1)..parent_node.child_count() {
        if let Some(child) = parent_node.child(i) {
            if child.kind() == "type" {
                if let Some(identifier_node) = find_child_by_type(child, "identifier") {
                    return Some(base.get_node_text(&identifier_node));
                } else {
                    // Handle complex types (e.g., Array[String])
                    return Some(base.get_node_text(&child).trim().to_string());
                }
            }
        }
    }

    // If no explicit type, try to infer from assignment
    for i in (name_index + 1)..parent_node.child_count() {
        if let Some(child) = parent_node.child(i) {
            if child.kind() == "=" {
                if let Some(value_node) = parent_node.child(i + 1) {
                    return Some(infer_type_from_expression(base, value_node));
                }
            }
        }
    }

    None
}

/// Infer type from an expression node
pub(super) fn infer_type_from_expression(base: &mut BaseExtractor, node: Node) -> String {
    match node.kind() {
        "string" => "String".to_string(),
        "integer" => "int".to_string(),
        "float" => "float".to_string(),
        "true" | "false" => "bool".to_string(),
        "null" => "null".to_string(),
        "identifier" => {
            let text = base.get_node_text(&node);
            if text.starts_with('$') || text.contains("Node") {
                text.replace('$', "")
            } else {
                "unknown".to_string()
            }
        }
        "call_expression" => {
            if let Some(callee_node) = find_child_by_type(node, "identifier") {
                let callee_text = base.get_node_text(&callee_node);
                // Common Godot constructors
                if ["Vector2", "Vector3", "Color", "Rect2", "Transform2D"]
                    .contains(&callee_text.as_str())
                {
                    return callee_text;
                }
            }
            "unknown".to_string()
        }
        _ => "unknown".to_string(),
    }
}
