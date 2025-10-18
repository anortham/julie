use super::helpers::{
    extract_derived_traits, extract_visibility, find_doc_comment, get_preceding_attributes,
};
/// Rust type definitions extraction
/// - Structs and enums
/// - Traits
/// - Unions
/// - Modules
/// - Constants and statics
/// - Macros and type aliases
use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use crate::extractors::rust::RustExtractor;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract struct definition
pub(super) fn extract_struct(
    extractor: &mut RustExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Symbol {
    let base = extractor.get_base_mut();
    let name_node = node.child_by_field_name("name");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "Anonymous".to_string());

    // Extract visibility and attributes
    let visibility = extract_visibility(base, node);
    let attributes = get_preceding_attributes(base, node);
    let derived_traits = extract_derived_traits(base, &attributes);

    // Extract generic type parameters
    let type_params = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "type_parameters")
        .map(|c| base.get_node_text(&c))
        .unwrap_or_default();

    // Build signature
    let mut signature = format!("{}struct {}{}", visibility, name, type_params);
    if !derived_traits.is_empty() {
        signature = format!("#[derive({})] {}", derived_traits.join(", "), signature);
    }

    let visibility_enum = if visibility.trim().is_empty() {
        Visibility::Private
    } else {
        Visibility::Public
    };

    base.create_symbol(
        &node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility_enum),
            parent_id,
            doc_comment: find_doc_comment(base, node),
            metadata: Some(HashMap::new()),
        },
    )
}

/// Extract enum definition
pub(super) fn extract_enum(
    extractor: &mut RustExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Symbol {
    let base = extractor.get_base_mut();
    let name_node = node.child_by_field_name("name");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "Anonymous".to_string());

    let visibility = extract_visibility(base, node);
    let attributes = get_preceding_attributes(base, node);
    let derived_traits = extract_derived_traits(base, &attributes);

    // Extract generic type parameters
    let type_params = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "type_parameters")
        .map(|c| base.get_node_text(&c))
        .unwrap_or_default();

    let mut signature = format!("{}enum {}{}", visibility, name, type_params);
    if !derived_traits.is_empty() {
        signature = format!("#[derive({})] {}", derived_traits.join(", "), signature);
    }

    let visibility_enum = if visibility.trim().is_empty() {
        Visibility::Private
    } else {
        Visibility::Public
    };

    base.create_symbol(
        &node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility_enum),
            parent_id,
            doc_comment: find_doc_comment(base, node),
            metadata: Some(HashMap::new()),
        },
    )
}

/// Extract trait definition
pub(super) fn extract_trait(
    extractor: &mut RustExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Symbol {
    let base = extractor.get_base_mut();
    let name_node = node.child_by_field_name("name");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "Anonymous".to_string());

    let visibility = extract_visibility(base, node);

    // Extract generic type parameters
    let type_params = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "type_parameters")
        .map(|c| base.get_node_text(&c))
        .unwrap_or_default();

    // Extract trait bounds
    let trait_bounds = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "trait_bounds")
        .map(|c| base.get_node_text(&c))
        .unwrap_or_default();

    // Extract associated types from declaration_list
    let mut associated_types = Vec::new();
    if let Some(declaration_list) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "declaration_list")
    {
        for child in declaration_list.children(&mut declaration_list.walk()) {
            if child.kind() == "associated_type" {
                let assoc_type = base.get_node_text(&child).replace(";", "");
                associated_types.push(assoc_type);
            }
        }
    }

    // Build signature
    let mut signature = format!(
        "{}trait {}{}{}",
        visibility, name, type_params, trait_bounds
    );
    if !associated_types.is_empty() {
        signature = format!("{} {{ {} }}", signature, associated_types.join("; "));
    }

    let visibility_enum = if visibility.trim().is_empty() {
        Visibility::Private
    } else {
        Visibility::Public
    };

    base.create_symbol(
        &node,
        name,
        SymbolKind::Interface,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility_enum),
            parent_id,
            doc_comment: find_doc_comment(base, node),
            metadata: Some(HashMap::new()),
        },
    )
}

/// Extract union definition
pub(super) fn extract_union(
    extractor: &mut RustExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Symbol {
    let base = extractor.get_base_mut();
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "type_identifier");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "Anonymous".to_string());

    let visibility = extract_visibility(base, node);
    let signature = format!("{}union {}", visibility, name);

    let visibility_enum = if visibility.trim().is_empty() {
        Visibility::Private
    } else {
        Visibility::Public
    };

    base.create_symbol(
        &node,
        name,
        SymbolKind::Union,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility_enum),
            parent_id,
            doc_comment: find_doc_comment(base, node),
            metadata: Some(HashMap::new()),
        },
    )
}

