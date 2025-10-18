// PHP Extractor - Helper utilities
// Common functions for AST navigation and type inspection

use super::PhpExtractor;
use crate::extractors::base::Visibility;
use tree_sitter::Node;

/// Helper method to find child node by type
#[allow(clippy::manual_find)] // Manual loop required for borrow checker
pub(super) fn find_child<'a>(
    _extractor: &PhpExtractor,
    node: &Node<'a>,
    child_type: &str,
) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == child_type {
            return Some(child);
        }
    }
    None
}

/// Helper method to find child node text by type
pub(super) fn find_child_text(
    extractor: &PhpExtractor,
    node: &Node,
    child_type: &str,
) -> Option<String> {
    find_child(extractor, node, child_type).map(|child| extractor.get_base().get_node_text(&child))
}

/// Extract modifiers from PHP nodes
pub(super) fn extract_modifiers(extractor: &PhpExtractor, node: &Node) -> Vec<String> {
    let mut modifiers = Vec::new();
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        match child.kind() {
            "visibility_modifier" => modifiers.push(extractor.get_base().get_node_text(&child)),
            "abstract_modifier" => modifiers.push("abstract".to_string()),
            "static_modifier" => modifiers.push("static".to_string()),
            "final_modifier" => modifiers.push("final".to_string()),
            "readonly_modifier" => modifiers.push("readonly".to_string()),
            "public" | "private" | "protected" | "static" | "abstract" | "final" | "readonly" => {
                modifiers.push(extractor.get_base().get_node_text(&child));
            }
            _ => {}
        }
    }

    modifiers
}

/// Determine visibility from modifiers
pub(super) fn determine_visibility(modifiers: &[String]) -> Visibility {
    for modifier in modifiers {
        match modifier.as_str() {
            "private" => return Visibility::Private,
            "protected" => return Visibility::Protected,
            _ => {}
        }
    }
    Visibility::Public // PHP defaults to public
}
