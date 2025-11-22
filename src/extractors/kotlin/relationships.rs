//! Relationship extraction for Kotlin (inheritance, implementation, calls)
//!
//! This module handles extraction of inheritance, interface implementation,
//! and method/function call relationships.

use crate::extractors::base::{
    BaseExtractor, PendingRelationship, Relationship, RelationshipKind, Symbol, SymbolKind,
};
use crate::extractors::kotlin::KotlinExtractor;
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

/// Extract function/method call relationships
///
/// Creates resolved Relationship when target is a local function.
/// Creates PendingRelationship when target is:
/// - Not found in local symbol_map (e.g., method on imported type)
pub(super) fn extract_call_relationships(
    extractor: &mut KotlinExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Build a map of symbols by name for quick lookup
    let symbol_map: HashMap<String, &Symbol> =
        symbols.iter().map(|s| (s.name.clone(), s)).collect();

    // Find call expression nodes in this subtree
    walk_tree_for_calls(extractor, node, &symbol_map, symbols, relationships);
}

fn walk_tree_for_calls(
    extractor: &mut KotlinExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    all_symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    if node.kind() == "call_expression" {
        extract_function_call_relationship(
            extractor,
            node,
            symbol_map,
            all_symbols,
            relationships,
        );
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_calls(extractor, child, symbol_map, all_symbols, relationships);
    }
}

fn extract_function_call_relationship(
    extractor: &mut KotlinExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    all_symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Extract the function name being called
    // In a call_expression, the function name is typically the first identifier
    let function_name = {
        let base = extractor.base();
        let mut result = None;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" || child.kind() == "simple_identifier" {
                result = Some(base.get_node_text(&child));
                break;
            }
            // Handle navigation expressions (obj.method) - get the last identifier
            if child.kind() == "navigation_expression" {
                let mut last_id = None;
                let mut nav_cursor = child.walk();
                for nav_child in child.children(&mut nav_cursor) {
                    if nav_child.kind() == "identifier" || nav_child.kind() == "simple_identifier" {
                        last_id = Some(base.get_node_text(&nav_child));
                    }
                }
                if last_id.is_some() {
                    result = last_id;
                    break;
                }
            }
        }
        result
    };

    let Some(function_name) = function_name else {
        return;
    };

    // Find the calling function context
    let calling_function = find_containing_function(extractor, node, all_symbols);
    let caller_symbol = calling_function
        .as_ref()
        .and_then(|name| symbol_map.get(name));

    // No caller context means we can't create a meaningful relationship
    let Some(caller) = caller_symbol else {
        return;
    };

    let line_number = node.start_position().row as u32 + 1;
    let file_path = extractor.base().file_path.clone();

    // Check if we can resolve the callee locally
    match symbol_map.get(function_name.as_str()) {
        Some(called_symbol) if called_symbol.kind == SymbolKind::Import => {
            // Target is an Import symbol - need cross-file resolution
            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: caller.id.clone(),
                callee_name: function_name,
                kind: RelationshipKind::Calls,
                file_path,
                line_number,
                confidence: 0.8,
            });
        }
        Some(called_symbol) => {
            // Target is a local function - create resolved Relationship
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    caller.id,
                    called_symbol.id,
                    RelationshipKind::Calls,
                    node.start_position().row
                ),
                from_symbol_id: caller.id.clone(),
                to_symbol_id: called_symbol.id.clone(),
                kind: RelationshipKind::Calls,
                file_path,
                line_number,
                confidence: 0.9,
                metadata: None,
            });
        }
        None => {
            // Target not found in local symbols - likely a method on imported type
            // Create PendingRelationship for cross-file resolution
            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: caller.id.clone(),
                callee_name: function_name,
                kind: RelationshipKind::Calls,
                file_path,
                line_number,
                confidence: 0.7,
            });
        }
    }
}

/// Find the function that contains this node
fn find_containing_function(
    extractor: &KotlinExtractor,
    node: Node,
    symbols: &[Symbol],
) -> Option<String> {
    let base = extractor.base();
    let file_path = &base.file_path;

    // Walk up the tree to find a function_declaration node
    let mut current = Some(node);
    while let Some(n) = current {
        if n.kind() == "function_declaration" {
            // Extract function name from the declaration
            let function_name = {
                let mut found_name = None;
                let mut cursor = n.walk();
                for child in n.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        found_name = Some(base.get_node_text(&child));
                        break;
                    }
                }
                found_name
            };

            if let Some(name) = function_name {
                // Verify this symbol exists in our symbol list
                if symbols
                    .iter()
                    .any(|s| s.name == name && &s.file_path == file_path)
                {
                    return Some(name);
                }
            }
            break;
        }
        current = n.parent();
    }

    None
}
