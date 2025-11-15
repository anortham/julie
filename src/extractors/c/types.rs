//! Type and attribute extraction methods
//!
//! This module provides functionality for extracting type information from the syntax tree,
//! including return types, parameters, qualifiers, and attributes.

use super::helpers;
use crate::extractors::base::BaseExtractor;

/// Extract return type from a function
pub(super) fn extract_return_type(base: &BaseExtractor, node: tree_sitter::Node) -> String {
    let mut cursor = node.walk();
    let mut base_types = Vec::new();
    let mut has_pointer = false;

    // Look for the specifier that contains the base type
    for child in node.children(&mut cursor) {
        match child.kind() {
            "primitive_type" | "type_identifier" | "sized_type_specifier" | "struct_specifier" => {
                base_types.push(base.get_node_text(&child));
            }
            "pointer_declarator" => {
                // Check if this is a function pointer return type
                has_pointer = true;
                let mut pointer_cursor = child.walk();
                for pointer_child in child.children(&mut pointer_cursor) {
                    if pointer_child.kind() == "function_declarator" {
                        // This indicates we have a pointer return type
                        continue;
                    }
                }
            }
            _ => {}
        }
    }

    // Special handling for function declarations with pointer return types
    let mut cursor2 = node.walk();
    for child in node.children(&mut cursor2) {
        if child.kind() == "pointer_declarator" {
            let mut pointer_cursor = child.walk();
            for pointer_child in child.children(&mut pointer_cursor) {
                if pointer_child.kind() == "function_declarator" {
                    // This is a function with a pointer return type
                    has_pointer = true;
                    break;
                }
            }
        }
    }

    if base_types.is_empty() {
        if has_pointer {
            return "void*".to_string();
        }
        return "void".to_string();
    }

    let base_type = base_types.join(" ");
    if has_pointer {
        format!("{}*", base_type)
    } else {
        base_type
    }
}

/// Extract storage class from a declaration (static, extern, etc.)
pub(super) fn extract_storage_class(
    base: &BaseExtractor,
    node: tree_sitter::Node,
) -> Option<String> {
    let mut storage_classes = Vec::new();
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if child.kind() == "storage_class_specifier" {
            storage_classes.push(base.get_node_text(&child));
        }
    }

    if storage_classes.is_empty() {
        None
    } else {
        Some(storage_classes.join(" "))
    }
}

/// Extract type qualifiers from a declaration (const, volatile, etc.)
pub(super) fn extract_type_qualifiers(
    base: &BaseExtractor,
    node: tree_sitter::Node,
) -> Option<String> {
    let mut qualifiers = Vec::new();
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if child.kind() == "type_qualifier" {
            qualifiers.push(base.get_node_text(&child));
        }
    }

    if qualifiers.is_empty() {
        None
    } else {
        Some(qualifiers.join(" "))
    }
}

/// Extract the data type from a declaration
/// Also searches through all descendants to find pointer declarators
pub(super) fn extract_variable_type(base: &BaseExtractor, node: tree_sitter::Node) -> String {
    let mut cursor = node.walk();
    let mut base_type = String::new();

    // Extract base type from direct children
    for child in node.children(&mut cursor) {
        match child.kind() {
            "primitive_type"
            | "type_identifier"
            | "sized_type_specifier"
            | "struct_specifier"
            | "enum_specifier" => {
                base_type = base.get_node_text(&child);
            }
            _ => {}
        }
    }

    if base_type.is_empty() {
        return "unknown".to_string();
    }

    // Search for pointer declarators in the entire subtree
    let pointer_count = count_all_pointer_levels(&node);

    // Append asterisks for each pointer level
    if pointer_count > 0 {
        format!("{}{}", base_type, "*".repeat(pointer_count))
    } else {
        base_type
    }
}

/// Count all pointer declarators in the tree by traversing all descendants
fn count_all_pointer_levels(node: &tree_sitter::Node) -> usize {
    let mut count = 0;
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if child.kind() == "pointer_declarator" {
            count += 1;
            // Don't recurse into pointer_declarator to avoid double-counting
            // Each pointer_declarator represents one level of indirection
        } else {
            // Recurse into other node types to find nested pointer_declarators
            count += count_all_pointer_levels(&child);
        }
    }

    count
}

/// Extract array specifier information from a declarator
pub(super) fn extract_array_specifier(
    base: &BaseExtractor,
    declarator: tree_sitter::Node,
) -> Option<String> {
    if let Some(array_decl) = helpers::find_node_by_type(declarator, "array_declarator") {
        // Extract array size information
        let mut sizes = Vec::new();
        let mut cursor = array_decl.walk();
        let mut found_identifier = false;

        for child in array_decl.children(&mut cursor) {
            if child.kind() == "identifier" && !found_identifier {
                found_identifier = true;
                continue; // Skip the variable name
            }
            if child.kind() != "[" && child.kind() != "]" && found_identifier {
                sizes.push(base.get_node_text(&child));
            }
        }

        if sizes.is_empty() {
            Some("[]".to_string())
        } else {
            Some(format!("[{}]", sizes.join(", ")))
        }
    } else {
        None
    }
}

