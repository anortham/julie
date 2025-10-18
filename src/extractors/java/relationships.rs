/// Inheritance and implementation relationship extraction
use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind};
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
