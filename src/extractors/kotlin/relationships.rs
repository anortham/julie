//! Relationship extraction for Kotlin (inheritance, implementation)
//!
//! This module handles extraction of inheritance and interface implementation
//! relationships between types.

use crate::extractors::base::{BaseExtractor, Relationship, RelationshipKind, Symbol, SymbolKind};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract inheritance and implementation relationships from a Kotlin type
pub(super) fn extract_inheritance_relationships(
    base: &BaseExtractor,
    node: &Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let class_symbol = find_class_symbol(base, node, symbols);
    if class_symbol.is_none() {
        return;
    }
    let class_symbol = class_symbol.unwrap();

    // Look for delegation_specifiers container first (wrapped case)
    let delegation_container = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "delegation_specifiers");
    let mut base_type_names = Vec::new();

    // Look for delegation_specifiers to find inheritance/interface implementation
    if let Some(delegation_container) = delegation_container {
        for child in delegation_container.children(&mut delegation_container.walk()) {
            if child.kind() == "delegation_specifier" {
                let type_node = child.children(&mut child.walk()).find(|n| {
                    matches!(
                        n.kind(),
                        "type" | "user_type" | "identifier" | "constructor_invocation"
                    )
                });
                if let Some(type_node) = type_node {
                    let base_type = if type_node.kind() == "constructor_invocation" {
                        // For constructor invocations like Widget(), extract just the type name
                        let user_type_node = type_node
                            .children(&mut type_node.walk())
                            .find(|n| n.kind() == "user_type");
                        if let Some(user_type_node) = user_type_node {
                            base.get_node_text(&user_type_node)
                        } else {
                            let full_text = base.get_node_text(&type_node);
                            full_text
                                .split('(')
                                .next()
                                .unwrap_or(&full_text)
                                .to_string()
                        }
                    } else {
                        base.get_node_text(&type_node)
                    };
                    base_type_names.push(base_type);
                }
            } else if child.kind() == "delegated_super_type" {
                let type_node = child
                    .children(&mut child.walk())
                    .find(|n| matches!(n.kind(), "type" | "user_type" | "identifier"));
                if let Some(type_node) = type_node {
                    base_type_names.push(base.get_node_text(&type_node));
                }
            } else if matches!(child.kind(), "type" | "user_type" | "identifier") {
                base_type_names.push(base.get_node_text(&child));
            }
        }
    } else {
        // Look for individual delegation_specifier nodes (multiple at same level)
        let delegation_specifiers: Vec<Node> = node
            .children(&mut node.walk())
            .filter(|n| n.kind() == "delegation_specifier")
            .collect();
        for delegation in delegation_specifiers {
            let explicit_delegation = delegation
                .children(&mut delegation.walk())
                .find(|n| n.kind() == "explicit_delegation");
            if let Some(explicit_delegation) = explicit_delegation {
                let type_text = base.get_node_text(&explicit_delegation);
                let type_name = type_text.split(" by ").next().unwrap_or(&type_text);
                base_type_names.push(type_name.to_string());
            } else {
                let type_node = delegation.children(&mut delegation.walk()).find(|n| {
                    matches!(
                        n.kind(),
                        "type" | "user_type" | "identifier" | "constructor_invocation"
                    )
                });
                if let Some(type_node) = type_node {
                    if type_node.kind() == "constructor_invocation" {
                        let user_type_node = type_node
                            .children(&mut type_node.walk())
                            .find(|n| n.kind() == "user_type");
                        if let Some(user_type_node) = user_type_node {
                            base_type_names.push(base.get_node_text(&user_type_node));
                        }
                    } else {
                        base_type_names.push(base.get_node_text(&type_node));
                    }
                }
            }
        }
    }

    // Create relationships for each base type
    for base_type_name in base_type_names {
        let base_type_symbol = symbols.iter().find(|s| {
            s.name == base_type_name
                && matches!(
                    s.kind,
                    SymbolKind::Class | SymbolKind::Interface | SymbolKind::Struct
                )
        });

        if let Some(base_type_symbol) = base_type_symbol {
            let relationship_kind = if base_type_symbol.kind == SymbolKind::Interface {
                RelationshipKind::Implements
            } else {
                RelationshipKind::Extends
            };

            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    class_symbol.id,
                    base_type_symbol.id,
                    relationship_kind,
                    node.start_position().row
                ),
                from_symbol_id: class_symbol.id.clone(),
                to_symbol_id: base_type_symbol.id.clone(),
                kind: relationship_kind,
                file_path: base.file_path.clone(),
                line_number: (node.start_position().row + 1) as u32,
                confidence: 1.0,
                metadata: Some(HashMap::from([(
                    "baseType".to_string(),
                    Value::String(base_type_name),
                )])),
            });
        }
    }
}

/// Find the symbol corresponding to a class/interface/enum node
fn find_class_symbol<'a>(
    base: &BaseExtractor,
    node: &Node,
    symbols: &'a [Symbol],
) -> Option<&'a Symbol> {
    let name_node = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "identifier");
    let class_name = name_node.map(|n| base.get_node_text(&n))?;

    symbols.iter().find(|s| {
        s.name == class_name
            && matches!(s.kind, SymbolKind::Class | SymbolKind::Interface)
            && s.file_path == base.file_path
    })
}
