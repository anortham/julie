// Dart Extractor - Signature Extraction
//
// Methods for extracting and building signatures for various Dart constructs

use super::helpers::*;
use tree_sitter::Node;

/// Extract class signature with modifiers, generics, inheritance, and interfaces
pub(super) fn extract_class_signature(node: &Node) -> String {
    let name_node = find_child_by_type(node, "identifier");
    let name = name_node
        .map(|n| get_node_text(&n))
        .unwrap_or_else(|| "Unknown".to_string());

    let is_abstract = is_abstract_class(node);
    let abstract_prefix = if is_abstract { "abstract " } else { "" };

    // Extract generic type parameters (e.g., <T>)
    let type_params_node = find_child_by_type(node, "type_parameters");
    let type_params = type_params_node
        .map(|n| get_node_text(&n))
        .unwrap_or_default();

    let extends_clause = find_child_by_type(node, "superclass");
    let extends_text = if let Some(extends_node) = extends_clause {
        // Extract the full superclass including generics (e.g., "State<MyPage>")
        if let Some(type_node) = find_child_by_type(&extends_node, "type_identifier") {
            let mut superclass_type = get_node_text(&type_node);

            // Check for generic type arguments
            if let Some(type_args_node) = type_node.next_sibling() {
                if type_args_node.kind() == "type_arguments" {
                    superclass_type.push_str(&get_node_text(&type_args_node));
                }
            }

            format!(" extends {}", superclass_type)
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let implements_clause = find_child_by_type(node, "interfaces");
    let implements_text = implements_clause
        .map(|n| format!(" implements {}", get_node_text(&n)))
        .unwrap_or_default();

    // Extract mixin clauses (with clause) - these are nested within superclass
    let mixin_text = if let Some(extends_node) = extends_clause {
        find_child_by_type(&extends_node, "mixins")
            .map(|n| format!(" {}", get_node_text(&n)))
            .unwrap_or_default()
    } else {
        String::new()
    };

    format!(
        "{}class {}{}{}{}{}",
        abstract_prefix, name, type_params, extends_text, mixin_text, implements_text
    )
}

/// Extract function signature with return type, parameters, and async modifier
pub(super) fn extract_function_signature(node: &Node, content: &str) -> String {
    let name_node = find_child_by_type(node, "identifier");
    let name = name_node
        .map(|n| get_node_text(&n))
        .unwrap_or_else(|| "unknown".to_string());

    // Get return type (can be type_identifier or void_type)
    let return_type_node = find_child_by_type(node, "type_identifier")
        .or_else(|| find_child_by_type(node, "void_type"));

    let mut return_type = return_type_node
        .map(|n| get_node_text(&n))
        .unwrap_or_default();

    // Check for generic type arguments (e.g., Future<String>)
    if let Some(type_node) = return_type_node {
        if let Some(type_args_node) = type_node.next_sibling() {
            if type_args_node.kind() == "type_arguments" {
                return_type.push_str(&get_node_text(&type_args_node));
            }
        }
    }

    // Extract generic type parameters (e.g., <T extends Comparable<T>>)
    let type_params_node = find_child_by_type(node, "type_parameters");
    let type_params = type_params_node
        .map(|n| get_node_text(&n))
        .unwrap_or_default();

    // Get parameters
    let param_list_node = find_child_by_type(node, "formal_parameter_list");
    let params = param_list_node
        .map(|n| get_node_text(&n))
        .unwrap_or_else(|| "()".to_string());

    // Check for async modifier
    let is_async = is_async_function(node, content);
    let async_modifier = if is_async { " async" } else { "" };

    // Build signature with return type, generic parameters, and async modifier
    if !return_type.is_empty() {
        format!(
            "{} {}{}{}{}",
            return_type, name, type_params, params, async_modifier
        )
    } else {
        format!("{}{}{}{}", name, type_params, params, async_modifier)
    }
}

/// Extract constructor signature with factory/const modifiers
pub(super) fn extract_constructor_signature(node: &Node) -> String {
    let is_factory = node.kind() == "factory_constructor_signature";
    let is_const = node.kind() == "constant_constructor_signature";

    // Extract constructor name - use consistent logic with extract_constructor
    let constructor_name = match node.kind() {
        "constant_constructor_signature" => {
            // For const constructors, just get the first identifier
            find_child_by_type(node, "identifier")
                .map(|n| get_node_text(&n))
                .unwrap_or_else(|| "Constructor".to_string())
        }
        "factory_constructor_signature" => {
            // For factory constructors, may need class.name pattern
            let mut identifiers = Vec::new();
            traverse_tree(*node, &mut |child| {
                if child.kind() == "identifier" && identifiers.len() < 2 {
                    identifiers.push(get_node_text(&child));
                }
            });
            identifiers.join(".")
        }
        _ => {
            // Regular constructor
            find_child_by_type(node, "identifier")
                .map(|n| get_node_text(&n))
                .unwrap_or_else(|| "Constructor".to_string())
        }
    };

    // Add prefixes
    let factory_prefix = if is_factory { "factory " } else { "" };
    let const_prefix = if is_const { "const " } else { "" };

    format!("{}{}{}()", factory_prefix, const_prefix, constructor_name)
}

/// Extract variable signature (just the name)
pub(super) fn extract_variable_signature(node: &Node) -> String {
    let name_node = find_child_by_type(node, "identifier");
    name_node
        .map(|n| get_node_text(&n))
        .unwrap_or_else(|| "unknown".to_string())
}
