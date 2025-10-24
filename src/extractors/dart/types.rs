// Dart Extractor - Types Extraction
//
// Methods for extracting type aliases, enums, mixins, and extensions

use super::helpers::*;
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract enum definition
pub(super) fn extract_enum(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = find_child_by_type(node, "identifier")?;
    let name = get_node_text(&name_node);

    let symbol = base.create_symbol(
        node,
        name.clone(),
        SymbolKind::Enum,
        SymbolOptions {
            signature: Some(format!("enum {}", name)),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|id| id.to_string()),
            metadata: Some(HashMap::new()),
            ..Default::default()
        },
    );

    Some(symbol)
}

/// Extract enum constant
pub(super) fn extract_enum_constant(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "enum_constant" {
        return None;
    }

    let name_node = find_child_by_type(node, "identifier")?;
    let constant_name = get_node_text(&name_node);

    // Check if there are arguments (enhanced enum)
    let argument_part = find_child_by_type(node, "argument_part");
    let signature = if let Some(arg_node) = argument_part {
        format!("{}{}", constant_name, get_node_text(&arg_node))
    } else {
        constant_name.clone()
    };

    let symbol = base.create_symbol(
        node,
        constant_name,
        SymbolKind::EnumMember,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|id| id.to_string()),
            metadata: Some(HashMap::new()),
            ..Default::default()
        },
    );

    Some(symbol)
}

/// Extract mixin definition
pub(super) fn extract_mixin(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = find_child_by_type(node, "identifier")?;
    let name = get_node_text(&name_node);

    // Check for "on" clause (constrained mixin)
    let on_node = find_child_by_type(node, "on");
    let type_node = find_child_by_type(node, "type_identifier");

    let signature = if let (Some(_on), Some(type_n)) = (on_node, type_node) {
        let constraint_type = get_node_text(&type_n);
        format!("mixin {} on {}", name, constraint_type)
    } else {
        format!("mixin {}", name)
    };

    let constraint_type_name = type_node.map(|n| get_node_text(&n));

    let mut symbol = base.create_symbol(
        node,
        name,
        SymbolKind::Interface,
        SymbolOptions {
            signature: Some(signature),
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
        .insert("isMixin".to_string(), serde_json::Value::Bool(true));
    if let Some(constraint_type) = constraint_type_name {
        symbol.metadata.get_or_insert_with(HashMap::new).insert(
            "constraintType".to_string(),
            serde_json::Value::String(constraint_type),
        );
    }

    Some(symbol)
}

/// Extract extension definition
pub(super) fn extract_extension(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = find_child_by_type(node, "identifier")?;
    let name = get_node_text(&name_node);

    // Check for "on" clause (type being extended)
    let on_node = find_child_by_type(node, "on");
    let type_node = find_child_by_type(node, "type_identifier");

    let signature = if let (Some(_on), Some(type_n)) = (on_node, type_node) {
        let extended_type = get_node_text(&type_n);
        format!("extension {} on {}", name, extended_type)
    } else {
        format!("extension {}", name)
    };

    let extended_type_name = type_node.map(|n| get_node_text(&n));

    let mut symbol = base.create_symbol(
        node,
        name,
        SymbolKind::Module,
        SymbolOptions {
            signature: Some(signature),
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
        .insert("isExtension".to_string(), serde_json::Value::Bool(true));
    if let Some(extended_type) = extended_type_name {
        symbol.metadata.get_or_insert_with(HashMap::new).insert(
            "extendedType".to_string(),
            serde_json::Value::String(extended_type),
        );
    }

    Some(symbol)
}

/// Extract type alias (typedef)
pub(super) fn extract_typedef(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "type_alias" {
        return None;
    }

    // Get the typedef name
    let name_node = find_child_by_type(node, "type_identifier")?;
    let name = get_node_text(&name_node);
    let is_private = name.starts_with('_');

    // Build signature with typedef keyword and generic parameters
    let type_params_node = find_child_by_type(node, "type_parameters");
    let type_params = type_params_node
        .map(|n| get_node_text(&n))
        .unwrap_or_default();

    // Get the type being aliased (everything after =)
    let mut aliased_type = String::new();
    let mut cursor = node.walk();
    let mut found_equals = false;

    for child in node.children(&mut cursor) {
        if child.kind() == "=" {
            found_equals = true;
            continue;
        }
        if found_equals && child.kind() != ";" {
            aliased_type.push_str(&get_node_text(&child));
        }
    }

    let signature = format!("typedef {}{} = {}", name, type_params, aliased_type.trim());

    let mut symbol = base.create_symbol(
        node,
        name,
        SymbolKind::Class,
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
        .insert("isTypedef".to_string(), serde_json::Value::Bool(true));
    symbol.metadata.get_or_insert_with(HashMap::new).insert(
        "aliasedType".to_string(),
        serde_json::Value::String(aliased_type.trim().to_string()),
    );

    Some(symbol)
}
