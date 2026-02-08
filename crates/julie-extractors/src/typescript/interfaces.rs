//! Interface, type alias, enum, property, and namespace extraction
//!
//! This module handles extraction of TypeScript-specific constructs including
//! interfaces, type aliases, enums, properties, and namespaces.

use crate::base::{Symbol, SymbolKind, SymbolOptions};
use crate::typescript::TypeScriptExtractor;
use tree_sitter::Node;

/// Extract an interface declaration and its members (properties and methods)
pub(super) fn extract_interface(
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

    let iface_symbol = extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Interface,
        SymbolOptions {
            doc_comment,
            ..Default::default()
        },
    );

    let parent_id = iface_symbol.id.clone();
    symbols.push(iface_symbol);

    // Extract interface members from the interface body
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            match child.kind() {
                "property_signature" => {
                    if let Some(member_name_node) = child.child_by_field_name("name") {
                        let member_name = extractor.base().get_node_text(&member_name_node);
                        if !member_name.is_empty() {
                            let signature = extractor.base().get_node_text(&child);
                            let member_symbol = extractor.base_mut().create_symbol(
                                &child,
                                member_name,
                                SymbolKind::Property,
                                SymbolOptions {
                                    parent_id: Some(parent_id.clone()),
                                    signature: Some(signature),
                                    ..Default::default()
                                },
                            );
                            symbols.push(member_symbol);
                        }
                    }
                }
                "method_signature" => {
                    if let Some(member_name_node) = child.child_by_field_name("name") {
                        let member_name = extractor.base().get_node_text(&member_name_node);
                        if !member_name.is_empty() {
                            let signature = extractor.base().get_node_text(&child);
                            let member_symbol = extractor.base_mut().create_symbol(
                                &child,
                                member_name,
                                SymbolKind::Method,
                                SymbolOptions {
                                    parent_id: Some(parent_id.clone()),
                                    signature: Some(signature),
                                    ..Default::default()
                                },
                            );
                            symbols.push(member_symbol);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    symbols
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
    use super::helpers;

    let name_node = node
        .child_by_field_name("name")
        .or_else(|| node.child_by_field_name("key"));
    let name = name_node.map(|n| extractor.base().get_node_text(&n))?;

    // Extract visibility from accessibility_modifier (private/protected/public)
    let visibility = helpers::extract_ts_visibility(node);

    // Extract decorators
    let content = extractor.base().content.clone();
    let decorators = helpers::extract_decorator_names(node, &content);

    // Check for readonly
    let is_readonly = helpers::has_readonly(node);

    // Build signature with decorators, access modifier, readonly, and type annotation
    let mut sig_parts = Vec::new();
    let decorator_prefix = helpers::decorator_prefix(&decorators);
    if !decorator_prefix.is_empty() {
        sig_parts.push(decorator_prefix.trim().to_string());
    }
    if is_readonly {
        sig_parts.push("readonly".to_string());
    }
    sig_parts.push(name.clone());
    // Append type annotation if present
    if let Some(type_ann) = extractor.base().get_field_text(&node, "type") {
        sig_parts.push(format!(": {}", type_ann));
    }
    let signature = if sig_parts.len() > 1 || !decorators.is_empty() || is_readonly {
        Some(sig_parts.join(" "))
    } else {
        None
    };

    // Extract JSDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    // Find parent class
    let parent_id = find_parent_class_or_interface_id(extractor, &node);

    Some(extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Property,
        SymbolOptions {
            signature,
            visibility,
            parent_id,
            doc_comment,
            ..Default::default()
        },
    ))
}

/// Find the parent class or interface ID for a member node
fn find_parent_class_or_interface_id(
    extractor: &TypeScriptExtractor,
    node: &Node,
) -> Option<String> {
    let mut current = node.parent();
    while let Some(parent_node) = current {
        match parent_node.kind() {
            "class_declaration" | "interface_declaration" => {
                if let Some(class_name_node) = parent_node.child_by_field_name("name") {
                    let class_name = extractor.base().get_node_text(&class_name_node);
                    let class_start = parent_node.start_position();
                    let candidates = [
                        extractor.base().generate_id(
                            &class_name,
                            class_start.row as u32,
                            class_start.column as u32,
                        ),
                        extractor.base().generate_id(
                            &class_name,
                            class_name_node.start_position().row as u32,
                            class_name_node.start_position().column as u32,
                        ),
                    ];

                    for candidate in candidates {
                        if extractor.base().symbol_map.contains_key(&candidate) {
                            return Some(candidate);
                        }
                    }

                    if let Some((id, _symbol)) =
                        extractor.base().symbol_map.iter().find(|(_, symbol)| {
                            symbol.name == class_name
                                && (symbol.kind == SymbolKind::Class
                                    || symbol.kind == SymbolKind::Interface)
                        })
                    {
                        return Some(id.clone());
                    }
                }
            }
            _ => {}
        }
        current = parent_node.parent();
    }
    None
}
