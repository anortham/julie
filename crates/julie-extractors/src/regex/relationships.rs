use crate::base::{BaseExtractor, Relationship, RelationshipKind, Symbol};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tree_sitter::{Node, Tree};

use super::flags;

pub(super) fn extract_relationships(
    base: &BaseExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let named_groups = named_group_symbols(symbols);
    let numbered_groups = numbered_group_symbols(symbols);
    let mut relationships = Vec::new();
    let mut seen = HashSet::new();

    visit_node(
        base,
        tree.root_node(),
        symbols,
        &named_groups,
        &numbered_groups,
        &mut relationships,
        &mut seen,
    );

    relationships
}

fn visit_node(
    base: &BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    named_groups: &HashMap<String, &Symbol>,
    numbered_groups: &HashMap<usize, &Symbol>,
    relationships: &mut Vec<Relationship>,
    seen: &mut HashSet<(String, String, u32, usize)>,
) {
    if let Some(group_name) = named_backreference_name(base, node) {
        if let Some(target) = named_groups.get(&group_name) {
            if let Some(source) = base.find_containing_symbol(&node, symbols) {
                push_backreference_relationship(
                    base,
                    source,
                    target,
                    node,
                    seen,
                    relationships,
                    "named-backreference",
                    Some(("name", Value::String(group_name))),
                );
            }
        }
    }

    if let Some(group_number) = numeric_backreference_number(base, node) {
        if let Some(target) = numbered_groups.get(&group_number) {
            if let Some(source) = base.find_containing_symbol(&node, symbols) {
                push_backreference_relationship(
                    base,
                    source,
                    target,
                    node,
                    seen,
                    relationships,
                    "numeric-backreference",
                    Some(("captureIndex", Value::Number((group_number as u64).into()))),
                );
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_node(
            base,
            child,
            symbols,
            named_groups,
            numbered_groups,
            relationships,
            seen,
        );
    }
}

fn named_group_symbols(symbols: &[Symbol]) -> HashMap<String, &Symbol> {
    symbols
        .iter()
        .filter_map(|symbol| {
            let name = symbol
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("named"))
                .and_then(Value::as_str)?;
            Some((name.to_string(), symbol))
        })
        .collect()
}

fn numbered_group_symbols(symbols: &[Symbol]) -> HashMap<usize, &Symbol> {
    symbols
        .iter()
        .filter_map(|symbol| {
            let capture_index = symbol
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("captureIndex"))
                .and_then(Value::as_u64)? as usize;
            Some((capture_index, symbol))
        })
        .collect()
}

fn push_backreference_relationship(
    base: &BaseExtractor,
    source: &Symbol,
    target: &Symbol,
    node: Node,
    seen: &mut HashSet<(String, String, u32, usize)>,
    relationships: &mut Vec<Relationship>,
    reference_type: &str,
    extra_metadata: Option<(&str, Value)>,
) {
    let line = (node.start_position().row + 1) as u32;
    let key = (
        source.id.clone(),
        target.id.clone(),
        line,
        node.start_byte(),
    );
    if !seen.insert(key) {
        return;
    }

    let mut metadata = HashMap::new();
    metadata.insert(
        "referenceType".to_string(),
        Value::String(reference_type.to_string()),
    );
    if let Some((key, value)) = extra_metadata {
        metadata.insert(key.to_string(), value);
    }

    relationships.push(base.create_relationship(
        source.id.clone(),
        target.id.clone(),
        RelationshipKind::References,
        &node,
        Some(1.0),
        Some(metadata),
    ));
}

fn named_backreference_name(base: &BaseExtractor, node: Node) -> Option<String> {
    match node.kind() {
        "backreference_escape" => {
            let content_after = base.content.get(node.start_byte()..)?;
            if !content_after.starts_with("\\k<") {
                return None;
            }
            let end_pos = content_after.find('>')?;
            if content_after.is_char_boundary(3) && content_after.is_char_boundary(end_pos) {
                let group_name = &content_after[3..end_pos];
                (!group_name.is_empty()).then(|| group_name.to_string())
            } else {
                None
            }
        }
        "backreference" => {
            let text = base.get_node_text(&node);
            flags::extract_backref_group_name(&text)
        }
        _ => None,
    }
}

fn numeric_backreference_number(base: &BaseExtractor, node: Node) -> Option<usize> {
    if node.kind() != "decimal_escape" {
        return None;
    }

    let text = base.get_node_text(&node);
    flags::extract_group_number(&text)?.parse().ok()
}

pub(super) fn referenced_capture_numbers(base: &BaseExtractor, tree: &Tree) -> HashSet<usize> {
    let mut numbers = HashSet::new();
    collect_referenced_capture_numbers(base, tree.root_node(), &mut numbers);
    numbers
}

fn collect_referenced_capture_numbers(
    base: &BaseExtractor,
    node: Node,
    numbers: &mut HashSet<usize>,
) {
    if let Some(number) = numeric_backreference_number(base, node) {
        numbers.insert(number);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_referenced_capture_numbers(base, child, numbers);
    }
}
