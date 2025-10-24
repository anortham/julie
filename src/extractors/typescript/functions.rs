//! Function and method extraction
//!
//! This module handles extraction of function declarations, methods, constructors,
//! and arrow functions assigned to variables.

use super::helpers;
use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions};
use crate::extractors::typescript::TypeScriptExtractor;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract a function declaration or arrow function
pub(super) fn extract_function(extractor: &mut TypeScriptExtractor, node: Node) -> Symbol {
    let name_node = node.child_by_field_name("name");
    let mut name = if let Some(name_node) = name_node {
        extractor.base().get_node_text(&name_node)
    } else {
        "Anonymous".to_string()
    };

    // Handle arrow functions assigned to variables
    if node.kind() == "arrow_function" {
        if let Some(parent) = node.parent() {
            if parent.kind() == "variable_declarator" {
                if let Some(var_name_node) = parent.child_by_field_name("name") {
                    name = extractor.base().get_node_text(&var_name_node);
                }
            }
        }
    }

    let signature = build_function_signature(extractor, &node, &name);
    let visibility = extractor.base().extract_visibility(&node);

    // Check for modifiers
    let is_async = helpers::has_modifier(node, "async");
    let is_generator = helpers::has_modifier(node, "*");

    let parameters = extract_parameters(extractor, &node);
    let return_type = extractor.base().get_field_text(&node, "return_type");
    let type_parameters = extract_type_parameters(extractor, &node);

    let mut metadata = HashMap::new();
    metadata.insert("isAsync".to_string(), serde_json::json!(is_async));
    metadata.insert("isGenerator".to_string(), serde_json::json!(is_generator));
    metadata.insert("parameters".to_string(), serde_json::json!(parameters));
    if let Some(return_type) = return_type {
        metadata.insert("returnType".to_string(), serde_json::json!(return_type));
    }
    metadata.insert(
        "typeParameters".to_string(),
        serde_json::json!(type_parameters),
    );

    // Extract JSDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    // CRITICAL FIX: Symbol must span entire function body for containment logic,
    // but ID should be generated from function name position (not body start).
    let mut symbol = extractor.base_mut().create_symbol(
        &node,
        name.clone(),
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility,
            parent_id: None,
            metadata: Some(metadata),
            doc_comment,
        },
    );

    // Regenerate ID using function name position (not body start)
    if let Some(name_node) = node.child_by_field_name("name") {
        let start_pos = name_node.start_position();
        let new_id =
            extractor
                .base()
                .generate_id(&name, start_pos.row as u32, start_pos.column as u32);

        let old_id = symbol.id.clone();
        symbol.id = new_id.clone();
        extractor.base_mut().symbol_map.remove(&old_id);
        extractor
            .base_mut()
            .symbol_map
            .insert(new_id, symbol.clone());
    } else if node.kind() == "arrow_function" {
        if let Some(parent) = node.parent() {
            if parent.kind() == "variable_declarator" {
                if let Some(var_name_node) = parent.child_by_field_name("name") {
                    let start_pos = var_name_node.start_position();
                    let new_id = extractor.base().generate_id(
                        &name,
                        start_pos.row as u32,
                        start_pos.column as u32,
                    );

                    let old_id = symbol.id.clone();
                    symbol.id = new_id.clone();
                    extractor.base_mut().symbol_map.remove(&old_id);
                    extractor
                        .base_mut()
                        .symbol_map
                        .insert(new_id, symbol.clone());
                }
            }
        }
    }

    symbol
}

/// Extract a method definition (inside a class)
pub(super) fn extract_method(extractor: &mut TypeScriptExtractor, node: Node) -> Symbol {
    let name_node = node.child_by_field_name("name");
    let name = if let Some(name_node) = name_node {
        extractor.base().get_node_text(&name_node)
    } else {
        "Anonymous".to_string()
    };

    // Determine if this is a constructor
    let symbol_kind = if name == "constructor" {
        SymbolKind::Constructor
    } else {
        SymbolKind::Method
    };

    let signature = build_function_signature(extractor, &node, &name);
    let visibility = extractor.base().extract_visibility(&node);

    // Check for modifiers
    let is_async = helpers::has_modifier(node, "async");
    let is_static = helpers::has_modifier(node, "static");
    let is_generator = helpers::has_modifier(node, "*");

    let parameters = extract_parameters(extractor, &node);
    let return_type = extractor.base().get_field_text(&node, "return_type");
    let type_parameters = extract_type_parameters(extractor, &node);

    let mut metadata = HashMap::new();
    metadata.insert("isAsync".to_string(), serde_json::json!(is_async));
    metadata.insert("isStatic".to_string(), serde_json::json!(is_static));
    metadata.insert("isGenerator".to_string(), serde_json::json!(is_generator));
    metadata.insert("parameters".to_string(), serde_json::json!(parameters));
    if let Some(return_type) = return_type {
        metadata.insert("returnType".to_string(), serde_json::json!(return_type));
    }
    metadata.insert(
        "typeParameters".to_string(),
        serde_json::json!(type_parameters),
    );

    // Find parent class
    let parent_id = find_parent_class_id(extractor, &node);

    // Extract JSDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    // CRITICAL FIX: Keep full body span for containment
    let mut symbol = extractor.base_mut().create_symbol(
        &node,
        name.clone(),
        symbol_kind,
        SymbolOptions {
            signature: Some(signature),
            visibility,
            parent_id,
            metadata: Some(metadata),
            doc_comment,
        },
    );

    // Regenerate ID using method name position
    if let Some(name_node) = node.child_by_field_name("name") {
        let start_pos = name_node.start_position();
        let new_id =
            extractor
                .base()
                .generate_id(&name, start_pos.row as u32, start_pos.column as u32);

        let old_id = symbol.id.clone();
        symbol.id = new_id.clone();
        extractor.base_mut().symbol_map.remove(&old_id);
        extractor
            .base_mut()
            .symbol_map
            .insert(new_id, symbol.clone());
    }

    symbol
}

