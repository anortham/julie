/// Class, interface, enum, and record extraction
use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use crate::extractors::java::JavaExtractor;
use serde_json;
use std::collections::HashMap;
use tree_sitter::Node;

use super::helpers;

/// Extract class declaration from a node
pub(super) fn extract_class(
    extractor: &mut JavaExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")?;

    let name = extractor.base().get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(extractor.base(), node);
    let visibility = helpers::determine_visibility(&modifiers);

    // Build signature
    let mut signature = if modifiers.is_empty() {
        format!("class {}", name)
    } else {
        format!("{} class {}", modifiers.join(" "), name)
    };

    // Handle generic type parameters
    if let Some(type_params) = helpers::extract_type_parameters(extractor.base(), node) {
        signature = signature.replace(
            &format!("class {}", name),
            &format!("class {}{}", name, type_params),
        );
    }

    // Check for inheritance and implementations
    if let Some(superclass) = helpers::extract_superclass(extractor.base(), node) {
        signature.push_str(&format!(" extends {}", superclass));
    }

    let interfaces = helpers::extract_implemented_interfaces(extractor.base(), node);
    if !interfaces.is_empty() {
        signature.push_str(&format!(" implements {}", interfaces.join(", ")));
    }

    // Handle sealed class permits clause (Java 17+)
    if let Some(permits_clause) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "permits")
    {
        signature.push_str(&format!(
            " {}",
            extractor.base().get_node_text(&permits_clause)
        ));
    }

    // Extract JavaDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment,
        ..Default::default()
    };

    Some(
        extractor
            .base_mut()
            .create_symbol(&node, name, SymbolKind::Class, options),
    )
}

/// Extract interface declaration from a node
pub(super) fn extract_interface(
    extractor: &mut JavaExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")?;

    let name = extractor.base().get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(extractor.base(), node);
    let visibility = helpers::determine_visibility(&modifiers);

    // Build signature
    let mut signature = if modifiers.is_empty() {
        format!("interface {}", name)
    } else {
        format!("{} interface {}", modifiers.join(" "), name)
    };

    // Check for interface inheritance (extends)
    let super_interfaces = helpers::extract_extended_interfaces(extractor.base(), node);
    if !super_interfaces.is_empty() {
        signature.push_str(&format!(" extends {}", super_interfaces.join(", ")));
    }

    // Handle generic type parameters
    if let Some(type_params) = helpers::extract_type_parameters(extractor.base(), node) {
        signature = signature.replace(
            &format!("interface {}", name),
            &format!("interface {}{}", name, type_params),
        );
    }

    // Extract JavaDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment,
        ..Default::default()
    };

    Some(
        extractor
            .base_mut()
            .create_symbol(&node, name, SymbolKind::Interface, options),
    )
}

/// Extract enum declaration from a node
pub(super) fn extract_enum(
    extractor: &mut JavaExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")?;

    let name = extractor.base().get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(extractor.base(), node);
    let visibility = helpers::determine_visibility(&modifiers);

    // Build signature
    let mut signature = if modifiers.is_empty() {
        format!("enum {}", name)
    } else {
        format!("{} enum {}", modifiers.join(" "), name)
    };

    // Check for interface implementations (enums can implement interfaces)
    let interfaces = helpers::extract_implemented_interfaces(extractor.base(), node);
    if !interfaces.is_empty() {
        signature.push_str(&format!(" implements {}", interfaces.join(", ")));
    }

    // Extract JavaDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment,
        ..Default::default()
    };

    Some(
        extractor
            .base_mut()
            .create_symbol(&node, name, SymbolKind::Enum, options),
    )
}

/// Extract enum constant from a node
pub(super) fn extract_enum_constant(
    extractor: &mut JavaExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")?;

    let name = extractor.base().get_node_text(&name_node);

    // Build signature - include arguments if present
    let mut signature = name.clone();
    let argument_list = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "argument_list");
    if let Some(args) = argument_list {
        signature.push_str(&extractor.base().get_node_text(&args));
    }

    // Extract JavaDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(Visibility::Public), // Enum constants are always public in Java
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment,
        ..Default::default()
    };

    Some(
        extractor
            .base_mut()
            .create_symbol(&node, name, SymbolKind::EnumMember, options),
    )
}

/// Extract record declaration from a node
pub(super) fn extract_record(
    extractor: &mut JavaExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")?;

    let name = extractor.base().get_node_text(&name_node);
    let modifiers = helpers::extract_modifiers(extractor.base(), node);
    let visibility = helpers::determine_visibility(&modifiers);

    // Get record parameters (record components)
    let param_list = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "formal_parameters");
    let params = param_list
        .map(|p| extractor.base().get_node_text(&p))
        .unwrap_or_else(|| "()".to_string());

    // Build signature
    let mut signature = if modifiers.is_empty() {
        format!("record {}{}", name, params)
    } else {
        format!("{} record {}{}", modifiers.join(" "), name, params)
    };

    // Handle generic type parameters
    if let Some(type_params) = helpers::extract_type_parameters(extractor.base(), node) {
        signature = signature.replace(
            &format!("record {}", name),
            &format!("record {}{}", name, type_params),
        );
    }

    // Check for interface implementations (records can implement interfaces)
    let interfaces = helpers::extract_implemented_interfaces(extractor.base(), node);
    if !interfaces.is_empty() {
        signature.push_str(&format!(" implements {}", interfaces.join(", ")));
    }

    let mut metadata = HashMap::new();
    metadata.insert(
        "type".to_string(),
        serde_json::Value::String("record".to_string()),
    );

    // Extract JavaDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(visibility),
        parent_id: parent_id.map(|s| s.to_string()),
        metadata: Some(metadata),
        doc_comment,
    };

    Some(
        extractor
            .base_mut()
            .create_symbol(&node, name, SymbolKind::Class, options),
    )
}
