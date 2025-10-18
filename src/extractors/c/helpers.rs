//! Helper methods for node navigation, name extraction, and tree utilities
//!
//! This module provides utilities for finding nodes, extracting names from various
//! C constructs, and navigating the syntax tree.

use crate::extractors::base::BaseExtractor;

/// Find a function declarator node within a declaration
pub(super) fn find_function_declarator<'a>(
    node: tree_sitter::Node<'a>,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "function_declarator" {
            return Some(child);
        }
        if child.kind() == "init_declarator" {
            let mut init_cursor = child.walk();
            for init_child in child.children(&mut init_cursor) {
                if init_child.kind() == "function_declarator" {
                    return Some(init_child);
                }
            }
        }
    }
    None
}

/// Find all variable declarators (identifiers, array declarators, etc.)
pub(super) fn find_variable_declarators<'a>(
    node: tree_sitter::Node<'a>,
) -> Vec<tree_sitter::Node<'a>> {
    let mut declarators = Vec::new();
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        match child.kind() {
            "init_declarator" | "declarator" | "identifier" | "array_declarator" => {
                declarators.push(child);
            }
            _ => {}
        }
    }

    declarators
}

/// Find the deepest identifier within a declarator node tree
pub(super) fn find_deepest_identifier<'a>(
    node: tree_sitter::Node<'a>,
) -> Option<tree_sitter::Node<'a>> {
    if node.kind() == "identifier" {
        return Some(node);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(result) = find_deepest_identifier(child) {
            return Some(result);
        }
    }

    None
}

/// Find a node by its type/kind recursively
pub(super) fn find_node_by_type<'a>(
    node: tree_sitter::Node<'a>,
    node_type: &str,
) -> Option<tree_sitter::Node<'a>> {
    if node.kind() == node_type {
        return Some(node);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(result) = find_node_by_type(child, node_type) {
            return Some(result);
        }
    }

    None
}

/// Extract macro name from a preprocessor node
pub(super) fn extract_macro_name(base: &BaseExtractor, node: tree_sitter::Node) -> String {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return base.get_node_text(&child);
        }
    }
    "unknown".to_string()
}

/// Extract include path from an include directive signature
pub(super) fn extract_include_path(signature: &str) -> String {
    // Extract include path from #include statement
    if let Some(start) = signature.find('"') {
        if let Some(end) = signature.rfind('"') {
            if start < end {
                return signature[start + 1..end].to_string();
            }
        }
    }
    if let Some(start) = signature.find('<') {
        if let Some(end) = signature.rfind('>') {
            if start < end {
                return signature[start + 1..end].to_string();
            }
        }
    }
    "unknown".to_string()
}

/// Check if an include is a system header (uses < > instead of " ")
pub(super) fn is_system_header(signature: &str) -> bool {
    signature.contains('<') && signature.contains('>')
}

/// Extract function name from a function definition or declaration
pub(super) fn extract_function_name(base: &BaseExtractor, node: tree_sitter::Node) -> String {
    // Look for function declarator
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "function_declarator" {
            if let Some(identifier) = child.child_by_field_name("declarator") {
                return base.get_node_text(&identifier);
            }
        }
        // For pointer return types, check pointer_declarator
        if child.kind() == "pointer_declarator" {
            let mut pointer_cursor = child.walk();
            for pointer_child in child.children(&mut pointer_cursor) {
                if pointer_child.kind() == "function_declarator" {
                    if let Some(identifier) = pointer_child.child_by_field_name("declarator") {
                        return base.get_node_text(&identifier);
                    }
                }
            }
        }
    }
    "unknown".to_string()
}

/// Extract function name from a function declaration
pub(super) fn extract_function_name_from_declaration(
    base: &BaseExtractor,
    node: tree_sitter::Node,
) -> String {
    if let Some(function_declarator) = find_function_declarator(node) {
        if let Some(identifier) = function_declarator.child_by_field_name("declarator") {
            return base.get_node_text(&identifier);
        }
    }
    "unknown".to_string()
}

