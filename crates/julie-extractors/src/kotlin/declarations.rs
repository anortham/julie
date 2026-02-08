//! Declaration extraction for Kotlin
//!
//! This module handles extraction of functions, packages, imports,
//! and type aliases. Split from types.rs for file size compliance.

use super::helpers;
use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract a Kotlin function declaration
pub(super) fn extract_function(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "identifier");
    let name = name_node
        .map(|n| base.get_node_text(&n))?;

    let modifiers = helpers::extract_modifiers(base, node);
    let type_params = helpers::extract_type_parameters(base, node);
    let receiver_type = helpers::extract_receiver_type(base, node);
    let parameters = helpers::extract_parameters(base, node);
    let return_type = helpers::extract_return_type(base, node);

    // Correct Kotlin signature order: modifiers + fun + typeParams + name
    let mut signature = "fun".to_string();

    if !modifiers.is_empty() {
        signature = format!("{} {}", modifiers.join(" "), signature);
    }

    if let Some(type_params) = type_params {
        signature.push_str(&format!(" {}", type_params));
    }

    // Add receiver type for extension functions (e.g., String.functionName)
    if let Some(receiver_type) = receiver_type {
        signature.push_str(&format!(" {}.{}", receiver_type, name));
    } else {
        signature.push_str(&format!(" {}", name));
    }

    signature.push_str(&parameters.unwrap_or_else(|| "()".to_string()));

    if let Some(return_type) = return_type {
        signature.push_str(&format!(": {}", return_type));
    }

    // Check for where clause (sibling node)
    if let Some(where_clause) = helpers::extract_where_clause(base, node) {
        signature.push_str(&format!(" {}", where_clause));
    }

    // Check for expression body (= expression)
    let function_body = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "function_body");
    if let Some(function_body) = function_body {
        let body_text = base.get_node_text(&function_body);
        if body_text.starts_with('=') {
            signature.push_str(&format!(" {}", body_text));
        }
    }

    // Determine symbol kind based on modifiers and context
    let symbol_kind = if modifiers.contains(&"operator".to_string()) {
        SymbolKind::Operator
    } else if parent_id.is_some() {
        SymbolKind::Method
    } else {
        SymbolKind::Function
    };

    let visibility = helpers::determine_visibility(&modifiers);
    let return_type = helpers::extract_return_type(base, node);

    let mut metadata = HashMap::from([
        (
            "type".to_string(),
            Value::String(
                if parent_id.is_some() {
                    "method"
                } else {
                    "function"
                }
                .to_string(),
            ),
        ),
        ("modifiers".to_string(), Value::String(modifiers.join(","))),
    ]);

    // Store return type for type inference
    if let Some(return_type) = return_type {
        metadata.insert("returnType".to_string(), Value::String(return_type));
    }

    // Extract KDoc comment
    let doc_comment = base.find_doc_comment(node);

    Some(base.create_symbol(
        node,
        name,
        symbol_kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

/// Extract a Kotlin package declaration
pub(super) fn extract_package(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Look for qualified_identifier which contains the full package name
    let name = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "qualified_identifier")
        .map(|n| base.get_node_text(&n))?;

    // Extract KDoc comment
    let doc_comment = base.find_doc_comment(node);

    Some(base.create_symbol(
        node,
        name.clone(),
        SymbolKind::Namespace,
        SymbolOptions {
            signature: Some(format!("package {}", name)),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([(
                "type".to_string(),
                Value::String("package".to_string()),
            )])),
            doc_comment,
        },
    ))
}

/// Extract a Kotlin import statement
pub(super) fn extract_import(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Look for qualified_identifier which contains the full import name
    let name = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "qualified_identifier")
        .map(|n| base.get_node_text(&n))?;

    // Extract KDoc comment
    let doc_comment = base.find_doc_comment(node);

    Some(base.create_symbol(
        node,
        name.clone(),
        SymbolKind::Import,
        SymbolOptions {
            signature: Some(format!("import {}", name)),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([(
                "type".to_string(),
                Value::String("import".to_string()),
            )])),
            doc_comment,
        },
    ))
}

/// Extract a Kotlin type alias
pub(super) fn extract_type_alias(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "identifier")
        .map(|n| base.get_node_text(&n))?;

    let modifiers = helpers::extract_modifiers(base, node);
    let type_params = helpers::extract_type_parameters(base, node);

    // Find the aliased type (after =) - may consist of multiple nodes
    let mut aliased_type = String::new();
    let children: Vec<Node> = node.children(&mut node.walk()).collect();
    if let Some(equal_index) = children.iter().position(|n| base.get_node_text(n) == "=") {
        if equal_index + 1 < children.len() {
            // Concatenate all nodes after the = (e.g., "suspend" + "(T) -> Unit")
            let type_nodes = &children[equal_index + 1..];
            aliased_type = type_nodes
                .iter()
                .map(|n| base.get_node_text(n))
                .collect::<Vec<String>>()
                .join(" ");
        }
    }

    let mut signature = format!("typealias {}", name);

    if !modifiers.is_empty() {
        signature = format!("{} {}", modifiers.join(" "), signature);
    }

    if let Some(type_params) = type_params {
        signature.push_str(&type_params);
    }

    if !aliased_type.is_empty() {
        signature.push_str(&format!(" = {}", aliased_type));
    }

    let visibility = helpers::determine_visibility(&modifiers);

    // Extract KDoc comment
    let doc_comment = base.find_doc_comment(node);

    Some(base.create_symbol(
        node,
        name,
        SymbolKind::Type,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([
                ("type".to_string(), Value::String("typealias".to_string())),
                ("modifiers".to_string(), Value::String(modifiers.join(","))),
                ("aliasedType".to_string(), Value::String(aliased_type)),
            ])),
            doc_comment,
        },
    ))
}
