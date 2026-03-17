//! Relationship extraction for Scala (inheritance, calls)
//!
//! Handles extends/implements relationships and function call relationships.

use crate::base::{
    BaseExtractor, PendingRelationship, Relationship, RelationshipKind, Symbol, SymbolKind,
};
use crate::scala::ScalaExtractor;
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract inheritance relationships from extends clauses
pub(super) fn extract_inheritance_relationships(
    extractor: &mut ScalaExtractor,
    node: &Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.base();
    let class_symbol = find_type_symbol(base, node, symbols);
    let Some(class_symbol) = class_symbol else {
        return;
    };

    // Collect base type names
    let base_types = collect_extends_types(extractor.base(), node);
    let file_path = extractor.base().file_path.clone();
    let line_number = (node.start_position().row + 1) as u32;

    for base_type_name in base_types {
        let base_type_symbol = symbols.iter().find(|s| {
            s.name == base_type_name
                && matches!(
                    s.kind,
                    SymbolKind::Class | SymbolKind::Trait | SymbolKind::Interface
                )
        });

        if let Some(base_type_symbol) = base_type_symbol {
            let relationship_kind = if base_type_symbol.kind == SymbolKind::Trait {
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
                file_path: file_path.clone(),
                line_number,
                confidence: 1.0,
                metadata: Some(HashMap::from([(
                    "baseType".to_string(),
                    Value::String(base_type_name),
                )])),
            });
        } else {
            // Pending relationship for cross-file resolution
            let pending_kind = if class_symbol.kind == SymbolKind::Trait {
                RelationshipKind::Extends
            } else {
                RelationshipKind::Implements
            };

            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: class_symbol.id.clone(),
                callee_name: base_type_name,
                kind: pending_kind,
                file_path: file_path.clone(),
                line_number,
                confidence: 0.9,
            });
        }
    }
}

/// Collect type names from extends clause
fn collect_extends_types(base: &BaseExtractor, node: &Node) -> Vec<String> {
    let mut types = Vec::new();

    let extends_clause = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "extends_clause");

    if let Some(ec) = extends_clause {
        for child in ec.children(&mut ec.walk()) {
            if child.kind() == "type_identifier" {
                types.push(base.get_node_text(&child));
            } else if child.kind() == "generic_type" || child.kind() == "stable_type_identifier" {
                // Extract the base type name from generic types like List[String]
                if let Some(name_node) = child
                    .children(&mut child.walk())
                    .find(|n| n.kind() == "type_identifier")
                {
                    types.push(base.get_node_text(&name_node));
                } else {
                    types.push(base.get_node_text(&child));
                }
            }
        }
    }

    types
}

/// Find the symbol for a type definition node
fn find_type_symbol<'a>(
    base: &BaseExtractor,
    node: &Node,
    symbols: &'a [Symbol],
) -> Option<&'a Symbol> {
    let name = node
        .child_by_field_name("name")
        .or_else(|| {
            node.children(&mut node.walk())
                .find(|n| n.kind() == "identifier")
        })
        .map(|n| base.get_node_text(&n))?;

    symbols.iter().find(|s| {
        s.name == name
            && matches!(
                s.kind,
                SymbolKind::Class
                    | SymbolKind::Trait
                    | SymbolKind::Interface
                    | SymbolKind::Enum
            )
            && s.file_path == base.file_path
    })
}

/// Extract function/method call relationships
pub(super) fn extract_call_relationships(
    extractor: &mut ScalaExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let symbol_map: HashMap<String, &Symbol> =
        symbols.iter().map(|s| (s.name.clone(), s)).collect();

    walk_tree_for_calls(extractor, node, &symbol_map, symbols, relationships);
}

fn walk_tree_for_calls(
    extractor: &mut ScalaExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    all_symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    if node.kind() == "call_expression" {
        extract_single_call(extractor, node, symbol_map, all_symbols, relationships);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_calls(extractor, child, symbol_map, all_symbols, relationships);
    }
}

fn extract_single_call(
    extractor: &mut ScalaExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    all_symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let function_name = {
        let base = extractor.base();
        let mut result = None;
        for child in node.children(&mut node.walk()) {
            if child.kind() == "identifier" {
                result = Some(base.get_node_text(&child));
                break;
            }
            if child.kind() == "field_expression" {
                // Get the rightmost identifier (the method name)
                let mut last_id = None;
                for fc in child.children(&mut child.walk()) {
                    if fc.kind() == "identifier" {
                        last_id = Some(base.get_node_text(&fc));
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

    let calling_function = find_containing_function(extractor, node, all_symbols);
    let caller_symbol = calling_function
        .as_ref()
        .and_then(|name| symbol_map.get(name));

    let Some(caller) = caller_symbol else {
        return;
    };

    let line_number = node.start_position().row as u32 + 1;
    let file_path = extractor.base().file_path.clone();

    match symbol_map.get(function_name.as_str()) {
        Some(called_symbol) if called_symbol.kind == SymbolKind::Import => {
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
    extractor: &ScalaExtractor,
    node: Node,
    symbols: &[Symbol],
) -> Option<String> {
    let base = extractor.base();
    let file_path = &base.file_path;

    let mut current = Some(node);
    while let Some(n) = current {
        if matches!(n.kind(), "function_definition" | "function_declaration") {
            let function_name = n
                .child_by_field_name("name")
                .or_else(|| {
                    n.children(&mut n.walk())
                        .find(|c| c.kind() == "identifier")
                })
                .map(|c| base.get_node_text(&c));

            if let Some(name) = function_name {
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