/// Extract variable name from a declarator node
pub(super) fn extract_variable_name(base: &BaseExtractor, declarator: tree_sitter::Node) -> String {
    if declarator.kind() == "identifier" {
        return base.get_node_text(&declarator);
    }

    // Find deepest identifier in declarator tree
    find_deepest_identifier(declarator)
        .map(|node| base.get_node_text(&node))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Extract struct name from a struct specifier
pub(super) fn extract_struct_name(base: &BaseExtractor, node: tree_sitter::Node) -> String {
    if let Some(name_node) = node.child_by_field_name("name") {
        base.get_node_text(&name_node)
    } else {
        "anonymous".to_string()
    }
}

/// Extract enum name from an enum specifier
pub(super) fn extract_enum_name(base: &BaseExtractor, node: tree_sitter::Node) -> String {
    if let Some(name_node) = node.child_by_field_name("name") {
        base.get_node_text(&name_node)
    } else {
        "anonymous".to_string()
    }
}

/// Check if this looks like a typedef name by examining parent context
pub(super) fn looks_like_typedef_name(
    base: &BaseExtractor,
    node: &tree_sitter::Node,
    _identifier_name: &str,
) -> bool {
    // Simple heuristic: check if previous siblings contain "typedef"
    if let Some(parent) = node.parent() {
        let mut cursor = parent.walk();
        for child in parent.children(&mut cursor) {
            let child_text = base.get_node_text(&child);
            if child_text.contains("typedef") {
                return true;
            }
        }
    }
    false
}

/// Recursively collect all identifiers from a node tree
pub(super) fn collect_all_identifiers(
    base: &BaseExtractor,
    node: tree_sitter::Node,
    identifiers: &mut Vec<String>,
) {
    match node.kind() {
        "identifier" | "type_identifier" | "primitive_type" => {
            let text = base.get_node_text(&node);
            identifiers.push(text);
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_all_identifiers(base, child, identifiers);
            }
        }
    }
}

/// Check if a tree contains a struct specifier
pub(super) fn contains_struct(node: tree_sitter::Node) -> bool {
    if node.kind() == "struct_specifier" {
        return true;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if contains_struct(child) {
            return true;
        }
    }

    false
}

/// Check if a typedef name is valid (not a C keyword)
pub(super) fn is_valid_typedef_name(name: &str) -> bool {
    let c_keywords = [
        "typedef", "int", "char", "void", "const", "volatile", "static", "extern", "unsigned",
        "signed", "long", "short", "float", "double",
    ];
    !c_keywords.contains(&name) && !name.is_empty()
}

/// Check if this is a typedef declaration
pub(super) fn is_typedef_declaration(base: &BaseExtractor, node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "storage_class_specifier" && base.get_node_text(&child) == "typedef" {
            return true;
        }
    }
    false
}

/// Check if a function/variable is static
pub(super) fn is_static_function(base: &BaseExtractor, node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "storage_class_specifier" && base.get_node_text(&child) == "static" {
            return true;
        }
    }
    false
}

/// Check if a variable is extern
pub(super) fn is_extern_variable(base: &BaseExtractor, node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "storage_class_specifier" && base.get_node_text(&child) == "extern" {
            return true;
        }
    }
    false
}

/// Check if a variable is const
pub(super) fn is_const_variable(base: &BaseExtractor, node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_qualifier" && base.get_node_text(&child) == "const" {
            return true;
        }
    }
    false
}

/// Check if a variable is volatile
pub(super) fn is_volatile_variable(base: &BaseExtractor, node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_qualifier" && base.get_node_text(&child) == "volatile" {
            return true;
        }
    }
    false
}

/// Check if a declarator is an array
pub(super) fn is_array_variable(declarator: tree_sitter::Node) -> bool {
    find_node_by_type(declarator, "array_declarator").is_some()
}
