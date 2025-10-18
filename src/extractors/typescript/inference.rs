//! Type inference from assignments and return statements
//!
//! This module handles basic type inference for variables and functions
//! based on their assignments and return statements.

use crate::extractors::base::Symbol;
use crate::extractors::typescript::TypeScriptExtractor;
use std::collections::HashMap;
use tree_sitter::Node;

/// Infer types from variable assignments and function returns
pub(crate) fn infer_types(
    extractor: &TypeScriptExtractor,
    symbols: &[Symbol],
) -> HashMap<String, String> {
    let mut types = HashMap::new();

    if let Ok(tree) = parse_content(extractor) {
        infer_types_from_tree(extractor, tree.root_node(), symbols, &mut types);
    }

    types
}

/// Parse content using the tree-sitter parser
fn parse_content(
    extractor: &TypeScriptExtractor,
) -> Result<tree_sitter::Tree, Box<dyn std::error::Error>> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_javascript::LANGUAGE.into())?;
    let tree = parser
        .parse(&extractor.base().content, None)
        .ok_or("Failed to parse content")?;
    Ok(tree)
}

/// Recursively infer types from tree nodes
pub(crate) fn infer_types_from_tree(
    extractor: &TypeScriptExtractor,
    node: Node,
    symbols: &[Symbol],
    types: &mut HashMap<String, String>,
) {
    // Look for variable declarations and assignments
    if node.kind() == "variable_declarator" {
        if let Some(name_node) = node.child_by_field_name("name") {
            let var_name = extractor.base().get_node_text(&name_node);

            // Find the symbol for this variable
            if let Some(symbol) = symbols.iter().find(|s| s.name == var_name) {
                // Look at the value to infer the type
                if let Some(value_node) = node.child_by_field_name("value") {
                    let inferred_type = infer_type_from_value(extractor, &value_node);
                    types.insert(symbol.id.clone(), inferred_type);
                }
            }
        }
    }
    // Look for function declarations
    else if node.kind() == "function_declaration"
        || node.kind() == "arrow_function"
        || node.kind() == "function_expression"
    {
        if let Some(name_node) = node.child_by_field_name("name") {
            let func_name = extractor.base().get_node_text(&name_node);

            // Find the function symbol
            if let Some(symbol) = symbols.iter().find(|s| s.name == func_name) {
                let return_type = infer_function_return_type(extractor, &node);
                types.insert(symbol.id.clone(), return_type);
            }
        }
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        infer_types_from_tree(extractor, child, symbols, types);
    }
}

/// Infer type from a value node
pub(crate) fn infer_type_from_value(extractor: &TypeScriptExtractor, value_node: &Node) -> String {
    match value_node.kind() {
        "string" => "string".to_string(),
        "number" => "number".to_string(),
        "true" | "false" => "boolean".to_string(),
        "array" => "array".to_string(),
        "object" => "object".to_string(),
        "null" => "null".to_string(),
        "undefined" => "undefined".to_string(),
        "arrow_function" | "function" | "function_expression" => "function".to_string(),
        "call_expression" => {
            // Try to infer based on common function calls
            if let Some(function_node) = value_node.child_by_field_name("function") {
                let function_name = extractor.base().get_node_text(&function_node);
                match function_name.as_str() {
                    "fetch" => "Promise<Response>".to_string(),
                    "Promise.resolve" => "Promise<any>".to_string(),
                    "JSON.parse" => "any".to_string(),
                    "JSON.stringify" => "string".to_string(),
                    _ => "any".to_string(),
                }
            } else {
                "any".to_string()
            }
        }
        _ => "any".to_string(),
    }
}

/// Infer return type of a function
pub(crate) fn infer_function_return_type(
    extractor: &TypeScriptExtractor,
    func_node: &Node,
) -> String {
    // Check for async functions
    let is_async = func_node
        .children(&mut func_node.walk())
        .any(|child| child.kind() == "async");

    if is_async {
        return "Promise<any>".to_string();
    }

    // Look for return statements in the function body
    if let Some(body_node) = func_node.child_by_field_name("body") {
        let mut return_types = Vec::new();
        collect_return_types(extractor, &body_node, &mut return_types);

        if !return_types.is_empty() {
            // If we found return statements, try to unify types
            if return_types.iter().all(|t| t == "string") {
                return "string".to_string();
            } else if return_types.iter().all(|t| t == "number") {
                return "number".to_string();
            } else if return_types.iter().all(|t| t == "boolean") {
                return "boolean".to_string();
            }
            // Mixed types or complex types
            return "any".to_string();
        }
    }

    // Default to function type
    "function".to_string()
}

/// Collect return types from a node's tree
pub(crate) fn collect_return_types(
    extractor: &TypeScriptExtractor,
    node: &Node,
    return_types: &mut Vec<String>,
) {
    if node.kind() == "return_statement" {
        if let Some(value_node) = node.child_by_field_name("argument") {
            let return_type = infer_type_from_value(extractor, &value_node);
            return_types.push(return_type);
        }
    }

    // Recursively search children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_return_types(extractor, &child, return_types);
    }
}
