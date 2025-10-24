//! Type and symbol extraction for Kotlin
//!
//! This module handles extraction of classes, interfaces, objects, functions,
//! properties, and other type declarations.

use super::helpers;
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract a Kotlin class declaration
pub(super) fn extract_class(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Symbol {
    let name_node = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "identifier");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "UnknownClass".to_string());

    // Check if this is actually an interface by looking for 'interface' child node
    let is_interface = node
        .children(&mut node.walk())
        .any(|n| n.kind() == "interface");

    let modifiers = helpers::extract_modifiers(base, node);
    let type_params = helpers::extract_type_parameters(base, node);
    let super_types = helpers::extract_super_types(base, node);
    let constructor_params = helpers::extract_primary_constructor_signature(base, node);

    // Determine if this is an enum class
    let is_enum = helpers::determine_class_kind(base, &modifiers, node) == SymbolKind::Enum;

    // Check for fun interface by looking for direct 'fun' child
    let has_fun_keyword = node
        .children(&mut node.walk())
        .any(|n| base.get_node_text(&n) == "fun");

    let mut signature = if is_interface {
        if has_fun_keyword {
            format!("fun interface {}", name)
        } else {
            format!("interface {}", name)
        }
    } else if is_enum {
        format!("enum class {}", name)
    } else {
        format!("class {}", name)
    };

    // For enum classes, don't include 'enum' in modifiers since it's already in the signature
    // For fun interfaces, don't include 'fun' in modifiers since it's already in the signature
    let final_modifiers: Vec<String> = if is_enum {
        modifiers.into_iter().filter(|m| m != "enum").collect()
    } else if has_fun_keyword {
        modifiers.into_iter().filter(|m| m != "fun").collect()
    } else {
        modifiers
    };

    if !final_modifiers.is_empty() {
        signature = format!("{} {}", final_modifiers.join(" "), signature);
    }

    if let Some(type_params) = type_params {
        signature.push_str(&type_params);
    }

    // Add primary constructor parameters to signature if present
    if let Some(constructor_params) = constructor_params {
        signature.push_str(&constructor_params);
    }

    if let Some(super_types) = super_types {
        signature.push_str(&format!(" : {}", super_types));
    }

    let symbol_kind = if is_interface {
        SymbolKind::Interface
    } else {
        helpers::determine_class_kind(base, &final_modifiers, node)
    };

    let visibility = helpers::determine_visibility(&final_modifiers);

    // Extract KDoc comment
    let doc_comment = base.find_doc_comment(node);

    base.create_symbol(
        node,
        name,
        symbol_kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([
                ("type".to_string(), Value::String("class".to_string())),
                (
                    "modifiers".to_string(),
                    Value::String(final_modifiers.join(",")),
                ),
            ])),
            doc_comment,
        },
    )
}

/// Extract a Kotlin interface declaration
pub(super) fn extract_interface(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Symbol {
    let name_node = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "identifier");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "UnknownInterface".to_string());

    let modifiers = helpers::extract_modifiers(base, node);
    let type_params = helpers::extract_type_parameters(base, node);
    let super_types = helpers::extract_super_types(base, node);

    let mut signature = format!("interface {}", name);

    if !modifiers.is_empty() {
        signature = format!("{} {}", modifiers.join(" "), signature);
    }

    if let Some(type_params) = type_params {
        signature.push_str(&type_params);
    }

    if let Some(super_types) = super_types {
        signature.push_str(&format!(" : {}", super_types));
    }

    let visibility = helpers::determine_visibility(&modifiers);

    // Extract KDoc comment
    let doc_comment = base.find_doc_comment(node);

    base.create_symbol(
        node,
        name,
        SymbolKind::Interface,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([
                ("type".to_string(), Value::String("interface".to_string())),
                ("modifiers".to_string(), Value::String(modifiers.join(","))),
            ])),
            doc_comment,
        },
    )
}

