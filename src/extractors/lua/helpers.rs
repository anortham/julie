/// Helper functions for common operations
///
/// Provides shared utilities:
/// - Node traversal helpers
/// - Type inference from expressions
/// - Node text extraction
use crate::extractors::base::BaseExtractor;
use tree_sitter::Node;

/// Find a child node by type
///
/// Searches immediate children for a node of the specified type.
/// Uses manual loop instead of iterator for borrow checker compatibility.
#[allow(clippy::manual_find)]
pub(crate) fn find_child_by_type<'a>(node: Node<'a>, node_type: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == node_type {
            return Some(child);
        }
    }
    None
}

/// Check if a node contains a function definition child
pub(crate) fn contains_function_definition(node: Node) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "function_definition" {
            return true;
        }
    }
    false
}

/// Infer the data type from an expression node
///
/// Handles various expression types and returns a string representation
/// of the inferred type (e.g., "string", "number", "function", "table", "import").
pub(crate) fn infer_type_from_expression(base: &BaseExtractor, node: Node) -> String {
    match node.kind() {
        "string" => "string".to_string(),
        "number" => "number".to_string(),
        "true" | "false" => "boolean".to_string(),
        "nil" => "nil".to_string(),
        "function_definition" => "function".to_string(),
        "table_constructor" | "table" => "table".to_string(),
        "function_call" => {
            // Check if this is a require() call
            if let Some(identifier) = find_child_by_type(node, "identifier") {
                if base.get_node_text(&identifier) == "require" {
                    return "import".to_string();
                }
            }
            "unknown".to_string()
        }
        _ => "unknown".to_string(),
    }
}
