//! Relationship extraction for function calls and imports
//!
//! This module handles extraction of relationships between symbols, such as function calls
//! and header file imports.

use crate::base::{PendingRelationship, Relationship, RelationshipKind, Symbol, SymbolKind};
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
    match node.kind() {
        "call_expression" => {
            extract_function_call_relationships(extractor, node, symbols, relationships);
        }
        "preproc_include" => {
            extract_include_relationships(extractor, node, relationships);
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_relationships_from_node(extractor, child, symbols, relationships);
    }
}

/// Extract function call relationships
fn extract_function_call_relationships(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    if let Some(function_node) = node.child_by_field_name("function") {
        if function_node.kind() == "identifier" {
            let function_name = extractor.get_base_mut().get_node_text(&function_node);

            if let Some(called_symbol) = symbols
                .iter()
                .find(|s| s.name == function_name && s.kind == SymbolKind::Function)
            {
                // Target function found locally - create resolved Relationship
                if let Some(containing_symbol) = find_containing_symbol(extractor, node, symbols) {
                    relationships.push(extractor.get_base_mut().create_relationship(
                        containing_symbol.id.clone(),
                        called_symbol.id.clone(),
                        RelationshipKind::Calls,
                        &node,
                        None,
                        None,
                    ));
                }
            } else {
                // Target not found in local symbols - likely a function from included header
                // Create PendingRelationship for cross-file resolution
                if let Some(containing_symbol) = find_containing_symbol(extractor, node, symbols) {
                    let file_path = extractor.get_base_mut().file_path.clone();
                    let line_number = node.start_position().row as u32 + 1;
                    extractor.add_pending_relationship(PendingRelationship {
                        from_symbol_id: containing_symbol.id.clone(),
                        callee_name: function_name,
                        kind: RelationshipKind::Calls,
                        file_path,
                        line_number,
                        confidence: 0.7, // Lower confidence - unknown target
                    });
                }
            }
        }
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
