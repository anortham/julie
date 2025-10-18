/// Function signatures and parameter extraction
/// Handles parameter lists, type hints, return types, and visibility inference
use super::PythonExtractor;
use crate::extractors::base::Visibility;
use tree_sitter::Node;

/// Extract function parameters from a parameters node
pub fn extract_parameters(extractor: &PythonExtractor, parameters_node: &Node) -> Vec<String> {
    let mut params = Vec::new();
    let base = extractor.base();

    let mut cursor = parameters_node.walk();
    for child in parameters_node.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                // Simple parameter name
                params.push(base.get_node_text(&child));
            }
            "parameter" => {
                // Handle basic parameter - find identifier child
                let mut param_cursor = child.walk();
                for param_child in child.children(&mut param_cursor) {
                    if param_child.kind() == "identifier" {
                        params.push(base.get_node_text(&param_child));
                        break;
                    }
                }
            }
            "default_parameter" => {
                // parameter = default_value
                let mut parts = Vec::new();
                let mut param_cursor = child.walk();
                for param_child in child.children(&mut param_cursor) {
                    if param_child.kind() == "identifier" {
                        parts.push(base.get_node_text(&param_child));
                    } else if param_child.kind() == "=" {
                        parts.push("=".to_string());
                    } else if !["(", ")", ","].contains(&param_child.kind()) {
                        parts.push(base.get_node_text(&param_child));
                    }
                }
                if !parts.is_empty() {
                    params.push(parts.join(""));
                }
            }
            "typed_parameter" => {
                // parameter: type
                let mut name = String::new();
                let mut type_str = String::new();
                let mut param_cursor = child.walk();
                for param_child in child.children(&mut param_cursor) {
                    if param_child.kind() == "identifier" && name.is_empty() {
                        name = base.get_node_text(&param_child);
                    } else if param_child.kind() == "type" {
                        type_str = format!(": {}", base.get_node_text(&param_child));
                    }
                }
                params.push(format!("{}{}", name, type_str));
            }
            "typed_default_parameter" => {
                // parameter: type = default_value
                let text = base.get_node_text(&child);
                params.push(text);
            }
            _ => {}
        }
    }

    params
}

/// Infer visibility from a symbol name
/// Python uses naming conventions: _private, __dunder__, public
pub fn infer_visibility(name: &str) -> Visibility {
    if name.starts_with("__") && name.ends_with("__") {
        // Dunder methods are public
        Visibility::Public
    } else if name.starts_with("_") {
        // Single underscore indicates private/protected
        Visibility::Private
    } else {
        Visibility::Public
    }
}

/// Check if a function has an async keyword
pub(super) fn has_async_keyword(node: &Node) -> bool {
    // Check if any of the node's children is an "async" keyword
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "async" {
            return true;
        }
    }
    false
}

/// Find type annotation in an assignment node
#[allow(clippy::manual_find)] // Manual loop required for borrow checker
pub(super) fn find_type_annotation<'a>(node: &'a Node<'a>) -> Option<Node<'a>> {
    // Look for type annotation in assignment node children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type" {
            return Some(child);
        }
    }
    None
}
