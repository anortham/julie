//! Property and constructor parameter extraction for Kotlin
//!
//! This module handles extraction of properties, constructor parameters,
//! and related metadata.

use super::helpers;
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract a Kotlin property declaration
pub(super) fn extract_property(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Symbol {
    // Look for name in variable_declaration first (the proper place for property names)
    let mut name_node = None;
    let var_decl = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "variable_declaration");
    if let Some(var_decl) = var_decl {
        name_node = var_decl
            .children(&mut var_decl.walk())
            .find(|n| n.kind() == "identifier");
    }

    // Fallback: look for identifier at top level (for interface properties)
    if name_node.is_none() {
        name_node = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "identifier");
    }
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "unknownProperty".to_string());

    let modifiers = helpers::extract_modifiers(base, node);
    let property_type = helpers::extract_property_type(base, node);

    // Check for val/var in binding_pattern_kind for interface properties
    let mut is_val = node.children(&mut node.walk()).any(|n| n.kind() == "val");
    let mut is_var = node.children(&mut node.walk()).any(|n| n.kind() == "var");

    if !is_val && !is_var {
        let binding_pattern = node
            .children(&mut node.walk())
            .find(|n| n.kind() == "binding_pattern_kind");
        if let Some(binding_pattern) = binding_pattern {
            is_val = binding_pattern
                .children(&mut binding_pattern.walk())
                .any(|n| n.kind() == "val");
            is_var = binding_pattern
                .children(&mut binding_pattern.walk())
                .any(|n| n.kind() == "var");
        }
    }

    let binding = if is_val {
        "val"
    } else if is_var {
        "var"
    } else {
        "val"
    };
    let mut signature = format!("{} {}", binding, name);

    if !modifiers.is_empty() {
        signature = format!("{} {}", modifiers.join(" "), signature);
    }

    if let Some(property_type) = property_type {
        signature.push_str(&format!(": {}", property_type));
    }

    // Add initializer value if present (especially for const val)
    if let Some(initializer) = helpers::extract_property_initializer(base, node) {
        signature.push_str(&format!(" = {}", initializer));
    }

    // Check for property delegation (by lazy, by Delegates.notNull(), etc.)
    if let Some(delegation) = helpers::extract_property_delegation(base, node) {
        signature.push_str(&format!(" {}", delegation));
    }

    // Determine symbol kind - const val should be Constant
    let is_const = modifiers.contains(&"const".to_string());
    let symbol_kind = if is_const && is_val {
        SymbolKind::Constant
    } else {
        SymbolKind::Property
    };

    let visibility = helpers::determine_visibility(&modifiers);
    let property_type = helpers::extract_property_type(base, node);

    let mut metadata = HashMap::from([
        (
            "type".to_string(),
            Value::String(if is_const { "constant" } else { "property" }.to_string()),
        ),
        ("modifiers".to_string(), Value::String(modifiers.join(","))),
        ("isVal".to_string(), Value::String(is_val.to_string())),
        ("isVar".to_string(), Value::String(is_var.to_string())),
    ]);

    // Store property type for type inference
    if let Some(property_type) = property_type {
        metadata.insert("propertyType".to_string(), Value::String(property_type));
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

/// Extract constructor parameters and create symbols for them
pub(super) fn extract_constructor_parameters(
    base: &mut BaseExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) {
    // First find the class_parameters container, then extract class_parameter nodes as properties
    let class_parameters = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "class_parameters");

    if let Some(class_parameters) = class_parameters {
        for child in class_parameters.children(&mut class_parameters.walk()) {
            if child.kind() == "class_parameter" {
                let name_node = child
                    .children(&mut child.walk())
                    .find(|n| n.kind() == "identifier");
                let name = name_node
                    .map(|n| base.get_node_text(&n))
                    .unwrap_or_else(|| "unknownParam".to_string());

                // Get binding pattern (val/var)
                let binding_node = child
                    .children(&mut child.walk())
                    .find(|n| matches!(n.kind(), "val" | "var"));
                let binding = binding_node
                    .map(|n| base.get_node_text(&n))
                    .unwrap_or_else(|| "val".to_string());

                // Get type (handle various type node structures including nullable)
                let type_node = child.children(&mut child.walk()).find(|n| {
                    matches!(
                        n.kind(),
                        "user_type" | "type" | "nullable_type" | "type_reference"
                    )
                });
                let param_type = type_node
                    .map(|n| base.get_node_text(&n))
                    .unwrap_or_else(|| "".to_string());

                // Get modifiers (like private)
                let modifiers_node = child
                    .children(&mut child.walk())
                    .find(|n| n.kind() == "modifiers");
                let modifiers = modifiers_node
                    .map(|n| base.get_node_text(&n))
                    .unwrap_or_else(|| "".to_string());

                // Get default value (handle various literal types and expressions)
                let default_value = child.children(&mut child.walk()).find(|n| {
                    matches!(
                        n.kind(),
                        "number_literal"
                            | "string_literal"
                            | "boolean_literal"
                            | "expression"
                            | "call_expression"
                    )
                });
                let default_val = default_value
                    .map(|n| format!(" = {}", base.get_node_text(&n)))
                    .unwrap_or_else(|| "".to_string());

                // Alternative: look for assignment pattern (= value)
                let final_signature = if default_val.is_empty() {
                    let children: Vec<Node> = child.children(&mut child.walk()).collect();
                    if let Some(equal_index) =
                        children.iter().position(|n| base.get_node_text(n) == "=")
                    {
                        if equal_index + 1 < children.len() {
                            let value_node = &children[equal_index + 1];
                            let default_assignment =
                                format!(" = {}", base.get_node_text(value_node));
                            let signature2 = format!("{} {}", binding, name);
                            let final_sig = if !param_type.is_empty() {
                                format!("{}: {}{}", signature2, param_type, default_assignment)
                            } else {
                                format!("{}{}", signature2, default_assignment)
                            };
                            if !modifiers.is_empty() {
                                format!("{} {}", modifiers, final_sig)
                            } else {
                                final_sig
                            }
                        } else {
                            format!("{} {}", binding, name)
                        }
                    } else {
                        format!("{} {}", binding, name)
                    }
                } else {
                    // Build signature
                    let mut signature = format!("{} {}", binding, name);
                    if !param_type.is_empty() {
                        signature.push_str(&format!(": {}", param_type));
                    }
                    signature.push_str(&default_val);

                    // Add modifiers to signature if present
                    if !modifiers.is_empty() {
                        format!("{} {}", modifiers, signature)
                    } else {
                        signature
                    }
                };

                // Determine visibility
                let visibility = if modifiers.contains("private") {
                    Visibility::Private
                } else if modifiers.contains("protected") {
                    Visibility::Protected
                } else {
                    Visibility::Public
                };

                // Extract KDoc comment
                let doc_comment = base.find_doc_comment(&child);

                let property_symbol = base.create_symbol(
                    &child,
                    name,
                    SymbolKind::Property,
                    SymbolOptions {
                        signature: Some(final_signature),
                        visibility: Some(visibility),
                        parent_id: parent_id.map(|s| s.to_string()),
                        metadata: Some(HashMap::from([
                            ("type".to_string(), Value::String("property".to_string())),
                            ("binding".to_string(), Value::String(binding)),
                            ("dataType".to_string(), Value::String(param_type)),
                            (
                                "hasDefaultValue".to_string(),
                                Value::String((!default_val.is_empty()).to_string()),
                            ),
                        ])),
                        doc_comment,
                    },
                );

                symbols.push(property_symbol);
            }
        }
    }
}
