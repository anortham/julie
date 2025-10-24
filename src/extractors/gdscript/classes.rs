//! Class extraction for GDScript

use super::helpers::find_child_by_type;
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract class_name statement (explicit class definition)
pub(super) fn extract_class_name_statement(
    base: &mut BaseExtractor,
    pending_inheritance: &HashMap<String, String>,
    node: Node,
    parent_id: Option<&String>,
) -> Option<Symbol> {
    let name_node = find_child_by_type(node, "name")?;
    let name = base.get_node_text(&name_node);

    // Include preceding annotations in signature
    let mut signature = base.get_node_text(&node);
    if let Some(parent) = node.parent() {
        // Look for annotations before this class_name_statement
        for i in 0..parent.child_count() {
            if let Some(child) = parent.child(i) {
                if child.kind() == "class_name_statement"
                    && base.get_node_text(&child) == base.get_node_text(&node)
                {
                    // Found our node, now look backwards for annotations
                    if i > 0 {
                        for j in (0..i).rev() {
                            if let Some(prev_child) = parent.child(j) {
                                if prev_child.kind() == "annotation" {
                                    let annotation_text = base.get_node_text(&prev_child);
                                    signature = format!("{}\n{}", annotation_text, signature);
                                    break;
                                }
                                if prev_child.kind() == "class_name_statement" {
                                    break;
                                }
                            }
                        }
                    }
                    break;
                }
            }
        }
    }

    let mut metadata = HashMap::new();
    if let Some(base_class_name) = pending_inheritance.get(&name) {
        metadata.insert(
            "baseClass".to_string(),
            Value::String(base_class_name.clone()),
        );
    }

    // Extract doc comment
    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.cloned(),
            metadata: if metadata.is_empty() {
                None
            } else {
                Some(metadata)
            },
            doc_comment,
        },
    ))
}

/// Extract inner class definition (`class Name: ...` syntax)
pub(super) fn extract_class_definition(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
) -> Option<Symbol> {
    // For `class` nodes, look for the name node in the parent's children
    let parent_node = node.parent()?;
    let mut name_node: Option<Node> = None;

    // Find the index of the current class node
    let mut class_index = None;
    for i in 0..parent_node.child_count() {
        if let Some(child) = parent_node.child(i) {
            if child.id() == node.id() {
                class_index = Some(i);
                break;
            }
        }
    }

    // Look for 'name' node after the 'class' node
    if let Some(idx) = class_index {
        for i in (idx + 1)..parent_node.child_count() {
            if let Some(child) = parent_node.child(i) {
                if child.kind() == "name" {
                    name_node = Some(child);
                    break;
                }
            }
        }
    }

    let name_node = name_node?;
    let name = base.get_node_text(&name_node);
    let signature = format!("class {}:", name);

    // Extract doc comment - try from the class node first, then from parent
    let mut doc_comment = base.find_doc_comment(&node);
    if doc_comment.is_none() && parent_node.kind() != "source" {
        // If not found on the class keyword, try the parent node (which contains the class block)
        doc_comment = base.find_doc_comment(&parent_node);
    }

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.cloned(),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Collect inheritance information from class_name and extends statements
pub(super) fn collect_inheritance_info(
    base: &mut BaseExtractor,
    node: Node,
    pending_inheritance: &mut HashMap<String, String>,
) {
    // Look for adjacent class_name_statement and extends_statement pairs
    for i in 0..node.child_count() {
        if let (Some(current_child), Some(next_child)) = (node.child(i), node.child(i + 1)) {
            // Check for class_name followed by extends
            if current_child.kind() == "class_name_statement"
                && next_child.kind() == "extends_statement"
            {
                if let (Some(name_node), Some(type_node)) = (
                    find_child_by_type(current_child, "name"),
                    find_child_by_type(next_child, "type"),
                ) {
                    let class_name = base.get_node_text(&name_node);
                    if let Some(identifier_node) = find_child_by_type(type_node, "identifier") {
                        let base_class_name = base.get_node_text(&identifier_node);
                        pending_inheritance.insert(class_name, base_class_name);
                    }
                }
            }

            // Check for extends followed by class_name (reverse order)
            if current_child.kind() == "extends_statement"
                && next_child.kind() == "class_name_statement"
            {
                if let (Some(type_node), Some(name_node)) = (
                    find_child_by_type(current_child, "type"),
                    find_child_by_type(next_child, "name"),
                ) {
                    let class_name = base.get_node_text(&name_node);
                    if let Some(identifier_node) = find_child_by_type(type_node, "identifier") {
                        let base_class_name = base.get_node_text(&identifier_node);
                        pending_inheritance.insert(class_name, base_class_name);
                    }
                }
            }
        }
    }

    // Recursively collect from children
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_inheritance_info(base, child, pending_inheritance);
        }
    }
}
