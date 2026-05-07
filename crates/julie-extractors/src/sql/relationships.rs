//! Relationship extraction (foreign keys, joins, table references).
//!
//! Handles extraction of relationships between tables and other objects:
//! - Foreign key relationships
//! - JOIN operations
//! - Table references in queries

use crate::base::{BaseExtractor, Relationship, RelationshipKind, Symbol, SymbolKind};
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
        // NOTE: select_statement/from_clause table references intentionally not extracted.
        // The original extract_table_references was a stub that found table names but
        // never created relationships. If needed in the future, implement here.
        "select_statement" | "from_clause" => {}
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

    // SQL is in NO_PENDING_CAPABILITIES: only emit a relationship when both the
    // source and target tables are defined in this file. A missing target means
    // the referenced table lives in another file; we suppress the relationship
    // entirely rather than emitting a dead synthetic ID like "external_users".
    if let (Some(source_symbol), Some(target_symbol)) = (source_symbol, target_symbol) {
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
        metadata.insert("isExternal".to_string(), Value::Bool(false));

        relationships.push(Relationship {
            id: format!(
                "{}_{}_{:?}_{}",
                source_symbol.id,
                target_symbol.id,
                RelationshipKind::References,
                node.start_position().row
            ),
            from_symbol_id: source_symbol.id.clone(),
            to_symbol_id: target_symbol.id.clone(),
            kind: RelationshipKind::References,
            file_path: base.file_path.clone(),
            line_number: node.start_position().row as u32 + 1,
            confidence: 1.0,
            metadata: Some(metadata),
        });
    }
}

/// Extract JOIN relationships
fn first_child_by_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    for i in 0..node.child_count() {
        let child = node.child(i as u32)?;
        if child.kind() == kind {
            return Some(child);
        }
    }
    None
}

fn table_symbol_from_relation<'a>(
    base: &BaseExtractor,
    relation_node: Node,
    symbols: &'a [Symbol],
) -> Option<(&'a Symbol, String)> {
    let object_reference = first_child_by_kind(relation_node, "object_reference")?;
    let name_node = object_reference
        .child_by_field_name("name")
        .or_else(|| first_child_by_kind(object_reference, "identifier"))?;
    let table_name = base.get_node_text(&name_node);
    let table_symbol = symbols
        .iter()
        .find(|s| s.name == table_name && s.kind == SymbolKind::Class)?;

    Some((table_symbol, table_name))
}

fn enclosing_from_node(mut node: Node) -> Option<Node> {
    while let Some(parent) = node.parent() {
        if parent.kind() == "from" {
            return Some(parent);
        }
        node = parent;
    }
    None
}

pub(super) fn extract_join_relationships(
    base: &mut BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let Some(from_node) = enclosing_from_node(node) else {
        return;
    };
    let Some(source_relation) = first_child_by_kind(from_node, "relation") else {
        return;
    };
    let Some((source_symbol, _source_table_name)) =
        table_symbol_from_relation(base, source_relation, symbols)
    else {
        return;
    };
    let Some(target_relation) = first_child_by_kind(node, "relation") else {
        return;
    };
    let Some((target_symbol, target_table_name)) =
        table_symbol_from_relation(base, target_relation, symbols)
    else {
        return;
    };

    // Create a join relationship from the FROM-side table to the joined table.
    let mut metadata = HashMap::new();
    metadata.insert("joinType".to_string(), Value::String("join".to_string()));
    metadata.insert("tableName".to_string(), Value::String(target_table_name));

    relationships.push(Relationship {
        id: format!(
            "{}_{}_{:?}_{}",
            source_symbol.id,
            target_symbol.id,
            RelationshipKind::Joins,
            node.start_position().row
        ),
        from_symbol_id: source_symbol.id.clone(),
        to_symbol_id: target_symbol.id.clone(),
        kind: RelationshipKind::Joins,
        file_path: base.file_path.clone(),
        line_number: node.start_position().row as u32 + 1,
        confidence: 0.9,
        metadata: Some(metadata),
    });
}
