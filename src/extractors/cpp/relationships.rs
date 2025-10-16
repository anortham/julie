//! Relationship extraction for C++
//! Handles inheritance relationships

use crate::extractors::base::{BaseExtractor, Relationship, RelationshipKind, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

use super::helpers;

/// Extract inheritance relationships from C++ code
pub(super) fn extract_relationships(
    base: &mut BaseExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    let mut symbol_map = HashMap::new();

    // Create a lookup map for symbols by name
    for symbol in symbols {
        symbol_map.insert(symbol.name.clone(), symbol);
    }

    // Walk the tree looking for inheritance relationships
    walk_tree_for_relationships(base, tree.root_node(), &symbol_map, &mut relationships);

    relationships
}

/// Recursively walk tree looking for class/struct inheritance
fn walk_tree_for_relationships(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
) {
    if matches!(node.kind(), "class_specifier" | "struct_specifier") {
        let inheritance = extract_inheritance_from_class(base, node, symbol_map);
        relationships.extend(inheritance);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_relationships(base, child, symbol_map, relationships);
    }
}

/// Extract inheritance relationships from a single class node
fn extract_inheritance_from_class(
    base: &mut BaseExtractor,
    class_node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Vec<Relationship> {
    let mut relationships = Vec::new();

    // Get the class name
    let mut cursor = class_node.walk();
    let name_node = class_node
        .children(&mut cursor)
        .find(|c| c.kind() == "type_identifier");

    let Some(name_node) = name_node else {
        return relationships;
    };

    let class_name = base.get_node_text(&name_node);
    let Some(derived_symbol) = symbol_map.get(&class_name) else {
        return relationships;
    };

    // Look for base class clause
    let base_clause = class_node
        .children(&mut class_node.walk())
        .find(|c| c.kind() == "base_class_clause");

    let Some(base_clause) = base_clause else {
        return relationships;
    };

    // Extract base classes
    let base_classes = helpers::extract_base_classes(base, base_clause);
    for base_class in base_classes {
        // Clean base class name (remove access specifiers)
        let clean_base_name = base_class
            .strip_prefix("public ")
            .or_else(|| base_class.strip_prefix("private "))
            .or_else(|| base_class.strip_prefix("protected "))
            .unwrap_or(&base_class);

        if let Some(base_symbol) = symbol_map.get(clean_base_name) {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    derived_symbol.id,
                    base_symbol.id,
                    RelationshipKind::Extends,
                    class_node.start_position().row
                ),
                from_symbol_id: derived_symbol.id.clone(),
                to_symbol_id: base_symbol.id.clone(),
                kind: RelationshipKind::Extends,
                file_path: base.file_path.clone(),
                line_number: (class_node.start_position().row + 1) as u32,
                confidence: 1.0,
                metadata: None,
            });
        }
    }

    relationships
}
