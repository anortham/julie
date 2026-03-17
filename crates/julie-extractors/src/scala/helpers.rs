//! Helper functions for Scala symbol extraction
//!
//! Utility functions for extracting modifiers, visibility, type parameters,
//! and other metadata from Scala AST nodes.

use crate::base::Visibility;
use tree_sitter::Node;

type Base = super::super::base::BaseExtractor;

/// Extract modifiers from a Scala node (abstract, sealed, case, private, etc.)
pub(super) fn extract_modifiers(base: &Base, node: &Node) -> Vec<String> {
    let mut modifiers = Vec::new();
    for child in node.children(&mut node.walk()) {
        if child.kind() == "modifiers" || child.kind() == "annotation" {
            // Walk into modifiers container
            for mod_child in child.children(&mut child.walk()) {
                let text = base.get_node_text(&mod_child);
                if is_modifier_keyword(&text) || text.starts_with('@') {
                    modifiers.push(text);
                }
            }
        }
        // Some modifiers may appear as direct children
        let text = base.get_node_text(&child);
        if child.kind() != "modifiers" && is_direct_modifier(child.kind()) {
            modifiers.push(text);
        }
    }
    modifiers
}

fn is_modifier_keyword(s: &str) -> bool {
    matches!(
        s,
        "abstract"
            | "sealed"
            | "case"
            | "private"
            | "protected"
            | "override"
            | "final"
            | "lazy"
            | "implicit"
            | "inline"
            | "open"
            | "opaque"
            | "transparent"
            | "erased"
            | "given"
            | "using"
    )
}

fn is_direct_modifier(kind: &str) -> bool {
    matches!(
        kind,
        "abstract"
            | "sealed"
            | "case"
            | "private"
            | "protected"
            | "override"
            | "final"
            | "lazy"
            | "implicit"
    )
}

/// Extract type parameters (e.g., `[T, U]`)
pub(super) fn extract_type_parameters(base: &Base, node: &Node) -> Option<String> {
    node.children(&mut node.walk())
        .find(|n| n.kind() == "type_parameters")
        .map(|tp| base.get_node_text(&tp))
}

/// Extract function parameters (e.g., `(x: Int, y: String)`)
pub(super) fn extract_parameters(base: &Base, node: &Node) -> Option<String> {
    // Scala uses `parameters` or `class_parameters`
    node.children(&mut node.walk())
        .find(|n| n.kind() == "parameters" || n.kind() == "class_parameters")
        .map(|p| base.get_node_text(&p))
}

/// Extract return type after `:` (e.g., `: String`)
pub(super) fn extract_return_type(base: &Base, node: &Node) -> Option<String> {
    // In Scala, return type typically appears as a child after `:`
    let mut found_colon = false;
    for child in node.children(&mut node.walk()) {
        if base.get_node_text(&child) == ":" {
            found_colon = true;
            continue;
        }
        if found_colon && is_type_node(child.kind()) {
            return Some(base.get_node_text(&child));
        }
    }
    None
}

/// Extract extends clause (e.g., `extends Animal with Serializable`)
pub(super) fn extract_extends(base: &Base, node: &Node) -> Option<String> {
    node.children(&mut node.walk())
        .find(|n| n.kind() == "extends_clause")
        .map(|ec| base.get_node_text(&ec))
}

/// Determine visibility from modifiers
pub(super) fn determine_visibility(modifiers: &[String]) -> Visibility {
    if modifiers.iter().any(|m| m == "private" || m.starts_with("private[")) {
        Visibility::Private
    } else if modifiers.iter().any(|m| m == "protected" || m.starts_with("protected[")) {
        Visibility::Protected
    } else {
        Visibility::Public // Scala defaults to public
    }
}

fn is_type_node(kind: &str) -> bool {
    matches!(
        kind,
        "type_identifier"
            | "generic_type"
            | "compound_type"
            | "infix_type"
            | "function_type"
            | "tuple_type"
            | "stable_type_identifier"
            | "parameter"  // sometimes used
    )
}

/// Get the name identifier from a node
///
/// For most Scala nodes, the name is an `identifier` child.
/// For `type_definition`, the name is a `type_identifier` child.
pub(super) fn get_name(base: &Base, node: &Node) -> Option<String> {
    node.child_by_field_name("name")
        .map(|n| base.get_node_text(&n))
        .or_else(|| {
            node.children(&mut node.walk())
                .find(|n| n.kind() == "identifier" || n.kind() == "type_identifier")
                .map(|n| base.get_node_text(&n))
        })
}