/// Extract initializer from an init_declarator node
pub(super) fn extract_initializer(
    base: &BaseExtractor,
    declarator: tree_sitter::Node,
) -> Option<String> {
    if declarator.kind() == "init_declarator" {
        // Look for initializer after '='
        let mut found_equals = false;
        let mut cursor = declarator.walk();

        for child in declarator.children(&mut cursor) {
            if base.get_node_text(&child) == "=" {
                found_equals = true;
            } else if found_equals {
                return Some(base.get_node_text(&child));
            }
        }
    }
    None
}

/// Extract struct attributes (PACKED, ALIGNED, etc.)
pub(super) fn extract_struct_attributes(
    base: &BaseExtractor,
    node: tree_sitter::Node,
) -> Vec<String> {
    let mut attributes = Vec::new();
    let node_text = base.get_node_text(&node);

    if node_text.contains("PACKED") {
        attributes.push("PACKED".to_string());
    }
    if node_text.contains("ALIGNED") {
        attributes.push("ALIGNED".to_string());
    }

    attributes
}

/// Extract alignment attributes (ALIGN macro, etc.)
pub(super) fn extract_alignment_attributes(
    base: &BaseExtractor,
    node: tree_sitter::Node,
) -> Vec<String> {
    let mut attributes = Vec::new();

    let node_text = base.get_node_text(&node);

    // Check for ALIGN(CACHE_LINE_SIZE) or similar patterns
    if let Some(align_start) = node_text.find("ALIGN(") {
        if let Some(align_end) = node_text[align_start..].find(')') {
            let end_idx = align_start + align_end + 1;
            // SAFETY: Check char boundary before slicing to prevent UTF-8 panic
            if node_text.is_char_boundary(align_start) && node_text.is_char_boundary(end_idx) {
                let align_attr = &node_text[align_start..end_idx];
                attributes.push(align_attr.to_string());
            }
        }
    }

    // Check parent node if this is a typedef struct
    if let Some(parent) = node.parent() {
        let parent_text = base.get_node_text(&parent);
        if let Some(align_start) = parent_text.find("ALIGN(") {
            if let Some(align_end) = parent_text[align_start..].find(')') {
                let end_idx = align_start + align_end + 1;
                // SAFETY: Check char boundary before slicing to prevent UTF-8 panic
                if parent_text.is_char_boundary(align_start)
                    && parent_text.is_char_boundary(end_idx)
                {
                    let align_attr = &parent_text[align_start..end_idx];
                    if !attributes.contains(&align_attr.to_string()) {
                        attributes.push(align_attr.to_string());
                    }
                }
            }
        }
    }

    attributes
}

/// Extract the underlying type from a type definition
pub(super) fn extract_underlying_type_from_type_definition(
    base: &BaseExtractor,
    node: tree_sitter::Node,
) -> String {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "typedef" | ";" | "type_identifier" => continue,
            _ => return base.get_node_text(&child),
        }
    }
    "unknown".to_string()
}

/// Extract the underlying type from a typedef declaration
pub(super) fn extract_underlying_type_from_declaration(
    base: &BaseExtractor,
    node: tree_sitter::Node,
) -> String {
    let mut types = Vec::new();
    let mut cursor = node.walk();
    let mut found_typedef = false;

    for child in node.children(&mut cursor) {
        if child.kind() == "storage_class_specifier" && base.get_node_text(&child) == "typedef" {
            found_typedef = true;
            continue;
        }

        if found_typedef {
            match child.kind() {
                "primitive_type" | "sized_type_specifier" => {
                    types.push(base.get_node_text(&child));
                }
                "type_identifier" => {
                    // Skip the last type_identifier as it's the typedef name
                    let text = base.get_node_text(&child);
                    types.push(text);
                }
                _ => {}
            }
        }
    }

    // Remove the last item if it looks like a typedef name (not a known C type)
    if types.len() > 1 {
        let last_type = &types[types.len() - 1];
        let known_c_types = [
            "char", "int", "short", "long", "float", "double", "void", "unsigned", "signed",
        ];
        if !known_c_types.iter().any(|&t| last_type.contains(t)) {
            types.pop(); // Remove the typedef name
        }
    }

    if types.is_empty() {
        "unknown".to_string()
    } else {
        types.join(" ")
    }
}
