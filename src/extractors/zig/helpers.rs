use crate::extractors::base::BaseExtractor;
use tree_sitter::Node;

/// Helper methods for Zig extractor - visibility, context checking, and AST navigation
pub(super) fn is_public_function(base: &BaseExtractor, node: Node) -> bool {
    // Check for "pub" keyword as first child of function
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "pub" || base.get_node_text(&child) == "pub" {
            return true;
        }
    }

    // Also check for "pub" keyword before function (fallback)
    if let Some(prev) = node.prev_sibling() {
        if prev.kind() == "pub" || base.get_node_text(&prev) == "pub" {
            return true;
        }
    }

    false
}

pub(super) fn is_export_function(base: &BaseExtractor, node: Node) -> bool {
    // Check for "export" keyword as first child of function
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "export" || base.get_node_text(&child) == "export" {
            return true;
        }
    }

    // Also check for "export" keyword before function (fallback)
    if let Some(prev) = node.prev_sibling() {
        if prev.kind() == "export" || base.get_node_text(&prev) == "export" {
            return true;
        }
    }

    false
}

pub(super) fn is_public_declaration(base: &BaseExtractor, node: Node) -> bool {
    // Check for "pub" keyword before declaration
    if let Some(prev) = node.prev_sibling() {
        if prev.kind() == "pub" || base.get_node_text(&prev) == "pub" {
            return true;
        }
    }
    false
}

pub(super) fn is_inside_struct(node: Node) -> bool {
    // Walk up the tree to see if we're inside a struct declaration
    let mut current = node.parent();
    while let Some(parent) = current {
        match parent.kind() {
            "struct_declaration" | "container_declaration" | "enum_declaration" => {
                return true;
            }
            _ => {
                current = parent.parent();
            }
        }
    }
    false
}
