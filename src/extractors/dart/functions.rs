// Dart Extractor - Functions, Methods, Classes, and Variables
//
// Methods for extracting functions, methods, classes, and related constructs

use super::helpers::*;
use super::signatures;
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract class definition
pub(super) fn extract_class(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = find_child_by_type(node, "identifier")?;
    let name = get_node_text(&name_node);

    // Check if it's a Flutter widget (extends StatelessWidget, StatefulWidget, etc.)
    let is_widget = is_flutter_widget(node);

    let mut symbol = base.create_symbol(
        node,
        name.clone(),
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signatures::extract_class_signature(node)),
            visibility: Some(Visibility::Public), // Dart classes are generally public unless private (_)
            parent_id: parent_id.map(|id| id.to_string()),
            metadata: Some(HashMap::new()),
            doc_comment: None,
        },
    );

    // Add Flutter widget annotation in documentation
    if is_widget {
        let doc = symbol.doc_comment.unwrap_or_default();
        symbol.doc_comment = Some(format!("{} [Flutter Widget]", doc).trim().to_string());
    }

    Some(symbol)
}

/// Extract function or top-level function
pub(super) fn extract_function(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = find_child_by_type(node, "identifier")?;
    let name = get_node_text(&name_node);

    let is_async = is_async_function(node, &base.content);
    let is_private = name.starts_with('_');

    // Use Method kind if inside a class (has parent_id), otherwise Function
    let symbol_kind = if parent_id.is_some() {
        SymbolKind::Method
    } else {
        SymbolKind::Function
    };

    let mut symbol = base.create_symbol(
        node,
        name,
        symbol_kind,
        SymbolOptions {
            signature: Some(signatures::extract_function_signature(node, &base.content)),
            visibility: Some(if is_private {
                Visibility::Private
            } else {
                Visibility::Public
            }),
            parent_id: parent_id.map(|id| id.to_string()),
            metadata: Some(HashMap::new()),
            ..Default::default()
        },
    );

    // Add async annotation
    if is_async {
        symbol
            .metadata
            .get_or_insert_with(HashMap::new)
            .insert("isAsync".to_string(), serde_json::Value::Bool(true));
    }

    Some(symbol)
}

/// Extract method
pub(super) fn extract_method(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // For method_signature nodes, look inside the nested function_signature
    let target_node = if node.kind() == "method_signature" {
        find_child_by_type(node, "function_signature").unwrap_or(*node)
    } else {
        *node
    };

    let name_node = find_child_by_type(&target_node, "identifier")?;
    let name = get_node_text(&name_node);

    let is_async = is_async_function(node, &base.content);
    let is_static = is_static_method(node);
    let is_private = name.starts_with('_');
    let is_override = is_override_method(node, &base.content);
    let is_flutter_lifecycle = is_flutter_lifecycle_method(&name);

    // Get the base function signature (return type + name + params)
    let base_signature = signatures::extract_function_signature(&target_node, &base.content);

    // Build method signature with modifiers
    let mut modifiers = Vec::new();
    if is_static {
        modifiers.push("static");
    }
    if is_async {
        modifiers.push("async");
    }
    if is_override {
        modifiers.push("@override");
    }

    let modifier_prefix = if modifiers.is_empty() {
        String::new()
    } else {
        format!("{} ", modifiers.join(" "))
    };
    let signature = format!("{}{}", modifier_prefix, base_signature);

    let mut symbol = base.create_symbol(
        node,
        name,
        SymbolKind::Method,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(if is_private {
                Visibility::Private
            } else {
                Visibility::Public
            }),
            parent_id: parent_id.map(|id| id.to_string()),
            metadata: Some(HashMap::new()),
            ..Default::default()
        },
    );

    // Add metadata
    symbol
        .metadata
        .get_or_insert_with(HashMap::new)
        .insert("isAsync".to_string(), serde_json::Value::Bool(is_async));
    symbol
        .metadata
        .get_or_insert_with(HashMap::new)
        .insert("isStatic".to_string(), serde_json::Value::Bool(is_static));
    symbol.metadata.get_or_insert_with(HashMap::new).insert(
        "isOverride".to_string(),
        serde_json::Value::Bool(is_override),
    );
    symbol.metadata.get_or_insert_with(HashMap::new).insert(
        "isFlutterLifecycle".to_string(),
        serde_json::Value::Bool(is_flutter_lifecycle),
    );

    Some(symbol)
}

