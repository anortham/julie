/// Function and method extraction
/// Handles regular functions, async functions, lambdas, and method detection
use super::super::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use super::PythonExtractor;
use super::{decorators, signatures};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract a regular function definition
pub fn extract_function(extractor: &mut PythonExtractor, node: Node) -> Symbol {
    // Extract function name from 'name' field
    let name = if let Some(name_node) = node.child_by_field_name("name") {
        extractor.base_mut().get_node_text(&name_node)
    } else {
        "Anonymous".to_string()
    };

    // Check if it's an async function
    let is_async = signatures::has_async_keyword(&node);

    // Extract parameters from 'parameters' field
    let parameters_node = node.child_by_field_name("parameters");
    let params = if let Some(parameters_node) = parameters_node {
        signatures::extract_parameters(extractor, &parameters_node)
    } else {
        Vec::new()
    };

    // Extract return type annotation from 'return_type' field
    let return_type = if let Some(return_type_node) = node.child_by_field_name("return_type") {
        format!(
            ": {}",
            extractor.base_mut().get_node_text(&return_type_node)
        )
    } else {
        String::new()
    };

    // Extract decorators
    let decorators_list = decorators::extract_decorators(extractor, &node);
    let decorator_info = if decorators_list.is_empty() {
        String::new()
    } else {
        format!("@{} ", decorators_list.join(" @"))
    };

    // Build signature
    let async_prefix = if is_async { "async " } else { "" };
    let signature = format!(
        "{}{}def {}({}){}",
        decorator_info,
        async_prefix,
        name,
        params.join(", "),
        return_type
    );

    // Determine if it's a method or function based on context
    let (symbol_kind, parent_id) = determine_function_kind(extractor, &node, &name);

    // Extract docstring
    let doc_comment = super::types::extract_docstring(extractor, &node);

    // Infer visibility from name
    let visibility = signatures::infer_visibility(&name);

    let mut metadata = HashMap::new();
    metadata.insert("decorators".to_string(), serde_json::json!(decorators_list));
    metadata.insert("isAsync".to_string(), serde_json::json!(is_async));
    metadata.insert("returnType".to_string(), serde_json::json!(return_type));

    extractor.base_mut().create_symbol(
        &node,
        name,
        symbol_kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id,
            metadata: Some(metadata),
            doc_comment,
        },
    )
}

/// Extract an async function definition
pub fn extract_async_function(extractor: &mut PythonExtractor, node: Node) -> Symbol {
    // Async functions are handled the same way as regular functions
    // The has_async_keyword check will detect the async keyword
    extract_function(extractor, node)
}

/// Extract a lambda expression
pub(super) fn extract_lambda(extractor: &mut PythonExtractor, node: Node) -> Symbol {
    // Extract lambda parameters
    let parameters_node = node.child_by_field_name("parameters");
    let params = if let Some(parameters_node) = parameters_node {
        signatures::extract_parameters(extractor, &parameters_node)
    } else {
        Vec::new()
    };

    // Extract lambda body (simplified)
    let body_node = node.child_by_field_name("body");
    let body = if let Some(body_node) = body_node {
        extractor.base_mut().get_node_text(&body_node)
    } else {
        String::new()
    };

    // Create signature: lambda params: body
    let signature = format!("lambda {}: {}", params.join(", "), body);

    // Create name with row number: <lambda:row>
    let start_pos = node.start_position();
    let name = format!("<lambda:{}>", start_pos.row);

    // Extract doc comment (preceding comments)
    let doc_comment = extractor.base().find_doc_comment(&node);

    extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: None, // Lambdas are typically inline and don't have meaningful parent relationships
            metadata: None,
            doc_comment,
        },
    )
}

/// Determine if a function is a method or standalone function
fn determine_function_kind(
    extractor: &PythonExtractor,
    node: &Node,
    name: &str,
) -> (SymbolKind, Option<String>) {
    // Check if this function is inside a class definition
    let mut current = *node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "class_definition" {
            // This is a method inside a class
            // Extract the class name to create parent_id
            let class_name = if let Some(name_node) = parent.child_by_field_name("name") {
                extractor.base().get_node_text(&name_node)
            } else {
                "Anonymous".to_string()
            };

            // Create parent_id using the same pattern as BaseExtractor
            let start_pos = parent.start_position();
            let parent_id = extractor.base().generate_id(
                &class_name,
                start_pos.row as u32,
                start_pos.column as u32,
            );

            // Determine method type
            let symbol_kind = if name == "__init__" {
                SymbolKind::Constructor
            } else {
                SymbolKind::Method
            };

            return (symbol_kind, Some(parent_id));
        }
        current = parent;
    }

    // Not inside a class, so it's a standalone function
    (SymbolKind::Function, None)
}
