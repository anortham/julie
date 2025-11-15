// PHP Extractor - Relationship extraction (inheritance, implementation)

use super::{PhpExtractor, find_child};
use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract class inheritance and implementation relationships
pub(super) fn extract_class_relationships(
    extractor: &mut PhpExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let class_symbol = find_class_symbol(extractor, node, symbols);
    if class_symbol.is_none() {
        return;
    }
    let class_symbol = class_symbol.unwrap();

    // Inheritance relationships
    if let Some(extends_node) = find_child(extractor, &node, "base_clause") {
        let base_class_name = extractor
            .get_base()
            .get_node_text(&extends_node)
            .replace("extends", "")
            .trim()
            .to_string();
        // Find the actual symbol for the base class
        if let Some(base_class_symbol) = symbols
            .iter()
            .find(|s| s.name == base_class_name && s.kind == SymbolKind::Class)
        {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    class_symbol.id,
                    base_class_symbol.id,
                    RelationshipKind::Extends,
                    node.start_position().row
                ),
                from_symbol_id: class_symbol.id.clone(),
                to_symbol_id: base_class_symbol.id.clone(),
                kind: RelationshipKind::Extends,
                file_path: extractor.get_base().file_path.clone(),
                line_number: node.start_position().row as u32 + 1,
                confidence: 1.0,
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "baseClass".to_string(),
                        serde_json::Value::String(base_class_name),
                    );
                    metadata
                }),
            });
        }
    }

    // Implementation relationships
    if let Some(implements_node) = find_child(extractor, &node, "class_interface_clause") {
        let interface_names: Vec<String> = extractor
            .get_base()
            .get_node_text(&implements_node)
            .replace("implements", "")
            .split(',')
            .map(|name| name.trim().to_string())
            .collect();

        for interface_name in interface_names {
            // Find the actual interface symbol
            let interface_symbol = symbols.iter().find(|s| {
                s.name == interface_name
                    && s.kind == SymbolKind::Interface
                    && s.file_path == extractor.get_base().file_path
            });

            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    class_symbol.id,
                    interface_symbol
                        .map(|s| s.id.clone())
                        .unwrap_or_else(|| format!("php-interface:{}", interface_name)),
                    RelationshipKind::Implements,
                    node.start_position().row
                ),
                from_symbol_id: class_symbol.id.clone(),
                to_symbol_id: interface_symbol
                    .map(|s| s.id.clone())
                    .unwrap_or_else(|| format!("php-interface:{}", interface_name)),
                kind: RelationshipKind::Implements,
                file_path: extractor.get_base().file_path.clone(),
                line_number: node.start_position().row as u32 + 1,
                confidence: if interface_symbol.is_some() { 1.0 } else { 0.8 },
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "interface".to_string(),
                        serde_json::Value::String(interface_name),
                    );
                    metadata
                }),
            });
        }
    }
}

/// Extract interface inheritance relationships
pub(super) fn extract_interface_relationships(
    extractor: &mut PhpExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let interface_symbol = find_interface_symbol(extractor, node, symbols);
    if interface_symbol.is_none() {
        return;
    }
    let interface_symbol = interface_symbol.unwrap();

    // Interface inheritance
    if let Some(extends_node) = find_child(extractor, &node, "base_clause") {
        let base_interface_names: Vec<String> = extractor
            .get_base()
            .get_node_text(&extends_node)
            .replace("extends", "")
            .split(',')
            .map(|name| name.trim().to_string())
            .collect();

        for base_interface_name in base_interface_names {
            let to_id = format!("php-interface:{}", base_interface_name);
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    interface_symbol.id,
                    to_id,
                    RelationshipKind::Extends,
                    node.start_position().row
                ),
                from_symbol_id: interface_symbol.id.clone(),
                to_symbol_id: to_id,
                kind: RelationshipKind::Extends,
                file_path: extractor.get_base().file_path.clone(),
                line_number: node.start_position().row as u32 + 1,
                confidence: 1.0,
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "baseInterface".to_string(),
                        serde_json::Value::String(base_interface_name),
                    );
                    metadata
                }),
            });
        }
    }
}

/// Find class symbol by node
pub(super) fn find_class_symbol<'a>(
    extractor: &PhpExtractor,
    node: Node,
    symbols: &'a [Symbol],
) -> Option<&'a Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = extractor.get_base().get_node_text(&name_node);

    symbols.iter().find(|s| {
        s.name == name
            && s.kind == SymbolKind::Class
            && s.file_path == extractor.get_base().file_path
    })
}

/// Find interface symbol by node
pub(super) fn find_interface_symbol<'a>(
    extractor: &PhpExtractor,
    node: Node,
    symbols: &'a [Symbol],
) -> Option<&'a Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = extractor.get_base().get_node_text(&name_node);

    symbols.iter().find(|s| {
        s.name == name
            && s.kind == SymbolKind::Interface
            && s.file_path == extractor.get_base().file_path
    })
}