/// Extract constructor
pub(super) fn extract_constructor(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Extract constructor name more precisely
    let constructor_name = match node.kind() {
        "factory_constructor_signature" => {
            // Factory constructor: factory ClassName.methodName
            let mut identifiers = Vec::new();
            traverse_tree(*node, &mut |child| {
                if child.kind() == "identifier" && identifiers.len() < 2 {
                    identifiers.push(get_node_text(&child));
                }
            });
            identifiers.join(".")
        }
        "constant_constructor_signature" => {
            // Const constructor: const ClassName(...) or const ClassName.namedConstructor(...)
            find_child_by_type(node, "identifier")
                .map(|n| get_node_text(&n))
                .unwrap_or_else(|| "Constructor".to_string())
        }
        _ => {
            // Regular constructor or named constructor
            let direct_children: Vec<_> = node
                .children(&mut node.walk())
                .filter(|child| child.kind() == "identifier")
                .collect();

            match direct_children.len() {
                1 => {
                    // Default constructor: ClassName()
                    get_node_text(&direct_children[0])
                }
                _ if direct_children.len() >= 2 => {
                    // Named constructor: ClassName.namedConstructor()
                    direct_children
                        .iter()
                        .take(2)
                        .map(|child| get_node_text(child))
                        .collect::<Vec<_>>()
                        .join(".")
                }
                _ => "Constructor".to_string(),
            }
        }
    };

    let is_factory = is_factory_constructor(node);
    let is_const = is_const_constructor(node);

    let mut symbol = base.create_symbol(
        node,
        constructor_name,
        SymbolKind::Constructor,
        SymbolOptions {
            signature: Some(signatures::extract_constructor_signature(node)),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|id| id.to_string()),
            metadata: Some(HashMap::new()),
            ..Default::default()
        },
    );

    // Add metadata
    symbol
        .metadata
        .get_or_insert_with(HashMap::new)
        .insert("isFactory".to_string(), serde_json::Value::Bool(is_factory));
    symbol
        .metadata
        .get_or_insert_with(HashMap::new)
        .insert("isConst".to_string(), serde_json::Value::Bool(is_const));

    Some(symbol)
}

/// Extract variable (top-level or field)
pub(super) fn extract_variable(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Simple iterative approach - search for the first initialized_variable_definition
    // This avoids the complexity of closures and lifetime issues
    let mut cursor = node.walk();

    // Look for initialized_variable_definition directly in children
    for child in node.children(&mut cursor) {
        if child.kind() == "initialized_variable_definition" {
            if let Some(name_node) = find_child_by_type(&child, "identifier") {
                let name = get_node_text(&name_node);
                let is_private = name.starts_with('_');
                let is_final = is_final_variable(&child);
                let is_const = is_const_variable(&child);

                let symbol_kind = if is_final || is_const {
                    SymbolKind::Constant
                } else {
                    SymbolKind::Variable
                };

                let mut symbol = base.create_symbol(
                    &child,
                    name,
                    symbol_kind,
                    SymbolOptions {
                        signature: Some(signatures::extract_variable_signature(&child)),
                        visibility: Some(if is_private {
                            Visibility::Private
                        } else {
                            Visibility::Public
                        }),
                        parent_id: parent_id.map(|id| id.to_string()),
                        metadata: Some(HashMap::new()),
                        ..Default::default()
                    },
                );

                // Add metadata
                symbol
                    .metadata
                    .get_or_insert_with(HashMap::new)
                    .insert("isFinal".to_string(), serde_json::Value::Bool(is_final));
                symbol
                    .metadata
                    .get_or_insert_with(HashMap::new)
                    .insert("isConst".to_string(), serde_json::Value::Bool(is_const));

                return Some(symbol);
            }
        }
    }

    None
}
