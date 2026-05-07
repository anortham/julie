//! Relationship extraction for function calls and imports
//!
//! This module handles extraction of relationships between symbols, such as function calls
//! and header file imports.

use crate::base::{
    LocalTargetResolution, Relationship, RelationshipKind, ScopedSymbolIndex, Symbol, SymbolKind,
    UnresolvedTarget,
};
use crate::c::CExtractor;
use std::collections::HashMap;

use super::helpers;

/// Extract relationships from nodes in the tree
pub(super) fn extract_relationships_from_node(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let scoped_index = ScopedSymbolIndex::new(symbols);
    walk_relationships(extractor, node, symbols, &scoped_index, relationships);
}

fn walk_relationships(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    scoped_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "call_expression" => {
            extract_function_call_relationships(
                extractor,
                node,
                symbols,
                scoped_index,
                relationships,
            );
        }
        "preproc_include" => {
            extract_include_relationships(extractor, node, relationships);
        }
        "type_identifier" => {
            extract_type_use_relationship(extractor, node, symbols, scoped_index, relationships);
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_relationships(extractor, child, symbols, scoped_index, relationships);
    }
}

/// Extract function call relationships
fn extract_function_call_relationships(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    scoped_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    let Some(function_node) = node.child_by_field_name("function") else {
        return;
    };

    let Some((unresolved_target, is_indirect)) =
        call_target_from_function_node(extractor, function_node)
    else {
        return;
    };
    let Some(containing_symbol) = find_containing_symbol(extractor, node, symbols) else {
        return;
    };
    let containing_symbol_id = containing_symbol.id.clone();
    let pending_confidence = if is_indirect { 0.45 } else { 0.7 };
    let relationship_confidence = if is_indirect { 0.5 } else { 1.0 };

    match scoped_index.resolve_call_target(
        &unresolved_target.terminal_name,
        Some(containing_symbol),
        unresolved_target.receiver.as_deref(),
    ) {
        LocalTargetResolution::Resolved(called_symbol) => {
            relationships.push(extractor.get_base_mut().create_relationship(
                containing_symbol_id,
                called_symbol.id.clone(),
                RelationshipKind::Calls,
                &node,
                Some(relationship_confidence),
                None,
            ));
        }
        LocalTargetResolution::Import(_)
        | LocalTargetResolution::Ambiguous
        | LocalTargetResolution::Missing
        | LocalTargetResolution::ReceiverQualified => {
            let pending = extractor.get_base_mut().create_pending_relationship(
                containing_symbol_id.clone(),
                unresolved_target,
                RelationshipKind::Calls,
                &node,
                Some(containing_symbol_id),
                Some(pending_confidence),
            );
            extractor.add_structured_pending_relationship(pending);
        }
    }
}

fn call_target_from_function_node(
    extractor: &mut CExtractor,
    function_node: tree_sitter::Node,
) -> Option<(UnresolvedTarget, bool)> {
    match function_node.kind() {
        "identifier" => {
            let function_name = extractor.get_base_mut().get_node_text(&function_node);
            Some((UnresolvedTarget::simple(function_name), false))
        }
        "field_expression" | "pointer_expression" => {
            if let Some(field_node) = function_node.child_by_field_name("field") {
                let terminal_name = extractor.get_base_mut().get_node_text(&field_node);
                let expression_text = extractor.get_base_mut().get_node_text(&function_node);
                let receiver = expression_text
                    .rsplit_once("->")
                    .or_else(|| expression_text.rsplit_once('.'))
                    .map(|(left, _)| left.trim().to_string())
                    .filter(|left| !left.is_empty());

                let target = if let Some(receiver) = receiver {
                    UnresolvedTarget {
                        display_name: expression_text,
                        terminal_name,
                        receiver: Some(receiver),
                        namespace_path: Vec::new(),
                        import_context: None,
                    }
                } else {
                    UnresolvedTarget::simple(terminal_name)
                };
                Some((target, true))
            } else {
                let identifier = helpers::find_deepest_identifier(function_node)?;
                let function_name = extractor.get_base_mut().get_node_text(&identifier);
                Some((UnresolvedTarget::simple(function_name), true))
            }
        }
        "parenthesized_expression" | "subscript_expression" => {
            let identifier = helpers::find_deepest_identifier(function_node)?;
            let function_name = extractor.get_base_mut().get_node_text(&identifier);
            Some((UnresolvedTarget::simple(function_name), true))
        }
        _ => {
            let identifier = helpers::find_deepest_identifier(function_node)?;
            let function_name = extractor.get_base_mut().get_node_text(&identifier);
            Some((UnresolvedTarget::simple(function_name), true))
        }
    }
}

