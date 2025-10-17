//! Helper functions for C++ extraction
//! Contains utilities for template parameters, base classes, and node analysis

use crate::extractors::base::BaseExtractor;
use tree_sitter::Node;

/// Extract template parameters from a template_declaration node
pub(super) fn extract_template_parameters(
    base: &BaseExtractor,
    template_node: Option<Node>,
) -> Option<String> {
    let mut current = template_node;
    while let Some(node) = current {
        if node.kind() == "template_declaration" {
            let mut cursor = node.walk();
            let param_list = node
                .children(&mut cursor)
                .find(|c| c.kind() == "template_parameter_list");
            if let Some(param_list) = param_list {
                return Some(format!("template{}", base.get_node_text(&param_list)));
            }
        }
        current = node.parent();
    }
    None
}

/// Extract base classes from a base_class_clause node
pub(super) fn extract_base_classes(base: &BaseExtractor, base_clause: Node) -> Vec<String> {
    let mut bases = Vec::new();
    let mut cursor = base_clause.walk();
    let children: Vec<Node> = base_clause.children(&mut cursor).collect();

    let mut i = 0;
    while i < children.len() {
        let child = children[i];

        if child.kind() == ":" || child.kind() == "," {
            i += 1;
            continue;
        }

        // For inheritance like ": public Shape", extract access + class name
        if child.kind() == "access_specifier" {
            let access = base.get_node_text(&child);
            // Look for the next child which should be the class name
            i += 1;
            if i < children.len() {
                let class_node = children[i];
                if matches!(
                    class_node.kind(),
                    "type_identifier" | "qualified_identifier" | "template_type"
                ) {
                    let class_name = base.get_node_text(&class_node);
                    bases.push(format!("{} {}", access, class_name));
                }
            }
        } else if matches!(
            child.kind(),
            "type_identifier" | "qualified_identifier" | "template_type"
        ) {
            // Class name without explicit access specifier
            let class_name = base.get_node_text(&child);
            bases.push(class_name);
        }

        i += 1;
    }

    bases
}

/// Find the parent enum node for an enum member
pub(super) fn find_parent_enum(node: Node) -> Option<Node> {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "enum_specifier" {
            return Some(parent);
        }
        current = parent.parent();
    }
    None
}

/// Recursively search for function_declarator in a node tree
pub(super) fn find_function_declarator_in_node(node: Node) -> Option<Node> {
    if node.kind() == "function_declarator" {
        return Some(node);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(result) = find_function_declarator_in_node(child) {
            return Some(result);
        }
    }

    None
}

/// Extract the storage class specifiers from a node
pub(super) fn extract_storage_class(base: &BaseExtractor, node: Node) -> Vec<String> {
    let mut storage_classes = Vec::new();
    let storage_types = ["static", "extern", "mutable", "thread_local"];

    for child in node.children(&mut node.walk()) {
        if storage_types.contains(&child.kind()) || child.kind() == "storage_class_specifier" {
            storage_classes.push(base.get_node_text(&child));
        }
    }

    storage_classes
}

/// Extract type specifiers from a node (const, constexpr, volatile)
pub(super) fn extract_type_specifiers(base: &BaseExtractor, node: Node) -> Vec<String> {
    let mut type_specifiers = Vec::new();
    let type_kinds = ["const", "constexpr", "volatile"];

    for child in node.children(&mut node.walk()) {
        if type_kinds.contains(&child.kind()) || child.kind() == "type_qualifier" {
            type_specifiers.push(base.get_node_text(&child));
        }
    }

    type_specifiers
}

/// Check if a declaration is a constant (has const/constexpr)
pub(super) fn is_constant_declaration(
    storage_class: &[String],
    type_specifiers: &[String],
) -> bool {
    type_specifiers
        .iter()
        .any(|spec| spec == "const" || spec == "constexpr")
        || storage_class.iter().any(|sc| sc == "constexpr")
}

/// Check if a node is inside a class and has static storage class
pub(super) fn is_static_member_variable(node: Node, storage_class: &[String]) -> bool {
    // Check if this is a static member variable inside a class

    // First check if it has static storage class
    let has_static = storage_class.iter().any(|sc| sc == "static");

    if !has_static {
        return false;
    }

    // Check if this declaration is inside a class by walking up the tree
    let mut current = node.parent();
    while let Some(parent) = current {
        match parent.kind() {
            "class_specifier" | "struct_specifier" => return true,
            "translation_unit" => return false, // Reached top level
            _ => current = parent.parent(),
        }
    }

    false
}

/// Find the identifier node in a declarator
pub(super) fn extract_declarator_name(node: Node) -> Option<Node> {
    node.children(&mut node.walk())
        .find(|c| c.kind() == "identifier")
}

/// Collect modifier keywords recursively from a node
pub(super) fn collect_modifiers_recursive(
    base: &BaseExtractor,
    node: Node,
    modifiers: &mut Vec<String>,
    modifier_types: &[&str],
) {
    for child in node.children(&mut node.walk()) {
        // Collect modifiers from both modifier_types and storage_class_specifier
        if modifier_types.contains(&child.kind()) || child.kind() == "storage_class_specifier" {
            let modifier = base.get_node_text(&child);
            if !modifiers.contains(&modifier) {
                modifiers.push(modifier);
            }
        }
        // Recursively check children but don't go too deep to avoid function bodies
        if !matches!(child.kind(), "compound_statement" | "function_body") {
            collect_modifiers_recursive(base, child, modifiers, modifier_types);
        }
    }
}
