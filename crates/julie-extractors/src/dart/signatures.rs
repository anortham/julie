// Dart Extractor - Signature Extraction
//
// Methods for extracting and building signatures for various Dart constructs

use super::helpers::*;
use tree_sitter::Node;

/// Extract class signature with modifiers, generics, inheritance, and interfaces
pub(super) fn extract_class_signature(node: &Node) -> Option<String> {
    let name_node = find_child_by_type(node, "identifier");
    let name = name_node.map(|n| get_node_text(&n))?;

    let is_abstract = is_abstract_class(node);
    let abstract_prefix = if is_abstract { "abstract " } else { "" };

    // Extract generic type parameters (e.g., <T>)
    let type_params_node = find_child_by_type(node, "type_parameters");
    let type_params = type_params_node
        .map(|n| get_node_text(&n))
        .unwrap_or_default();

    let extends_clause = find_child_by_type(node, "superclass");
    let extends_text = if let Some(extends_node) = extends_clause {
        let superclass_type = extract_superclass_type_text(&extends_node);

        if superclass_type.is_empty() {
            String::new()
        } else {
            format!(" extends {}", superclass_type)
        }
    } else {
        String::new()
    };

    let implements_clause = find_child_by_type(node, "interfaces");
    let implements_text = implements_clause
        .map(|n| {
            let interfaces = get_node_text(&n);
            let interfaces = interfaces
                .trim()
                .strip_prefix("implements")
                .unwrap_or(interfaces.trim())
                .trim();
            if interfaces.is_empty() {
                String::new()
            } else {
                format!(" implements {}", interfaces)
            }
        })
        .unwrap_or_default();

    // Extract mixin clauses (with clause) - these are nested within superclass
    let mixin_text = if let Some(extends_node) = extends_clause {
        find_child_by_type(&extends_node, "mixins")
            .map(|n| format!(" {}", get_node_text(&n)))
            .unwrap_or_default()
    } else {
        String::new()
    };

    Some(format!(
        "{}class {}{}{}{}{}",
        abstract_prefix, name, type_params, extends_text, mixin_text, implements_text
    ))
}

fn extract_superclass_type_text(extends_node: &Node) -> String {
    let source = get_node_text(extends_node);
    let source = source.trim();
    let after_extends = source
        .strip_prefix("extends")
        .unwrap_or(source)
        .trim_start();
    split_before_top_level_keyword(after_extends, "with")
        .trim()
        .to_string()
}

fn split_before_top_level_keyword<'a>(source: &'a str, keyword: &str) -> &'a str {
    let mut angle_depth = 0usize;
    let mut byte_index = 0usize;

    for ch in source.chars() {
        match ch {
            '<' => angle_depth += 1,
            '>' => angle_depth = angle_depth.saturating_sub(1),
            _ => {}
        }

        if angle_depth == 0 && source[byte_index..].starts_with(keyword) {
            let before = source[..byte_index].chars().next_back();
            let after = source[byte_index + keyword.len()..].chars().next();
            if before.is_none_or(char::is_whitespace) && after.is_none_or(char::is_whitespace) {
                return &source[..byte_index];
            }
        }

        byte_index += ch.len_utf8();
    }

    source
}

/// Extract function signature with return type, parameters, and async modifier
pub(super) fn extract_function_signature(node: &Node, content: &str) -> Option<String> {
    let name_node = find_child_by_type(node, "identifier");
    let name = name_node.map(|n| get_node_text(&n))?;

    let return_type_node = find_child_by_type(node, "type")
        .or_else(|| find_child_by_type(node, "type_identifier"))
        .or_else(|| find_child_by_type(node, "void_type"));

    let mut return_type = return_type_node
        .map(|n| get_node_text(&n))
        .unwrap_or_default();

    if let Some(type_node) = return_type_node {
        if type_node.kind() != "type" {
            if let Some(type_args_node) = type_node.next_sibling() {
                if type_args_node.kind() == "type_arguments" {
                    return_type.push_str(&get_node_text(&type_args_node));
                }
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
        Some(format!(
            "{} {}{}{}{}",
            return_type, name, type_params, params, async_modifier
        ))
    } else {
        Some(format!(
            "{}{}{}{}",
            name, type_params, params, async_modifier
        ))
    }
}

/// Extract constructor signature with factory/const modifiers
pub(super) fn extract_constructor_signature(node: &Node) -> Option<String> {
    let is_factory = node.kind() == "factory_constructor_signature";
    let is_const = node.kind() == "constant_constructor_signature";

    // Extract constructor name - use consistent logic with extract_constructor
    let constructor_name = match node.kind() {
        "constant_constructor_signature" => {
            // For const constructors, just get the first identifier
            find_child_by_type(node, "identifier").map(|n| get_node_text(&n))?
        }
        "factory_constructor_signature" => {
            // For factory constructors, may need class.name pattern
            let mut identifiers = Vec::new();
            traverse_tree(*node, &mut |child| {
                if child.kind() == "identifier" && identifiers.len() < 2 {
                    identifiers.push(get_node_text(&child));
                }
            });
            if identifiers.is_empty() {
                return None;
            }
            identifiers.join(".")
        }
        _ => {
            // Regular constructor
            find_child_by_type(node, "identifier").map(|n| get_node_text(&n))?
        }
    };

    // Add prefixes
    let factory_prefix = if is_factory { "factory " } else { "" };
    let const_prefix = if is_const { "const " } else { "" };

    Some(format!(
        "{}{}{}()",
        factory_prefix, const_prefix, constructor_name
    ))
}

/// Extract variable signature (just the name)
pub(super) fn extract_variable_signature(node: &Node) -> Option<String> {
    let name_node = find_child_by_type(node, "identifier");
    name_node.map(|n| get_node_text(&n))
}
