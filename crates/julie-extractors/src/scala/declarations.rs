//! Declaration extraction for Scala
//!
//! Functions, imports, packages, type aliases, given instances, and extensions.

use super::helpers;
use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use crate::test_detection::is_test_symbol;
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract a Scala function/method definition
pub(super) fn extract_function(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name = helpers::get_name(base, node)?;
    let modifiers = helpers::extract_modifiers(base, node);
    let type_params = helpers::extract_type_parameters(base, node);
    let parameters = helpers::extract_parameters(base, node);
    let return_type = helpers::extract_return_type(base, node);

    let mut signature = "def".to_string();

    // Add modifiers
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

    if let Some(tp) = type_params {
        signature.push_str(&tp);
    }

    signature.push_str(&parameters.unwrap_or_else(|| "()".to_string()));

    if let Some(ref rt) = return_type {
        signature.push_str(&format!(": {}", rt));
    }

    // Determine method vs function
    let symbol_kind = if parent_id.is_some() {
        SymbolKind::Method
    } else {
        SymbolKind::Function
    };

    let visibility = helpers::determine_visibility(&modifiers);

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

    if let Some(return_type) = return_type {
        metadata.insert("returnType".to_string(), Value::String(return_type));
    }

    let doc_comment = base.find_doc_comment(node);

    // Test detection
    let annotations: Vec<String> = modifiers
        .iter()
        .filter(|m| m.starts_with('@'))
        .map(|m| m.strip_prefix('@').unwrap_or(m).to_string())
        .collect();

    if is_test_symbol(
        "scala",
        &name,
        &base.file_path,
        &symbol_kind,
        &annotations,
        &[],
        doc_comment.as_deref(),
    ) {
        metadata.insert("is_test".to_string(), Value::Bool(true));
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
        },
    ))
}

/// Extract a Scala import declaration
pub(super) fn extract_import(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Get the full import text, strip the "import " prefix
    let full_text = base.get_node_text(node);
    let name = full_text
        .strip_prefix("import ")
        .unwrap_or(&full_text)
        .to_string();

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

/// Extract a Scala package clause
pub(super) fn extract_package(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Get the full text and strip "package " prefix
    let full_text = base.get_node_text(node);
    let name = full_text
        .strip_prefix("package ")
        .unwrap_or(&full_text)
        .trim()
        .to_string();

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

/// Extract a Scala type alias (type X = Y)
pub(super) fn extract_type_alias(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name = helpers::get_name(base, node)?;
    let modifiers = helpers::extract_modifiers(base, node);
    let _type_params = helpers::extract_type_parameters(base, node);

    let full_text = base.get_node_text(node);
    let signature = full_text.trim().to_string();

    let visibility = helpers::determine_visibility(&modifiers);
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
            ])),
            doc_comment,
        },
    ))
}

/// Extract a Scala 3 given definition
pub(super) fn extract_given(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name = helpers::get_name(base, node).unwrap_or_else(|| "<anonymous>".to_string());
    let full_text = base.get_node_text(node);
    let signature = full_text.lines().next().unwrap_or(&full_text).trim().to_string();

    let doc_comment = base.find_doc_comment(node);

    Some(base.create_symbol(
        node,
        name,
        SymbolKind::Variable,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([
                ("type".to_string(), Value::String("given".to_string())),
                ("given".to_string(), Value::Bool(true)),
            ])),
            doc_comment,
        },
    ))
}

/// Extract a Scala 3 extension definition
pub(super) fn extract_extension(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let full_text = base.get_node_text(node);
    let signature = full_text.lines().next().unwrap_or(&full_text).trim().to_string();

    // Try to extract name from the extension parameter type
    let name = helpers::get_name(base, node).unwrap_or_else(|| "extension".to_string());

    let doc_comment = base.find_doc_comment(node);

    Some(base.create_symbol(
        node,
        name,
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([
                ("type".to_string(), Value::String("extension".to_string())),
                ("extension".to_string(), Value::Bool(true)),
            ])),
            doc_comment,
        },
    ))
}
