//! Interface, type alias, enum, property, and namespace extraction
//!
//! This module handles extraction of TypeScript-specific constructs including
//! interfaces, type aliases, enums, properties, and namespaces.

use crate::base::{Symbol, SymbolKind, SymbolOptions};
use crate::typescript::TypeScriptExtractor;
use tree_sitter::Node;

/// Extract an interface declaration
pub(super) fn extract_interface(
    extractor: &mut TypeScriptExtractor,
    node: Node,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name");
    let name = name_node.map(|n| extractor.base().get_node_text(&n))?;

    // Extract JSDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    Some(extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Interface,
        SymbolOptions {
            doc_comment,
            ..Default::default()
        },
    ))
}

/// Extract a type alias declaration
pub(super) fn extract_type_alias(
    extractor: &mut TypeScriptExtractor,
    node: Node,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name");
    let name = name_node.map(|n| extractor.base().get_node_text(&n))?;

    // Extract JSDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    Some(extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Type,
        SymbolOptions {
            doc_comment,
            ..Default::default()
        },
    ))
}

/// Extract an enum declaration and its members
pub(super) fn extract_enum(
    extractor: &mut TypeScriptExtractor,
    node: Node,
) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    let name_node = node.child_by_field_name("name");
    let name = match name_node.map(|n| extractor.base().get_node_text(&n)) {
        Some(name) => name,
        None => return symbols,
    };

    // Extract JSDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    let enum_symbol = extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Enum,
        SymbolOptions {
            doc_comment,
            ..Default::default()
        },
    );

    let parent_id = enum_symbol.id.clone();
    symbols.push(enum_symbol);

    // Extract enum members from the enum body
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "enum_member" || child.kind() == "property_identifier" {
                let member_name_node = child
                    .child_by_field_name("name")
                    .or_else(|| {
                        // Some grammars put the identifier directly
                        if child.kind() == "property_identifier" {
                            Some(child)
                        } else {
                            child
                                .children(&mut child.walk())
                                .find(|c| c.kind() == "property_identifier" || c.kind() == "identifier")
                        }
                    });

                if let Some(member_name_node) = member_name_node {
                    let member_name = extractor.base().get_node_text(&member_name_node);
                    if !member_name.is_empty() && member_name != "," {
                        let member_symbol = extractor.base_mut().create_symbol(
                            &child,
                            member_name,
                            SymbolKind::EnumMember,
                            SymbolOptions {
                                parent_id: Some(parent_id.clone()),
                                ..Default::default()
                            },
                        );
                        symbols.push(member_symbol);
                    }
                }
            }
        }
    }

    symbols
}

/// Extract a namespace declaration
pub(super) fn extract_namespace(
    extractor: &mut TypeScriptExtractor,
    node: Node,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name");
    let name = name_node.map(|n| extractor.base().get_node_text(&n))?;

    // Extract JSDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    Some(extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Namespace,
        SymbolOptions {
            doc_comment,
            ..Default::default()
        },
    ))
}

/// Extract a property (class property or interface property)
pub(super) fn extract_property(
    extractor: &mut TypeScriptExtractor,
    node: Node,
) -> Option<Symbol> {
    let name_node = node
        .child_by_field_name("name")
        .or_else(|| node.child_by_field_name("key"));
    let name = name_node.map(|n| extractor.base().get_node_text(&n))?;

    // Extract JSDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    Some(extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Property,
        SymbolOptions {
            doc_comment,
            ..Default::default()
        },
    ))
}
