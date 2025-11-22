/// Inheritance, implementation, and call relationship extraction
use crate::extractors::base::{
    PendingRelationship, Relationship, RelationshipKind, Symbol, SymbolKind,
};
use crate::extractors::java::JavaExtractor;
use serde_json;
use std::collections::HashMap;
use tree_sitter::Node;

use super::helpers;

/// Extract inheritance relationships from a class/interface/enum declaration
pub(super) fn extract_inheritance_relationships(
    extractor: &mut JavaExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let type_symbol = find_type_symbol(extractor, node, symbols);
    if type_symbol.is_none() {
        return;
    }
    let type_symbol = type_symbol.unwrap();

    // Handle class inheritance (extends)
    if let Some(superclass) = helpers::extract_superclass(extractor.base(), node) {
        if let Some(base_type_symbol) = symbols.iter().find(|s| {
            s.name == superclass && matches!(s.kind, SymbolKind::Class | SymbolKind::Interface)
        }) {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    type_symbol.id,
                    base_type_symbol.id,
                    RelationshipKind::Extends,
                    node.start_position().row
                ),
                from_symbol_id: type_symbol.id.clone(),
                to_symbol_id: base_type_symbol.id.clone(),
                kind: RelationshipKind::Extends,
                file_path: extractor.base().file_path.clone(),
                line_number: (node.start_position().row + 1) as u32,
                confidence: 1.0,
                metadata: {
                    let mut map = HashMap::new();
                    map.insert(
                        "baseType".to_string(),
                        serde_json::Value::String(superclass),
                    );
                    Some(map)
                },
            });
        }
    }

    // Handle interface implementations
    let interfaces = helpers::extract_implemented_interfaces(extractor.base(), node);
    for interface_name in interfaces {
        if let Some(interface_symbol) = symbols
            .iter()
            .find(|s| s.name == interface_name && s.kind == SymbolKind::Interface)
        {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    type_symbol.id,
                    interface_symbol.id,
                    RelationshipKind::Implements,
                    node.start_position().row
                ),
                from_symbol_id: type_symbol.id.clone(),
                to_symbol_id: interface_symbol.id.clone(),
                kind: RelationshipKind::Implements,
                file_path: extractor.base().file_path.clone(),
                line_number: (node.start_position().row + 1) as u32,
                confidence: 1.0,
                metadata: {
                    let mut map = HashMap::new();
                    map.insert(
                        "interface".to_string(),
                        serde_json::Value::String(interface_name),
                    );
                    Some(map)
                },
            });
        }
    }
}

/// Find the type symbol (class/interface/enum) that corresponds to this node
fn find_type_symbol<'a>(
    extractor: &JavaExtractor,
    node: Node,
    symbols: &'a [Symbol],
) -> Option<&'a Symbol> {
    let name_node = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")?;
    let type_name = extractor.base().get_node_text(&name_node);

    symbols.iter().find(|s| {
        s.name == type_name
            && matches!(
                s.kind,
                SymbolKind::Class | SymbolKind::Interface | SymbolKind::Enum
            )
            && s.file_path == extractor.base().file_path
    })
}

/// Extract method call relationships
///
/// Creates resolved Relationship when target is a local method.
/// Creates PendingRelationship when target is:
/// - An Import symbol (needs cross-file resolution)
/// - Not found in local symbol_map (e.g., method on imported type)
pub(super) fn extract_call_relationships(
    extractor: &mut JavaExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Build a map of symbols by name for quick lookup
    let symbol_map: HashMap<String, &Symbol> =
        symbols.iter().map(|s| (s.name.clone(), s)).collect();

    // Find method invocation nodes in this subtree
    walk_tree_for_calls(extractor, node, &symbol_map, symbols, relationships);
}

fn walk_tree_for_calls(
    extractor: &mut JavaExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    all_symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    if node.kind() == "method_invocation" {
        extract_method_call_relationship(extractor, node, symbol_map, all_symbols, relationships);
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_calls(extractor, child, symbol_map, all_symbols, relationships);
    }
}

fn extract_method_call_relationship(
    extractor: &mut JavaExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    all_symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.base();

    // Extract the method name being called
    // In a method_invocation, the last identifier is the method name
    let method_name = {
        let mut last_id = None;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                last_id = Some(base.get_node_text(&child));
            }
        }
        last_id
    };

    let Some(method_name) = method_name else {
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
    match symbol_map.get(method_name.as_str()) {
        Some(called_symbol) if called_symbol.kind == SymbolKind::Import => {
            // Target is an Import symbol - need cross-file resolution
            // Don't create relationship pointing to Import (useless for trace_call_path)
            // Instead, create a PendingRelationship with the method name
            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: caller.id.clone(),
                callee_name: method_name,
                kind: RelationshipKind::Calls,
                file_path,
                line_number,
                confidence: 0.8, // Lower confidence - needs resolution
            });
        }
        Some(called_symbol) => {
            // Target is a local method - create resolved Relationship
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
                callee_name: method_name,
                kind: RelationshipKind::Calls,
                file_path,
                line_number,
                confidence: 0.7, // Lower confidence - unknown target
            });
        }
    }
}

/// Find the method that contains this node
fn find_containing_function(
    extractor: &JavaExtractor,
    node: Node,
    symbols: &[Symbol],
) -> Option<String> {
    let base = extractor.base();
    let file_path = &base.file_path;

    // Walk up the tree to find a method_declaration node
    let mut current = Some(node);
    while let Some(n) = current {
        if n.kind() == "method_declaration" {
            // Extract method name from the declaration
            let method_name = {
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

            if let Some(name) = method_name {
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
