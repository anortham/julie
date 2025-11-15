//! Helper functions for node finding, attributes, and modifiers
//! Provides utilities for navigating the PowerShell AST and extracting node information

use crate::extractors::base::BaseExtractor;
use tree_sitter::Node;

/// Find the function name node from a function_statement
pub(super) fn find_function_name_node<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    children
        .into_iter()
        .find(|child| matches!(child.kind(), "function_name" | "identifier" | "cmdlet_name"))
}

/// Find the variable name node from an assignment or variable node
pub(super) fn find_variable_name_node<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    children.into_iter().find(|child| {
        matches!(
            child.kind(),
            "left_assignment_expression" | "variable" | "identifier"
        )
    })
}

/// Find the parameter name node from a parameter_definition
pub(super) fn find_parameter_name_node<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    children
        .into_iter()
        .find(|child| matches!(child.kind(), "variable" | "parameter_name"))
}

/// Find the class name node from a class_statement
pub(super) fn find_class_name_node<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    children
        .into_iter()
        .find(|child| matches!(child.kind(), "simple_name" | "identifier" | "type_name"))
}

/// Find the method name node from a class_method_definition
pub(super) fn find_method_name_node<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    children
        .into_iter()
        .find(|child| matches!(child.kind(), "simple_name" | "identifier" | "method_name"))
}

/// Find the property name node from a class_property_definition
pub(super) fn find_property_name_node<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    children
        .into_iter()
        .find(|child| matches!(child.kind(), "variable" | "property_name" | "identifier"))
}

/// Find the enum name node from an enum_statement
pub(super) fn find_enum_name_node<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    children
        .into_iter()
        .find(|child| matches!(child.kind(), "simple_name" | "identifier" | "type_name"))
}

/// Find the enum member name node from an enum_member
pub(super) fn find_enum_member_name_node<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    children
        .into_iter()
        .find(|child| matches!(child.kind(), "simple_name" | "identifier"))
}

/// Find the command name node from a command or command_expression
pub(super) fn find_command_name_node<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    children
        .into_iter()
        .find(|child| matches!(child.kind(), "command_name" | "identifier" | "cmdlet_name"))
}

/// Find the configuration name node from a configuration statement
pub(super) fn find_configuration_name_node<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    children
        .into_iter()
        .find(|child| child.kind() == "identifier")
}

/// Extract the value from an enum member (if assigned)
pub(super) fn extract_enum_member_value(base: &BaseExtractor, node: Node) -> Option<String> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();

    // Look for assignment pattern: name = value
    for (i, child) in children.iter().enumerate() {
        if child.kind() == "=" && i + 1 < children.len() {
            return Some(base.get_node_text(&children[i + 1]));
        }
    }
    None
}

/// Check if a node has an attribute (e.g., [CmdletBinding], [Parameter])
pub(super) fn has_attribute(base: &BaseExtractor, node: Node, attribute_name: &str) -> bool {
    let node_text = base.get_node_text(&node);
    node_text.contains(&format!("[{}", attribute_name))
}

/// Check if a parameter node has an attribute (e.g., Mandatory=$true)
pub(super) fn has_parameter_attribute(
    base: &BaseExtractor,
    node: Node,
    attribute_name: &str,
) -> bool {
    let node_text = base.get_node_text(&node);
    node_text.contains(&format!("{}=$true", attribute_name))
        || node_text.contains(&format!("{}=true", attribute_name))
}

/// Check if a node has a modifier (e.g., static, hidden)
pub(super) fn has_modifier(base: &BaseExtractor, node: Node, modifier: &str) -> bool {
    let node_text = base.get_node_text(&node);
    node_text.contains(modifier)
}

/// Extract parameter attributes from a parameter definition
pub(super) fn extract_parameter_attributes(base: &BaseExtractor, node: Node) -> String {
    let node_text = base.get_node_text(&node);
    if let Some(captures) = regex::Regex::new(r"\[Parameter[^\]]*\]")
        .unwrap()
        .captures(&node_text)
    {
        captures
            .get(0)
            .map_or(String::new(), |m| m.as_str().to_string())
    } else {
        String::new()
    }
}

/// Extract inheritance relationship from a class definition
pub(super) fn extract_inheritance(base: &BaseExtractor, node: Node) -> Option<String> {
    let node_text = base.get_node_text(&node);
    regex::Regex::new(r":\s*(\w+)")
        .unwrap()
        .captures(&node_text)
        .and_then(|captures| captures.get(1).map(|m| m.as_str().to_string()))
}

/// Extract return type annotation from a method definition
pub(super) fn extract_return_type(base: &BaseExtractor, node: Node) -> Option<String> {
    let node_text = base.get_node_text(&node);
    regex::Regex::new(r"\[(\w+)\]")
        .unwrap()
        .captures(&node_text)
        .and_then(|captures| captures.get(1).map(|m| format!("[{}]", m.as_str())))
}

/// Extract property type annotation from a property definition
pub(super) fn extract_property_type(base: &BaseExtractor, node: Node) -> Option<String> {
    let node_text = base.get_node_text(&node);
    regex::Regex::new(r"\[(\w+)\]")
        .unwrap()
        .captures(&node_text)
        .and_then(|captures| captures.get(1).map(|m| format!("[{}]", m.as_str())))
}

/// Recursively find all nodes of a given type
#[allow(clippy::only_used_in_recursion)] // &self used in recursive calls
pub(super) fn find_nodes_by_type<'a>(node: Node<'a>, node_type: &str) -> Vec<Node<'a>> {
    let mut result = Vec::new();
    let mut cursor = node.walk();

    // Check direct children first
    for child in node.children(&mut cursor) {
        if child.kind() == node_type {
            result.push(child);
        }
        // Recursively search in children
        result.extend(find_nodes_by_type(child, node_type));
    }

    result
}

/// Extract function name from a param_block node (used for advanced functions)
pub(super) fn extract_function_name_from_param_block(
    base: &BaseExtractor,
    node: Node,
    function_name_re: &regex::Regex,
) -> Option<String> {
    // For param_block nodes inside advanced functions, we need to look up the tree
    // to find the ERROR node that contains the function declaration

    // First, try to find ERROR node at program level (parent's parent's parent typically)
    let mut current = Some(node);
    while let Some(n) = current {
        if n.kind() == "program" {
            break;
        }
        current = n.parent();
    }

    if let Some(program_node) = current {
        // Look for ERROR node in program children
        let mut cursor = program_node.walk();
        for child in program_node.children(&mut cursor) {
            if child.kind() == "ERROR" {
                let text = base.get_node_text(&child);
                // Extract function name from text like "\nfunction Set-CustomProperty {"
                if let Some(captures) = function_name_re.captures(&text) {
                    if let Some(func_name) = captures.get(1) {
                        return Some(func_name.as_str().to_string());
                    }
                }
            }
        }
    }

    // Fallback: look in parent nodes for any ERROR containing function
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "ERROR" {
            let text = base.get_node_text(&n);
            if let Some(captures) = function_name_re.captures(&text) {
                if let Some(func_name) = captures.get(1) {
                    return Some(func_name.as_str().to_string());
                }
            }
        }
        current = n.parent();
    }

    None
}
