//! Property extraction for Scala (val/var)
//!
//! Handles immutable vals and mutable vars.

use super::helpers;
use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

fn val_kind_for_scope(node: &Node) -> SymbolKind {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        match ancestor.kind() {
            "function_definition" | "function_declaration" => return SymbolKind::Constant,
            "class_definition" | "object_definition" => return SymbolKind::Property,
            _ => {
                current = ancestor.parent();
            }
        }
    }

    SymbolKind::Constant
}

/// Extract a Scala val (immutable value)
pub(super) fn extract_val(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name = helpers::get_name(base, node)?;
    let modifiers = helpers::extract_modifiers(base, node);
    let annotations = helpers::extract_annotations(base, node);
    let return_type = helpers::extract_return_type(base, node);
    let symbol_kind = val_kind_for_scope(node);

    let is_lazy = modifiers.contains(&"lazy".to_string());

    let mut signature = "val".to_string();
    let sig_modifiers: Vec<&String> = modifiers
        .iter()
        .filter(|m| !matches!(m.as_str(), "private" | "protected"))
        .collect();
    if !sig_modifiers.is_empty() {
        signature = format!(
            "{} {}",
            sig_modifiers
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            signature
        );
    }

    signature.push_str(&format!(" {}", name));

    if let Some(ref rt) = return_type {
        signature.push_str(&format!(": {}", rt));
    }

    let visibility = helpers::determine_visibility(&modifiers);
    let doc_comment = base.find_doc_comment(node);

    let mut metadata = HashMap::from([
        (
            "type".to_string(),
            Value::String(
                if symbol_kind == SymbolKind::Property {
                    "property"
                } else {
                    "val"
                }
                .to_string(),
            ),
        ),
        ("modifiers".to_string(), Value::String(modifiers.join(","))),
    ]);
    metadata.insert("binding".to_string(), Value::String("val".to_string()));
    if is_lazy {
        metadata.insert("lazy".to_string(), Value::Bool(true));
    }
    if let Some(ref rt) = return_type {
        metadata.insert("propertyType".to_string(), Value::String(rt.clone()));
    }

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
            annotations,
        },
    ))
}

/// Extract a Scala var (mutable variable)
pub(super) fn extract_var(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name = helpers::get_name(base, node)?;
    let modifiers = helpers::extract_modifiers(base, node);
    let return_type = helpers::extract_return_type(base, node);

    let mut signature = "var".to_string();
    let sig_modifiers: Vec<&String> = modifiers
        .iter()
        .filter(|m| !matches!(m.as_str(), "private" | "protected"))
        .collect();
    if !sig_modifiers.is_empty() {
        signature = format!(
            "{} {}",
            sig_modifiers
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            signature
        );
    }

    signature.push_str(&format!(" {}", name));

    if let Some(ref rt) = return_type {
        signature.push_str(&format!(": {}", rt));
    }

    let visibility = helpers::determine_visibility(&modifiers);
    let doc_comment = base.find_doc_comment(node);

    let mut metadata = HashMap::from([
        ("type".to_string(), Value::String("var".to_string())),
        ("modifiers".to_string(), Value::String(modifiers.join(","))),
    ]);
    if let Some(ref rt) = return_type {
        metadata.insert("propertyType".to_string(), Value::String(rt.clone()));
    }

    Some(base.create_symbol(
        node,
        name,
        SymbolKind::Variable,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment,
            annotations: Vec::new(),
        },
    ))
}

/// Extract case class constructor parameters as property symbols.
pub(super) fn extract_case_class_constructor_fields(
    base: &mut BaseExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) {
    let Some(parent_id) = parent_id else {
        return;
    };

    let class_modifiers = helpers::extract_modifiers(base, node);
    if !class_modifiers.iter().any(|modifier| modifier == "case") {
        return;
    }

    let class_parameters = node
        .children(&mut node.walk())
        .find(|child| child.kind() == "class_parameters");
    let Some(class_parameters) = class_parameters else {
        return;
    };

    for parameter in class_parameters.children(&mut class_parameters.walk()) {
        if parameter.kind() != "class_parameter" {
            continue;
        }

        let name = parameter
            .child_by_field_name("name")
            .map(|name_node| base.get_node_text(&name_node))
            .or_else(|| {
                parameter
                    .children(&mut parameter.walk())
                    .find(|child| child.kind() == "identifier")
                    .map(|name_node| base.get_node_text(&name_node))
            });
        let Some(name) = name else {
            continue;
        };

        if symbols.iter().any(|symbol| {
            symbol.name == name
                && symbol.kind == SymbolKind::Property
                && symbol.parent_id.as_deref() == Some(parent_id)
        }) {
            continue;
        }

        let param_modifiers = helpers::extract_modifiers(base, &parameter);
        let annotations = helpers::extract_annotations(base, &parameter);

        let binding = parameter
            .children(&mut parameter.walk())
            .find(|child| matches!(child.kind(), "val" | "var"))
            .map(|binding_node| base.get_node_text(&binding_node))
            .unwrap_or_else(|| "val".to_string());

        let property_type = parameter
            .child_by_field_name("type")
            .map(|type_node| base.get_node_text(&type_node))
            .or_else(|| {
                let mut found_colon = false;
                for child in parameter.children(&mut parameter.walk()) {
                    let text = base.get_node_text(&child);
                    if text == ":" {
                        found_colon = true;
                        continue;
                    }
                    if text == "=" {
                        break;
                    }
                    if found_colon && child.is_named() {
                        return Some(text);
                    }
                }
                None
            });

        let mut signature = format!("{} {}", binding, name);
        if let Some(ref property_type) = property_type {
            signature.push_str(&format!(": {}", property_type));
        }

        let visibility = helpers::determine_visibility(&param_modifiers);
        let doc_comment = base.find_doc_comment(&parameter);

        let mut metadata = HashMap::from([
            ("type".to_string(), Value::String("property".to_string())),
            ("binding".to_string(), Value::String(binding)),
            (
                "modifiers".to_string(),
                Value::String(param_modifiers.join(",")),
            ),
        ]);
        if let Some(ref property_type) = property_type {
            metadata.insert(
                "propertyType".to_string(),
                Value::String(property_type.clone()),
            );
        }

        symbols.push(base.create_symbol(
            &parameter,
            name,
            SymbolKind::Property,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(visibility),
                parent_id: Some(parent_id.to_string()),
                metadata: Some(metadata),
                doc_comment,
                annotations,
            },
        ));
    }
}
