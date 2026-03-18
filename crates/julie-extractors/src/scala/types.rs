//! Type extraction for Scala
//!
//! Handles classes, traits, objects, enums, and enum cases.

use super::helpers;
use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract a Scala class definition (class, case class, abstract class)
pub(super) fn extract_class(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name = helpers::get_name(base, node)?;
    let modifiers = helpers::extract_modifiers(base, node);
    let type_params = helpers::extract_type_parameters(base, node);
    let params = helpers::extract_parameters(base, node);
    let extends = helpers::extract_extends(base, node);

    let mut sig_parts = Vec::new();

    // Add modifiers to signature (exclude visibility)
    let sig_modifiers: Vec<&String> = modifiers
        .iter()
        .filter(|m| !matches!(m.as_str(), "private" | "protected"))
        .collect();
    if !sig_modifiers.is_empty() {
        sig_parts.push(
            sig_modifiers
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        );
    }

    sig_parts.push(format!("class {}", name));
    let mut signature = sig_parts.join(" ");

    if let Some(tp) = type_params {
        signature.push_str(&tp);
    }
    if let Some(p) = params {
        signature.push_str(&p);
    }
    if let Some(ext) = extends {
        signature.push_str(&format!(" {}", ext));
    }

    let visibility = helpers::determine_visibility(&modifiers);
    let doc_comment = base.find_doc_comment(node);

    Some(base.create_symbol(
        node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([
                ("type".to_string(), Value::String("class".to_string())),
                ("modifiers".to_string(), Value::String(modifiers.join(","))),
            ])),
            doc_comment,
        },
    ))
}

/// Extract a Scala trait definition
pub(super) fn extract_trait(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name = helpers::get_name(base, node)?;
    let modifiers = helpers::extract_modifiers(base, node);
    let type_params = helpers::extract_type_parameters(base, node);
    let extends = helpers::extract_extends(base, node);

    let mut sig_parts = Vec::new();
    let sig_modifiers: Vec<&String> = modifiers
        .iter()
        .filter(|m| !matches!(m.as_str(), "private" | "protected"))
        .collect();
    if !sig_modifiers.is_empty() {
        sig_parts.push(
            sig_modifiers
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        );
    }

    sig_parts.push(format!("trait {}", name));
    let mut signature = sig_parts.join(" ");

    if let Some(tp) = type_params {
        signature.push_str(&tp);
    }
    if let Some(ext) = extends {
        signature.push_str(&format!(" {}", ext));
    }

    let visibility = helpers::determine_visibility(&modifiers);
    let doc_comment = base.find_doc_comment(node);

    Some(base.create_symbol(
        node,
        name,
        SymbolKind::Trait,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([
                ("type".to_string(), Value::String("trait".to_string())),
                ("modifiers".to_string(), Value::String(modifiers.join(","))),
            ])),
            doc_comment,
        },
    ))
}

/// Extract a Scala object definition
pub(super) fn extract_object(
    base: &mut BaseExtractor,
    node: &Node,
    symbols: &[Symbol],
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name = helpers::get_name(base, node)?;
    let modifiers = helpers::extract_modifiers(base, node);
    let extends = helpers::extract_extends(base, node);

    // Check if companion (matches an existing class/trait name)
    let is_companion = symbols.iter().any(|s| {
        s.name == name
            && matches!(
                s.kind,
                SymbolKind::Class | SymbolKind::Trait | SymbolKind::Enum
            )
    });

    let mut sig_parts = Vec::new();
    let sig_modifiers: Vec<&String> = modifiers
        .iter()
        .filter(|m| !matches!(m.as_str(), "private" | "protected"))
        .collect();
    if !sig_modifiers.is_empty() {
        sig_parts.push(
            sig_modifiers
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        );
    }

    sig_parts.push(format!("object {}", name));
    let mut signature = sig_parts.join(" ");

    if let Some(ext) = extends {
        signature.push_str(&format!(" {}", ext));
    }

    let visibility = helpers::determine_visibility(&modifiers);
    let doc_comment = base.find_doc_comment(node);

    let mut metadata = HashMap::from([
        ("type".to_string(), Value::String("object".to_string())),
        ("modifiers".to_string(), Value::String(modifiers.join(","))),
    ]);
    if is_companion {
        metadata.insert("companion".to_string(), Value::Bool(true));
    }

    Some(base.create_symbol(
        node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

/// Extract a Scala 3 enum definition
pub(super) fn extract_enum(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name = helpers::get_name(base, node)?;
    let modifiers = helpers::extract_modifiers(base, node);
    let type_params = helpers::extract_type_parameters(base, node);
    let extends = helpers::extract_extends(base, node);

    let mut signature = format!("enum {}", name);
    if let Some(tp) = type_params {
        signature.push_str(&tp);
    }
    if let Some(ext) = extends {
        signature.push_str(&format!(" {}", ext));
    }

    let visibility = helpers::determine_visibility(&modifiers);
    let doc_comment = base.find_doc_comment(node);

    Some(base.create_symbol(
        node,
        name,
        SymbolKind::Enum,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([
                ("type".to_string(), Value::String("enum".to_string())),
                ("modifiers".to_string(), Value::String(modifiers.join(","))),
            ])),
            doc_comment,
        },
    ))
}

/// Extract a Scala 3 enum case (simple or full)
pub(super) fn extract_enum_case(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name = helpers::get_name(base, node)?;
    let params = helpers::extract_parameters(base, node);

    let mut signature = format!("case {}", name);
    if let Some(p) = params {
        signature.push_str(&p);
    }

    let doc_comment = base.find_doc_comment(node);

    Some(base.create_symbol(
        node,
        name,
        SymbolKind::EnumMember,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(crate::base::Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([(
                "type".to_string(),
                Value::String("enum-member".to_string()),
            )])),
            doc_comment,
        },
    ))
}