fn extract_type_use_relationship(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    scoped_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    if is_c_type_declaration_name(node) {
        return;
    }

    let type_name = extractor.get_base_mut().get_node_text(&node);
    let Some(source_symbol) = source_symbol_for_type_use(extractor, node, symbols) else {
        return;
    };
    let source_symbol_id = source_symbol.id.clone();

    if let Some(target_symbol) = resolve_type_target(scoped_index, &type_name) {
        if target_symbol.id == source_symbol_id {
            return;
        }
        push_unique_relationship(
            relationships,
            extractor.get_base_mut().create_relationship(
                source_symbol_id,
                target_symbol.id.clone(),
                RelationshipKind::Uses,
                &node,
                Some(0.8),
                None,
            ),
        );
    } else {
        let pending = extractor.get_base_mut().create_pending_relationship(
            source_symbol_id.clone(),
            UnresolvedTarget::simple(type_name),
            RelationshipKind::Uses,
            &node,
            Some(source_symbol_id),
            Some(0.7),
        );
        extractor.add_structured_pending_relationship(pending);
    }
}

/// Extract include file relationships
fn extract_include_relationships(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    relationships: &mut Vec<Relationship>,
) {
    let Some(include_path) = helpers::extract_include_path(&extractor.base.get_node_text(&node))
    else {
        return;
    };
    let from_id = format!("file:{}", extractor.base.file_path);
    let to_id = format!("header:{}", include_path);
    relationships.push(Relationship {
        id: format!(
            "{}_{}_{:?}_{}",
            from_id,
            to_id,
            RelationshipKind::Imports,
            node.start_position().row
        ),
        from_symbol_id: from_id,
        to_symbol_id: to_id,
        kind: RelationshipKind::Imports,
        file_path: extractor.base.file_path.clone(),
        line_number: (node.start_position().row + 1) as u32,
        confidence: 1.0,
        metadata: Some(HashMap::from([(
            "includePath".to_string(),
            serde_json::Value::String(include_path),
        )])),
    });
}

/// Find the symbol that contains this node
fn find_containing_symbol<'a>(
    extractor: &CExtractor,
    node: tree_sitter::Node,
    symbols: &'a [Symbol],
) -> Option<&'a Symbol> {
    // Reuse the shared BaseExtractor containment logic so we correctly match
    // call expressions with their enclosing function definition (standard approach).
    extractor.base.find_containing_symbol(&node, symbols)
}

fn source_symbol_for_type_use<'a>(
    extractor: &CExtractor,
    node: tree_sitter::Node,
    symbols: &'a [Symbol],
) -> Option<&'a Symbol> {
    let containing = extractor.base.find_containing_symbol(&node, symbols)?;
    if matches!(containing.kind, SymbolKind::Field | SymbolKind::Property) {
        if let Some(parent_id) = containing.parent_id.as_deref() {
            if let Some(parent) = symbols.iter().find(|symbol| symbol.id == parent_id) {
                return Some(parent);
            }
        }
    }
    Some(containing)
}

fn resolve_type_target<'a>(
    scoped_index: &'a ScopedSymbolIndex<'a>,
    type_name: &str,
) -> Option<&'a Symbol> {
    let candidates: Vec<&Symbol> = scoped_index
        .candidates_by_name(type_name)
        .filter(|symbol| is_type_symbol(&symbol.kind))
        .collect();
    if let [candidate] = candidates.as_slice() {
        return Some(*candidate);
    }

    let top_level: Vec<&Symbol> = candidates
        .iter()
        .copied()
        .filter(|symbol| symbol.parent_id.is_none())
        .collect();
    if let [candidate] = top_level.as_slice() {
        return Some(*candidate);
    }

    None
}

fn is_type_symbol(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Class
            | SymbolKind::Struct
            | SymbolKind::Union
            | SymbolKind::Enum
            | SymbolKind::Type
            | SymbolKind::Interface
            | SymbolKind::Trait
    )
}

fn is_c_type_declaration_name(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };

    match parent.kind() {
        "struct_specifier" | "union_specifier" | "enum_specifier" => {
            parent.child_by_field_name("body").is_some()
        }
        "type_definition" => parent
            .child_by_field_name("declarator")
            .is_some_and(|declarator| declarator.id() == node.id()),
        _ => false,
    }
}

fn push_unique_relationship(relationships: &mut Vec<Relationship>, relationship: Relationship) {
    if relationships.iter().any(|existing| {
        existing.kind == relationship.kind
            && existing.from_symbol_id == relationship.from_symbol_id
            && existing.to_symbol_id == relationship.to_symbol_id
    }) {
        return;
    }
    relationships.push(relationship);
}
