//! Shared helper functions for GDScript extraction

use crate::extractors::base::BaseExtractor;
use tree_sitter::Node;

/// Helper to find a child node of a specific type
pub(super) fn find_child_by_type<'a>(node: Node<'a>, child_type: &str) -> Option<Node<'a>> {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == child_type {
                return Some(child);
            }
        }
    }
    None
}

/// Helper to find multiple annotations preceding a node at the source level
pub(super) fn extract_variable_annotations(
    base: &mut BaseExtractor,
    parent_node: Node,
    signature: &str,
) -> (Vec<String>, String) {
    let mut annotations = Vec::new();
    let mut full_signature = signature.to_string();

    // Check for annotations as children
    for i in 0..parent_node.child_count() {
        if let Some(child) = parent_node.child(i) {
            if child.kind() == "annotations" {
                for j in 0..child.child_count() {
                    if let Some(annotation_child) = child.child(j) {
                        if annotation_child.kind() == "annotation" {
                            let annotation_text = base.get_node_text(&annotation_child);
                            annotations.push(annotation_text);
                        }
                    }
                }
            }
        }
    }

    // Also look for sibling annotations at source level
    if let Some(grandparent) = parent_node.parent() {
        // Find parent node index
        let mut node_index = None;
        for i in 0..grandparent.child_count() {
            if let Some(child) = grandparent.child(i) {
                if child.id() == parent_node.id() {
                    node_index = Some(i);
                    break;
                }
            }
        }

        if let Some(idx) = node_index {
            let mut annotation_texts = Vec::new();

            // Look backwards for annotations
            for i in (0..idx).rev() {
                if let Some(child) = grandparent.child(i) {
                    if child.kind() == "annotations" {
                        for j in 0..child.child_count() {
                            if let Some(annotation_child) = child.child(j) {
                                if annotation_child.kind() == "annotation" {
                                    let annotation_text = base.get_node_text(&annotation_child);
                                    annotations.push(annotation_text.clone());
                                    annotation_texts.insert(0, annotation_text);
                                }
                            }
                        }
                    } else if child.kind() == "annotation" {
                        let annotation_text = base.get_node_text(&child);
                        annotations.push(annotation_text.clone());
                        annotation_texts.insert(0, annotation_text);
                    } else if child.kind() == "variable_statement" || child.kind() == "var" {
                        break;
                    }
                }
            }

            // Build full signature with annotations
            if !annotation_texts.is_empty() {
                full_signature = format!("{}\n{}", annotation_texts.join("\n"), signature);
            }
        }
    }

    (annotations, full_signature)
}

/// Helper to get position key for deduplication
pub(super) fn get_position_key(node: Node) -> String {
    format!(
        "{}:{}:{}",
        node.start_position().row,
        node.start_position().column,
        node.kind()
    )
}
