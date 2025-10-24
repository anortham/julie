// Dart Extractor - Members Extraction
//
// Methods for extracting fields, properties, getters, and setters

use super::helpers::*;
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract field definition
pub(super) fn extract_field(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "declaration" {
        return None;
    }

    // Find the type and identifier
    let type_node = find_child_by_type(node, "type_identifier")?;
    let identifier_list_node = find_child_by_type(node, "initialized_identifier_list")?;

    // Get the first initialized_identifier (fields can have multiple like "String a, b, c;")
    let identifier_node = find_child_by_type(&identifier_list_node, "initialized_identifier")?;

    // Get just the identifier part (not the assignment)
    let name_node = find_child_by_type(&identifier_node, "identifier")?;

    let field_name = get_node_text(&name_node);
    let field_type = get_node_text(&type_node);
    let is_private = field_name.starts_with('_');

    // Check for modifiers using child nodes
    let is_late = find_child_by_type(node, "late").is_some();
    let is_final = find_child_by_type(node, "final").is_some()
        || find_child_by_type(node, "final_builtin").is_some();
    let is_static = find_child_by_type(node, "static").is_some();

    // Check for nullable type
    let nullable_node = find_child_by_type(node, "nullable_type");
    let is_nullable = nullable_node.is_some();

    // Build signature with modifiers
    let mut modifiers = Vec::new();
    if is_static {
        modifiers.push("static");
    }
    if is_final {
        modifiers.push("final");
    }
    if is_late {
        modifiers.push("late");
    }

    let modifier_prefix = if modifiers.is_empty() {
        String::new()
    } else {
        format!("{} ", modifiers.join(" "))
    };
    let nullable_suffix = if is_nullable { "?" } else { "" };
    let field_signature = format!(
        "{}{}{} {}",
        modifier_prefix, field_type, nullable_suffix, field_name
    );

    let mut symbol = base.create_symbol(
        node,
        field_name,
        SymbolKind::Field,
        SymbolOptions {
            signature: Some(field_signature),
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

    // Add field annotations
    let mut annotations = Vec::new();
    if is_late {
        annotations.push("Late");
    }
    if is_final {
        annotations.push("Final");
    }
    if is_static {
        annotations.push("Static");
    }

    if !annotations.is_empty() {
        let doc = symbol.doc_comment.unwrap_or_default();
        symbol.doc_comment = Some(
            format!("{} [{}]", doc, annotations.join(", "))
                .trim()
                .to_string(),
        );
    }

    Some(symbol)
}

/// Extract getter property
pub(super) fn extract_getter(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = find_child_by_type(node, "identifier")?;
    let name = get_node_text(&name_node);
    let is_private = name.starts_with('_');

    let mut symbol = base.create_symbol(
        node,
        name.clone(),
        SymbolKind::Property,
        SymbolOptions {
            signature: Some(format!("get {}", name)),
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

    // Add getter annotation
    let doc = symbol.doc_comment.unwrap_or_default();
    symbol.doc_comment = Some(format!("{} [Getter]", doc).trim().to_string());

    Some(symbol)
}

/// Extract setter property
pub(super) fn extract_setter(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = find_child_by_type(node, "identifier")?;
    let name = get_node_text(&name_node);
    let is_private = name.starts_with('_');

    let mut symbol = base.create_symbol(
        node,
        name.clone(),
        SymbolKind::Property,
        SymbolOptions {
            signature: Some(format!("set {}", name)),
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

    // Add setter annotation
    let doc = symbol.doc_comment.unwrap_or_default();
    symbol.doc_comment = Some(format!("{} [Setter]", doc).trim().to_string());

    Some(symbol)
}