/// Extract a variable declarator
pub(super) fn extract_variable(extractor: &mut TypeScriptExtractor, node: Node) -> Symbol {
    let name_node = node.child_by_field_name("name");
    let name = if let Some(name_node) = name_node {
        extractor.base().get_node_text(&name_node)
    } else {
        "Anonymous".to_string()
    };

    // Check if this variable contains an arrow function
    if let Some(value_node) = node.child_by_field_name("value") {
        if value_node.kind() == "arrow_function" {
            // Extract as a function instead of a variable
            return extract_function(extractor, value_node);
        }
    }

    // Extract JSDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Variable,
        SymbolOptions {
            doc_comment,
            ..Default::default()
        },
    )
}

/// Build a function signature string (e.g., "foo(x, y): string")
fn build_function_signature(extractor: &TypeScriptExtractor, node: &Node, name: &str) -> String {
    let params = extractor
        .base()
        .get_field_text(node, "parameters")
        .or_else(|| extractor.base().get_field_text(node, "formal_parameters"))
        .unwrap_or_else(|| "()".to_string());
    let return_type = extractor.base().get_field_text(node, "return_type");

    let mut signature = format!("{}{}", name, params);
    if let Some(return_type) = return_type {
        signature.push_str(&format!(": {}", return_type));
    }

    signature
}

/// Extract type parameters from a function (e.g., <T, U> in generics)
fn extract_type_parameters(extractor: &TypeScriptExtractor, node: &Node) -> Vec<String> {
    if let Some(type_params) = node.child_by_field_name("type_parameters") {
        let mut params = Vec::new();
        let mut cursor = type_params.walk();
        for child in type_params.children(&mut cursor) {
            if child.kind() == "type_parameter" {
                params.push(extractor.base().get_node_text(&child));
            }
        }
        params
    } else {
        Vec::new()
    }
}

/// Extract function parameters
fn extract_parameters(extractor: &TypeScriptExtractor, node: &Node) -> Vec<String> {
    if let Some(params) = node.child_by_field_name("parameters") {
        let mut parameters = Vec::new();
        let mut cursor = params.walk();
        for child in params.children(&mut cursor) {
            if child.kind() == "parameter" || child.kind() == "identifier" {
                parameters.push(extractor.base().get_node_text(&child));
            }
        }
        parameters
    } else {
        Vec::new()
    }
}

/// Find the parent class ID for a method
fn find_parent_class_id(extractor: &TypeScriptExtractor, node: &Node) -> Option<String> {
    let mut current = node.parent();
    while let Some(parent_node) = current {
        if parent_node.kind() == "class_declaration" {
            if let Some(class_name_node) = parent_node.child_by_field_name("name") {
                let class_name = extractor.base().get_node_text(&class_name_node);
                let class_start = parent_node.start_position();
                let candidates = [
                    extractor.base().generate_id(
                        &class_name,
                        class_start.row as u32,
                        class_start.column as u32,
                    ),
                    extractor.base().generate_id(
                        &class_name,
                        class_name_node.start_position().row as u32,
                        class_name_node.start_position().column as u32,
                    ),
                ];

                for candidate in candidates {
                    if extractor.base().symbol_map.contains_key(&candidate) {
                        return Some(candidate);
                    }
                }

                if let Some((id, _symbol)) =
                    extractor.base().symbol_map.iter().find(|(_, symbol)| {
                        symbol.name == class_name && symbol.kind == SymbolKind::Class
                    })
                {
                    return Some(id.clone());
                }
            }
        }
        current = parent_node.parent();
    }
    None
}
