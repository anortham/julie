//! Relationship extraction for Scala (inheritance, calls)
//!
//! Handles extends/implements relationships and function call relationships.

use crate::base::{
    BaseExtractor, LocalTargetResolution, Relationship, RelationshipKind, ScopedSymbolIndex,
    Symbol, SymbolKind, UnresolvedTarget,
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

            let pending = extractor.base().create_pending_relationship(
                class_symbol.id.clone(),
                UnresolvedTarget::simple(base_type_name),
                pending_kind,
                node,
                Some(class_symbol.id.clone()),
                Some(0.9),
            );
            extractor.add_structured_pending_relationship(pending);
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
                SymbolKind::Class | SymbolKind::Trait | SymbolKind::Interface | SymbolKind::Enum
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
    let symbol_index = ScopedSymbolIndex::new(symbols);

    walk_tree_for_calls(extractor, node, &symbol_index, symbols, relationships);
}

fn walk_tree_for_calls(
    extractor: &mut ScalaExtractor,
    node: Node,
    symbol_index: &ScopedSymbolIndex<'_>,
    all_symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    if node.kind() == "call_expression" {
        extract_single_call(extractor, node, symbol_index, all_symbols, relationships);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_calls(extractor, child, symbol_index, all_symbols, relationships);
    }
}

fn extract_single_call(
    extractor: &mut ScalaExtractor,
    node: Node,
    symbol_index: &ScopedSymbolIndex<'_>,
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

    let Some(caller) = find_innermost_containing_symbol(node, all_symbols) else {
        return;
    };

    let target = unresolved_call_target(extractor, node, &function_name);
    let line_number = node.start_position().row as u32 + 1;
    let file_path = extractor.base().file_path.clone();

    match symbol_index.resolve_call_target(
        function_name.as_str(),
        Some(caller),
        target.receiver.as_deref(),
    ) {
        LocalTargetResolution::Import(_) => {
            let pending = extractor.base().create_pending_relationship(
                caller.id.clone(),
                target,
                RelationshipKind::Calls,
                &node,
                Some(caller.id.clone()),
                Some(0.8),
            );
            extractor.add_structured_pending_relationship(pending);
        }
        LocalTargetResolution::Resolved(called_symbol) => {
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
        LocalTargetResolution::Ambiguous
        | LocalTargetResolution::ReceiverQualified
        | LocalTargetResolution::Missing => {
            let pending = extractor.base().create_pending_relationship(
                caller.id.clone(),
                target,
                RelationshipKind::Calls,
                &node,
                Some(caller.id.clone()),
                Some(0.7),
            );
            extractor.add_structured_pending_relationship(pending);
        }
    }
}

fn find_innermost_containing_symbol<'a>(node: Node, symbols: &'a [Symbol]) -> Option<&'a Symbol> {
    symbols
        .iter()
        .filter(|symbol| {
            node.start_byte() >= symbol.start_byte as usize
                && node.end_byte() <= symbol.end_byte as usize
        })
        .min_by_key(|symbol| symbol.end_byte - symbol.start_byte)
}

fn unresolved_call_target(
    extractor: &ScalaExtractor,
    node: Node,
    fallback_name: &str,
) -> UnresolvedTarget {
    let field_expression = node
        .children(&mut node.walk())
        .find(|child| child.kind() == "field_expression");

    if let Some(field_expression) = field_expression {
        let mut identifiers = Vec::new();
        collect_identifiers(extractor, field_expression, &mut identifiers);
        if identifiers.len() < 2 {
            return UnresolvedTarget::simple(fallback_name.to_string());
        }
        let terminal_name = identifiers
            .pop()
            .unwrap_or_else(|| fallback_name.to_string());
        let receiver = identifiers.pop();
        let namespace_path = identifiers;
        let mut display_parts = namespace_path.clone();
        if let Some(receiver_name) = receiver.as_ref() {
            display_parts.push(receiver_name.clone());
        }
        display_parts.push(terminal_name.clone());
        return UnresolvedTarget {
            display_name: display_parts.join("."),
            terminal_name,
            receiver,
            namespace_path,
            import_context: None,
        };
    }

    UnresolvedTarget::simple(fallback_name.to_string())
}

fn collect_identifiers(extractor: &ScalaExtractor, node: Node, identifiers: &mut Vec<String>) {
    if node.kind() == "identifier" {
        identifiers.push(extractor.base().get_node_text(&node));
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_identifiers(extractor, child, identifiers);
    }
}
