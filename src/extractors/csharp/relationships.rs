// C# Relationship Extraction

use crate::extractors::base::{BaseExtractor, Relationship, RelationshipKind, Symbol, SymbolKind};
use tree_sitter::Tree;

/// Extract relationships from the tree
pub fn extract_relationships(
    base: &BaseExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    visit_relationships(base, tree.root_node(), symbols, &mut relationships);
    relationships
}

fn visit_relationships(
    base: &BaseExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "class_declaration" | "interface_declaration" | "struct_declaration" => {
            extract_inheritance_relationships(base, node, symbols, relationships);
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_relationships(base, child, symbols, relationships);
    }
}

fn extract_inheritance_relationships(
    base: &BaseExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let mut cursor = node.walk();
    let name_node = node
        .children(&mut cursor)
        .find(|c| c.kind() == "identifier");
    let Some(name_node) = name_node else { return };

    let current_symbol_name = base.get_node_text(&name_node);
    let Some(current_symbol) = symbols.iter().find(|s| s.name == current_symbol_name) else {
        return;
    };

    let base_list = node.children(&mut cursor).find(|c| c.kind() == "base_list");
    let Some(base_list) = base_list else { return };

    let mut base_cursor = base_list.walk();
    let base_types: Vec<String> = base_list
        .children(&mut base_cursor)
        .filter(|c| c.kind() != ":" && c.kind() != ",")
        .map(|c| base.get_node_text(&c))
        .collect();

    for base_type_name in base_types {
        if let Some(base_symbol) = symbols.iter().find(|s| s.name == base_type_name) {
            let relationship_kind = if base_symbol.kind == SymbolKind::Interface {
                RelationshipKind::Implements
            } else {
                RelationshipKind::Extends
            };

            let relationship = Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    current_symbol.id,
                    base_symbol.id,
                    relationship_kind,
                    node.start_position().row
                ),
                from_symbol_id: current_symbol.id.clone(),
                to_symbol_id: base_symbol.id.clone(),
                kind: relationship_kind,
                file_path: base.file_path.clone(),
                line_number: (node.start_position().row + 1) as u32,
                confidence: 1.0,
                metadata: None,
            };

            relationships.push(relationship);
        }
    }
}
