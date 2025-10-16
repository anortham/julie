//! Interface, type alias, enum, property, and namespace extraction
//!
//! This module handles extraction of TypeScript-specific constructs including
//! interfaces, type aliases, enums, properties, and namespaces.

use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions};
use crate::extractors::typescript::TypeScriptExtractor;
use tree_sitter::Node;

/// Extract an interface declaration
pub(super) fn extract_interface(extractor: &mut TypeScriptExtractor, node: Node) -> Symbol {
    let name_node = node.child_by_field_name("name");
    let name = if let Some(name_node) = name_node {
        extractor.base().get_node_text(&name_node)
    } else {
        "Anonymous".to_string()
    };

    extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Interface,
        SymbolOptions::default(),
    )
}

/// Extract a type alias declaration
pub(super) fn extract_type_alias(extractor: &mut TypeScriptExtractor, node: Node) -> Symbol {
    let name_node = node.child_by_field_name("name");
    let name = if let Some(name_node) = name_node {
        extractor.base().get_node_text(&name_node)
    } else {
        "Anonymous".to_string()
    };

    extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Type,
        SymbolOptions::default(),
    )
}

/// Extract an enum declaration
pub(super) fn extract_enum(extractor: &mut TypeScriptExtractor, node: Node) -> Symbol {
    let name_node = node.child_by_field_name("name");
    let name = if let Some(name_node) = name_node {
        extractor.base().get_node_text(&name_node)
    } else {
        "Anonymous".to_string()
    };

    extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Enum,
        SymbolOptions::default(),
    )
}

/// Extract a namespace declaration
pub(super) fn extract_namespace(extractor: &mut TypeScriptExtractor, node: Node) -> Symbol {
    let name_node = node.child_by_field_name("name");
    let name = if let Some(name_node) = name_node {
        extractor.base().get_node_text(&name_node)
    } else {
        "Anonymous".to_string()
    };

    extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Namespace,
        SymbolOptions::default(),
    )
}

/// Extract a property (class property or interface property)
pub(super) fn extract_property(extractor: &mut TypeScriptExtractor, node: Node) -> Symbol {
    let name_node = node
        .child_by_field_name("name")
        .or_else(|| node.child_by_field_name("key"));
    let name = if let Some(name_node) = name_node {
        extractor.base().get_node_text(&name_node)
    } else {
        "Anonymous".to_string()
    };

    extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Property,
        SymbolOptions::default(),
    )
}

