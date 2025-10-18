/// Helper functions for Java extraction
/// Handles modifiers, visibility, and common parsing utilities
use crate::extractors::base::{BaseExtractor, Visibility};
use tree_sitter::Node;

/// Extract all modifiers from a Java node (public, private, static, final, etc.)
pub(super) fn extract_modifiers(base: &BaseExtractor, node: Node) -> Vec<String> {
    node.children(&mut node.walk())
        .find(|c| c.kind() == "modifiers")
        .map(|modifiers_node| {
            modifiers_node
                .children(&mut modifiers_node.walk())
                .map(|c| base.get_node_text(&c))
                .collect()
        })
        .unwrap_or_default()
}

/// Determine visibility from modifier list
pub(super) fn determine_visibility(modifiers: &[String]) -> Visibility {
    if modifiers.contains(&"public".to_string()) {
        Visibility::Public
    } else if modifiers.contains(&"private".to_string()) {
        Visibility::Private
    } else if modifiers.contains(&"protected".to_string()) {
        Visibility::Protected
    } else {
        Visibility::Private // Default visibility in Java (package-private maps to Private)
    }
}

/// Extract superclass from a class declaration node
pub(super) fn extract_superclass(base: &BaseExtractor, node: Node) -> Option<String> {
    let superclass_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "superclass")?;

    let type_node = superclass_node
        .children(&mut superclass_node.walk())
        .find(|c| matches!(c.kind(), "type_identifier" | "generic_type"))?;

    Some(base.get_node_text(&type_node))
}

/// Extract implemented interfaces from a class/enum/record
pub(super) fn extract_implemented_interfaces(base: &BaseExtractor, node: Node) -> Vec<String> {
    let interfaces_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "super_interfaces");

    if let Some(interfaces_node) = interfaces_node {
        if let Some(type_list_node) = interfaces_node
            .children(&mut interfaces_node.walk())
            .find(|c| c.kind() == "type_list")
        {
            return type_list_node
                .children(&mut type_list_node.walk())
                .filter(|c| matches!(c.kind(), "type_identifier" | "generic_type"))
                .map(|c| base.get_node_text(&c))
                .collect();
        }
    }

    Vec::new()
}

/// Extract extended interfaces from an interface declaration
pub(super) fn extract_extended_interfaces(base: &BaseExtractor, node: Node) -> Vec<String> {
    let extends_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "extends_interfaces");

    if let Some(extends_node) = extends_node {
        if let Some(type_list_node) = extends_node
            .children(&mut extends_node.walk())
            .find(|c| c.kind() == "type_list")
        {
            return type_list_node
                .children(&mut type_list_node.walk())
                .filter(|c| matches!(c.kind(), "type_identifier" | "generic_type"))
                .map(|c| base.get_node_text(&c))
                .collect();
        }
    }

    Vec::new()
}

/// Extract type parameters from a generic type (e.g., <T, U>)
pub(super) fn extract_type_parameters(base: &BaseExtractor, node: Node) -> Option<String> {
    let type_params_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "type_parameters")?;

    Some(base.get_node_text(&type_params_node))
}

/// Extract throws clause from a method declaration
pub(super) fn extract_throws_clause(base: &BaseExtractor, node: Node) -> Option<String> {
    let throws_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "throws")?;

    Some(base.get_node_text(&throws_node))
}