/// Extract a Kotlin object declaration
pub(super) fn extract_object(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Symbol {
    let name_node = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "identifier");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "UnknownObject".to_string());

    let modifiers = helpers::extract_modifiers(base, node);
    let super_types = helpers::extract_super_types(base, node);

    let mut signature = format!("object {}", name);

    if !modifiers.is_empty() {
        signature = format!("{} {}", modifiers.join(" "), signature);
    }

    if let Some(super_types) = super_types {
        signature.push_str(&format!(" : {}", super_types));
    }

    let visibility = helpers::determine_visibility(&modifiers);

    // Extract KDoc comment
    let doc_comment = base.find_doc_comment(node);

    base.create_symbol(
        node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([
                ("type".to_string(), Value::String("object".to_string())),
                ("modifiers".to_string(), Value::String(modifiers.join(","))),
            ])),
            doc_comment,
        },
    )
}

/// Extract a Kotlin companion object
pub(super) fn extract_companion_object(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Symbol {
    // Companion objects always have the name "Companion"
    let name = "Companion".to_string();

    let mut signature = "companion object".to_string();

    // Check if companion object has a custom name
    let name_node = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "identifier");
    if let Some(name_node) = name_node {
        let custom_name = base.get_node_text(&name_node);
        signature.push_str(&format!(" {}", custom_name));
    }

    // Extract KDoc comment
    let doc_comment = base.find_doc_comment(node);

    base.create_symbol(
        node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([(
                "type".to_string(),
                Value::String("companion-object".to_string()),
            )])),
            doc_comment,
        },
    )
}

/// Extract a Kotlin function declaration
pub(super) fn extract_function(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Symbol {
    let name_node = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "identifier");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "unknownFunction".to_string());

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

    base.create_symbol(
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
    )
}

/// Extract a Kotlin package declaration
pub(super) fn extract_package(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Symbol {
    // Look for qualified_identifier which contains the full package name
    let name_node = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "qualified_identifier");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "UnknownPackage".to_string());

    // Extract KDoc comment
    let doc_comment = base.find_doc_comment(node);

    base.create_symbol(
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
    )
}

/// Extract a Kotlin import statement
pub(super) fn extract_import(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Symbol {
    // Look for qualified_identifier which contains the full import name
    let name_node = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "qualified_identifier");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "UnknownImport".to_string());

    // Extract KDoc comment
    let doc_comment = base.find_doc_comment(node);

    base.create_symbol(
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
    )
}

/// Extract a Kotlin type alias
pub(super) fn extract_type_alias(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Symbol {
    let name_node = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "identifier");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "UnknownTypeAlias".to_string());

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

    base.create_symbol(
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
    )
}

/// Extract enum members from an enum class body
pub(super) fn extract_enum_members(
    base: &mut BaseExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) {
    for child in node.children(&mut node.walk()) {
        if child.kind() == "enum_entry" {
            let name_node = child
                .children(&mut child.walk())
                .find(|n| n.kind() == "identifier");
            if let Some(name_node) = name_node {
                let name = base.get_node_text(&name_node);

                // Check for constructor parameters
                let mut signature = name.clone();
                let value_args = child
                    .children(&mut child.walk())
                    .find(|n| n.kind() == "value_arguments");
                if let Some(value_args) = value_args {
                    let args = base.get_node_text(&value_args);
                    signature.push_str(&args);
                }

                // Extract KDoc comment
                let doc_comment = base.find_doc_comment(&child);

                let symbol = base.create_symbol(
                    &child,
                    name,
                    SymbolKind::EnumMember,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(Visibility::Public),
                        parent_id: parent_id.map(|s| s.to_string()),
                        metadata: Some(HashMap::from([(
                            "type".to_string(),
                            Value::String("enum-member".to_string()),
                        )])),
                        doc_comment,
                    },
                );
                symbols.push(symbol);
            }
        }
    }
}