/// Extract module definition
pub(super) fn extract_module(
    extractor: &mut RustExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Symbol {
    let base = extractor.get_base_mut();
    let name_node = node.child_by_field_name("name");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "anonymous".to_string());

    let visibility = extract_visibility(base, node);
    let signature = format!("{}mod {}", visibility, name);

    let visibility_enum = if visibility.trim().is_empty() {
        Visibility::Private
    } else {
        Visibility::Public
    };

    base.create_symbol(
        &node,
        name,
        SymbolKind::Namespace,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility_enum),
            parent_id,
            doc_comment: find_doc_comment(base, node),
            metadata: Some(HashMap::new()),
        },
    )
}

/// Extract const definition
pub(super) fn extract_const(
    extractor: &mut RustExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Symbol {
    let base = extractor.get_base_mut();
    let name_node = node.child_by_field_name("name");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "anonymous".to_string());

    let visibility = extract_visibility(base, node);
    let type_node = node.child_by_field_name("type");
    let value_node = node.child_by_field_name("value");

    let mut signature = format!("{}const {}", visibility, name);
    if let Some(type_node) = type_node {
        signature.push_str(&format!(": {}", base.get_node_text(&type_node)));
    }
    if let Some(value_node) = value_node {
        signature.push_str(&format!(" = {}", base.get_node_text(&value_node)));
    }

    let visibility_enum = if visibility.trim().is_empty() {
        Visibility::Private
    } else {
        Visibility::Public
    };

    base.create_symbol(
        &node,
        name,
        SymbolKind::Constant,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility_enum),
            parent_id,
            doc_comment: find_doc_comment(base, node),
            metadata: Some(HashMap::new()),
        },
    )
}

/// Extract static definition
pub(super) fn extract_static(
    extractor: &mut RustExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Symbol {
    let base = extractor.get_base_mut();
    let name_node = node.child_by_field_name("name");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "anonymous".to_string());

    let visibility = extract_visibility(base, node);
    let is_mutable = node
        .children(&mut node.walk())
        .any(|c| c.kind() == "mutable_specifier");
    let type_node = node.child_by_field_name("type");
    let value_node = node.child_by_field_name("value");

    let mut signature = format!("{}static ", visibility);
    if is_mutable {
        signature.push_str("mut ");
    }
    signature.push_str(&name);
    if let Some(type_node) = type_node {
        signature.push_str(&format!(": {}", base.get_node_text(&type_node)));
    }
    if let Some(value_node) = value_node {
        signature.push_str(&format!(" = {}", base.get_node_text(&value_node)));
    }

    let visibility_enum = if visibility.trim().is_empty() {
        Visibility::Private
    } else {
        Visibility::Public
    };

    base.create_symbol(
        &node,
        name,
        SymbolKind::Variable,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility_enum),
            parent_id,
            doc_comment: find_doc_comment(base, node),
            metadata: Some(HashMap::new()),
        },
    )
}

/// Extract macro definition
pub(super) fn extract_macro(
    extractor: &mut RustExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Symbol {
    let base = extractor.get_base_mut();
    let name_node = node.child_by_field_name("name");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "anonymous".to_string());

    let signature = format!("macro_rules! {}", name);

    base.create_symbol(
        &node,
        name,
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            doc_comment: find_doc_comment(base, node),
            metadata: Some(HashMap::new()),
        },
    )
}

/// Extract type alias definition
pub(super) fn extract_type_alias(
    extractor: &mut RustExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Symbol {
    let base = extractor.get_base_mut();
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "type_identifier");
    let name = name_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "anonymous".to_string());

    let visibility = extract_visibility(base, node);

    // Extract generic type parameters
    let type_params = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "type_parameters")
        .map(|c| base.get_node_text(&c))
        .unwrap_or_default();

    // Extract the type definition (after =)
    let children: Vec<_> = node.children(&mut node.walk()).collect();
    let equals_index = children.iter().position(|c| c.kind() == "=");
    let type_def = if let Some(index) = equals_index {
        if index + 1 < children.len() {
            format!(" = {}", base.get_node_text(&children[index + 1]))
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let signature = format!("{}type {}{}{}", visibility, name, type_params, type_def);

    let visibility_enum = if visibility.trim().is_empty() {
        Visibility::Private
    } else {
        Visibility::Public
    };

    base.create_symbol(
        &node,
        name,
        SymbolKind::Type,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility_enum),
            parent_id,
            doc_comment: find_doc_comment(base, node),
            metadata: Some(HashMap::new()),
        },
    )
}
