// C# Helper Methods
//
// Collection of utility functions for parsing C# AST nodes and extracting metadata

use crate::extractors::base::{BaseExtractor, Visibility};
use tree_sitter::Node;

/// Extract modifiers from a node (attributes and modifiers)
pub fn extract_modifiers(base: &BaseExtractor, node: &Node) -> Vec<String> {
    let mut attributes = Vec::new();
    let mut modifiers = Vec::new();

    let mut cursor = node.walk();

    // Extract attributes
    for child in node.children(&mut cursor) {
        if child.kind() == "attribute_list" {
            attributes.push(base.get_node_text(&child));
        }
    }

    // Extract modifiers
    for child in node.children(&mut cursor) {
        if child.kind() == "modifier" {
            modifiers.push(base.get_node_text(&child));
        }
    }

    // Combine attributes and modifiers
    [attributes, modifiers].concat()
}

/// Determine visibility from modifiers
pub fn determine_visibility(modifiers: &[String], node_type: Option<&str>) -> Visibility {
    if modifiers.contains(&"public".to_string()) {
        return Visibility::Public;
    }
    if modifiers.contains(&"private".to_string()) {
        return Visibility::Private;
    }
    if modifiers.contains(&"protected".to_string()) {
        return Visibility::Protected;
    }
    if modifiers.contains(&"internal".to_string()) {
        return Visibility::Private; // Map internal to Private, store actual value in metadata
    }

    // Special cases for default visibility
    if node_type == Some("constructor_declaration") {
        return Visibility::Public; // Constructors default to public when in public classes
    }

    // Default visibility in C#
    Visibility::Private
}

/// Get C# visibility string including internal
pub fn get_csharp_visibility_string(modifiers: &[String]) -> String {
    if modifiers.contains(&"public".to_string()) {
        "public".to_string()
    } else if modifiers.contains(&"private".to_string()) {
        "private".to_string()
    } else if modifiers.contains(&"protected".to_string()) {
        "protected".to_string()
    } else if modifiers.contains(&"internal".to_string()) {
        "internal".to_string()
    } else {
        "private".to_string() // Default
    }
}

/// Extract base list (inheritance/implementation classes and interfaces)
pub fn extract_base_list(base: &BaseExtractor, node: &Node) -> Vec<String> {
    let mut cursor = node.walk();
    let base_list = node.children(&mut cursor).find(|c| c.kind() == "base_list");

    if let Some(base_list) = base_list {
        let mut base_cursor = base_list.walk();
        base_list
            .children(&mut base_cursor)
            .filter(|c| c.kind() != ":" && c.kind() != ",")
            .map(|c| base.get_node_text(&c))
            .collect()
    } else {
        Vec::new()
    }
}

/// Extract type parameters (generic type parameters like <T, U>)
pub fn extract_type_parameters(base: &BaseExtractor, node: &Node) -> Option<String> {
    let mut cursor = node.walk();
    let type_params = node
        .children(&mut cursor)
        .find(|c| c.kind() == "type_parameter_list");
    type_params.map(|tp| base.get_node_text(&tp))
}

/// Extract return type from a method node
pub fn extract_return_type(base: &BaseExtractor, node: &Node) -> Option<String> {
    // Find method name identifier - comes before parameter_list (may have type_parameter_list in between)
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    let param_list_index = children.iter().position(|c| c.kind() == "parameter_list")?;

    // Look backwards from parameter_list to find the method name identifier
    let name_node = children[..param_list_index]
        .iter()
        .rev()
        .find(|c| c.kind() == "identifier")?;

    let name_index = children.iter().position(|c| std::ptr::eq(c, name_node))?;
    // Look for return type, but exclude modifiers
    let return_type_node = children[..name_index].iter().find(|c| {
        matches!(
            c.kind(),
            "predefined_type"
                | "identifier"
                | "qualified_name"
                | "generic_name"
                | "array_type"
                | "nullable_type"
                | "tuple_type"
        )
    });

    return_type_node.map(|node| base.get_node_text(node))
}

/// Extract property type from a property declaration
pub fn extract_property_type(base: &BaseExtractor, node: &Node) -> Option<String> {
    // In C# property declarations, the type is typically the first significant node
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();

    // Skip modifiers and find the type node
    let modifiers = [
        "public",
        "private",
        "protected",
        "internal",
        "static",
        "virtual",
        "override",
        "abstract",
    ];

    for child in &children {
        let child_text = base.get_node_text(child);

        // Skip modifier nodes
        if modifiers.contains(&child_text.as_str()) {
            continue;
        }

        // Look for type nodes
        if matches!(
            child.kind(),
            "predefined_type"
                | "identifier"
                | "qualified_name"
                | "generic_name"
                | "array_type"
                | "nullable_type"
                | "tuple_type"
        ) {
            return Some(child_text);
        }
    }

    None
}

/// Extract field type from a field declaration
pub fn extract_field_type(base: &BaseExtractor, node: &Node) -> Option<String> {
    // Field type is the first child of variable_declaration
    let mut cursor = node.walk();
    let var_declaration = node
        .children(&mut cursor)
        .find(|c| c.kind() == "variable_declaration")?;

    let mut var_cursor = var_declaration.walk();
    let type_node = var_declaration.children(&mut var_cursor).find(|c| {
        matches!(
            c.kind(),
            "predefined_type"
                | "identifier"
                | "qualified_name"
                | "generic_name"
                | "array_type"
                | "nullable_type"
        )
    });

    type_node.map(|node| base.get_node_text(&node))
}
