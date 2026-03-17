//! Property extraction for Scala (val/var)
//!
//! Handles immutable vals and mutable vars.

use super::helpers;
use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract a Scala val (immutable value)
pub(super) fn extract_val(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name = helpers::get_name(base, node)?;
    let modifiers = helpers::extract_modifiers(base, node);
    let return_type = helpers::extract_return_type(base, node);

    let is_lazy = modifiers.contains(&"lazy".to_string());

    let mut signature = "val".to_string();
    let sig_modifiers: Vec<&String> = modifiers
        .iter()
        .filter(|m| !matches!(m.as_str(), "private" | "protected"))
        .collect();
    if !sig_modifiers.is_empty() {
        signature = format!(
            "{} {}",
            sig_modifiers.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" "),
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
        ("type".to_string(), Value::String("val".to_string())),
        ("modifiers".to_string(), Value::String(modifiers.join(","))),
    ]);
    if is_lazy {
        metadata.insert("lazy".to_string(), Value::Bool(true));
    }
    if let Some(ref rt) = return_type {
        metadata.insert("propertyType".to_string(), Value::String(rt.clone()));
    }

    Some(base.create_symbol(
        node,
        name,
        SymbolKind::Constant,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment,
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
            sig_modifiers.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" "),
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
        },
    ))
}
