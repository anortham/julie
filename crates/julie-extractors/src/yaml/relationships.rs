use crate::base::{BaseExtractor, Relationship, RelationshipKind, Symbol};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tree_sitter::{Node, Tree};

pub(super) fn extract_relationships(
    base: &BaseExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    let mut seen = HashSet::new();
    walk_tree(
        base,
        tree.root_node(),
        symbols,
        &mut relationships,
        &mut seen,
    );
    relationships
}

fn walk_tree(
    base: &BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
    seen: &mut HashSet<(String, String, u32, String)>,
) {
    if node.kind() == "alias" {
        extract_alias_relationship(base, node, symbols, relationships, seen);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree(base, child, symbols, relationships, seen);
    }
}

fn extract_alias_relationship(
    base: &BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
    seen: &mut HashSet<(String, String, u32, String)>,
) {
    let Some(alias_name) = alias_name(base, node) else {
        return;
    };
    let Some(target) = anchor_target(symbols, &alias_name) else {
        return;
    };
    let Some(source) = base.find_containing_symbol(&node, symbols) else {
        return;
    };

    let line_number = (node.start_position().row + 1) as u32;
    let key = (
        source.id.clone(),
        target.id.clone(),
        line_number,
        alias_name.clone(),
    );
    if !seen.insert(key) {
        return;
    }

    let mut metadata = HashMap::new();
    metadata.insert("alias".to_string(), Value::String(alias_name));

    relationships.push(base.create_relationship(
        source.id.clone(),
        target.id.clone(),
        RelationshipKind::References,
        &node,
        Some(1.0),
        Some(metadata),
    ));
}

fn alias_name(base: &BaseExtractor, node: Node) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "alias_name" {
            return Some(base.get_node_text(&child));
        }
    }
    None
}

fn anchor_target<'a>(symbols: &'a [Symbol], alias_name: &str) -> Option<&'a Symbol> {
    let anchor_pattern = format!("&{}", alias_name);
    symbols.iter().find(|symbol| {
        symbol
            .signature
            .as_ref()
            .is_some_and(|signature| signature.contains(&anchor_pattern))
    })
}
