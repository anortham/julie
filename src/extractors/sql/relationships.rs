//! Relationship extraction (foreign keys, joins, table references).
//!
//! Handles extraction of relationships between tables and other objects:
//! - Foreign key relationships
//! - JOIN operations
//! - Table references in queries

use crate::extractors::base::{BaseExtractor, Relationship, RelationshipKind, Symbol, SymbolKind};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract relationships recursively from tree
pub(super) fn extract_relationships_internal(
    base: &mut BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "constraint" => {
            // Check if this is a foreign key constraint
            let has_foreign = base.find_child_by_type(&node, "keyword_foreign");
            if has_foreign.is_some() {
                extract_foreign_key_relationship(base, node, symbols, relationships);
            }
        }
        "foreign_key_constraint" | "references_clause" => {
            extract_foreign_key_relationship(base, node, symbols, relationships);
        }
        "select_statement" | "from_clause" => {
            extract_table_references(base, node, symbols, relationships);
        }
        "join" | "join_clause" => {
            extract_join_relationships(base, node, symbols, relationships);
        }
        _ => {}
    }

    // Recursively visit children
    for child in node.children(&mut node.walk()) {
        extract_relationships_internal(base, child, symbols, relationships);
    }
}

/// Extract foreign key relationship from FOREIGN KEY constraint
pub(super) fn extract_foreign_key_relationship(
    base: &mut BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Port extractForeignKeyRelationship logic
    // Extract foreign key relationships between tables
    // Look for object_reference after keyword_references
    let references_keyword = base.find_child_by_type(&node, "keyword_references");
    if references_keyword.is_none() {
        return;
    }

    let object_ref_node = base.find_child_by_type(&node, "object_reference");
    let referenced_table_node = if let Some(obj_ref) = object_ref_node {
        base.find_child_by_type(&obj_ref, "identifier")
    } else {
        base.find_child_by_type(&node, "table_name")
            .or_else(|| base.find_child_by_type(&node, "identifier"))
    };

    let referenced_table_node = match referenced_table_node {
        Some(node) => node,
        None => return,
    };

    let referenced_table = base.get_node_text(&referenced_table_node);

    // Find the source table (parent of this foreign key)
    let mut current_node = node.parent();
    while let Some(current) = current_node {
        if current.kind() == "create_table" {
            break;
        }
        current_node = current.parent();
    }

    let current_node = match current_node {
        Some(node) => node,
        None => return,
    };

    // Look for table name in object_reference (same pattern as extractTableDefinition)
    let source_object_ref_node = base.find_child_by_type(&current_node, "object_reference");
    let source_table_node = if let Some(obj_ref) = source_object_ref_node {
        base.find_child_by_type(&obj_ref, "identifier")
    } else {
        base.find_child_by_type(&current_node, "identifier")
            .or_else(|| base.find_child_by_type(&current_node, "table_name"))
    };

    let source_table_node = match source_table_node {
        Some(node) => node,
        None => return,
    };

    let source_table = base.get_node_text(&source_table_node);

    // Find corresponding symbols
    let source_symbol = symbols
        .iter()
        .find(|s| s.name == source_table && s.kind == SymbolKind::Class);
    let target_symbol = symbols
        .iter()
        .find(|s| s.name == referenced_table && s.kind == SymbolKind::Class);

    // Create relationship if we have at least the source symbol
    if let Some(source_symbol) = source_symbol {
        let mut metadata = HashMap::new();
        metadata.insert(
            "targetTable".to_string(),
            Value::String(referenced_table.clone()),
        );
        metadata.insert("sourceTable".to_string(), Value::String(source_table));
        metadata.insert(
            "relationshipType".to_string(),
            Value::String("foreign_key".to_string()),
        );
        metadata.insert(
            "isExternal".to_string(),
            Value::Bool(target_symbol.is_none()),
        );

        relationships.push(Relationship {
            id: format!(
                "{}_{}_{:?}_{}",
                source_symbol.id,
                target_symbol
                    .map(|s| s.id.clone())
                    .unwrap_or_else(|| format!("external_{}", referenced_table)),
                RelationshipKind::References,
                node.start_position().row
            ),
            from_symbol_id: source_symbol.id.clone(),
            to_symbol_id: target_symbol
                .map(|s| s.id.clone())
                .unwrap_or_else(|| format!("external_{}", referenced_table)),
            kind: RelationshipKind::References,
            file_path: base.file_path.clone(),
            line_number: node.start_position().row as u32,
            confidence: if target_symbol.is_some() { 1.0 } else { 0.8 },
            metadata: Some(metadata),
        });
    }
}

/// Extract table references in SELECT statements
pub(super) fn extract_table_references(
    base: &mut BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    _relationships: &mut Vec<Relationship>,
) {
    // Port extractTableReferences logic
    base.traverse_tree(&node, &mut |child_node| {
        if child_node.kind() == "table_name"
            || (child_node.kind() == "identifier"
                && child_node
                    .parent()
                    .is_some_and(|p| p.kind() == "from_clause"))
        {
            let table_name = base.get_node_text(child_node);
            let _table_symbol = symbols
                .iter()
                .find(|s| s.name == table_name && s.kind == SymbolKind::Class);
        }
    });
}

/// Extract JOIN relationships
pub(super) fn extract_join_relationships(
    base: &mut BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Port extractJoinRelationships logic
    base.traverse_tree(&node, &mut |child_node| {
        if child_node.kind() == "table_name"
            || (child_node.kind() == "identifier"
                && child_node
                    .parent()
                    .is_some_and(|p| p.kind() == "object_reference"))
        {
            let table_name = base.get_node_text(child_node);
            let table_symbol = symbols
                .iter()
                .find(|s| s.name == table_name && s.kind == SymbolKind::Class);

            if let Some(table_symbol) = table_symbol {
                // Create a join relationship
                let mut metadata = HashMap::new();
                metadata.insert("joinType".to_string(), Value::String("join".to_string()));
                metadata.insert("tableName".to_string(), Value::String(table_name.clone()));

                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        table_symbol.id,
                        table_symbol.id,
                        RelationshipKind::Joins,
                        node.start_position().row
                    ),
                    from_symbol_id: table_symbol.id.clone(),
                    to_symbol_id: table_symbol.id.clone(),
                    kind: RelationshipKind::Joins,
                    file_path: base.file_path.clone(),
                    line_number: node.start_position().row as u32,
                    confidence: 0.9,
                    metadata: Some(metadata),
                });
            }
        }
    });
}
