//! PowerShell relationship extraction
//! Handles inheritance, method calls, and other symbol relationships

use crate::extractors::base::{BaseExtractor, Relationship, RelationshipKind, Symbol, SymbolKind};
use tree_sitter::Node;

use super::helpers::{extract_inheritance, find_class_name_node, find_command_name_node};

/// Extract relationships from the AST
pub(super) fn walk_tree_for_relationships(
    base: &BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "command_expression" | "pipeline_expression" => {
            extract_command_relationships(base, node, symbols, relationships);
        }
        "class_definition" => {
            extract_inheritance_relationships(base, node, symbols, relationships);
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_relationships(base, child, symbols, relationships);
    }
}

/// Extract relationships from command calls
fn extract_command_relationships(
    base: &BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    if let Some(command_name_node) = find_command_name_node(node) {
        let command_name = base.get_node_text(&command_name_node);
        if let Some(command_symbol) = symbols
            .iter()
            .find(|s| s.name == command_name && s.kind == SymbolKind::Function)
        {
            // Find the parent function that calls this command
            let mut current = Some(node);
            while let Some(n) = current {
                if n.kind() == "function_definition" {
                    if let Some(func_name_node) = super::helpers::find_function_name_node(n) {
                        let func_name = base.get_node_text(&func_name_node);
                        if let Some(func_symbol) = symbols
                            .iter()
                            .find(|s| s.name == func_name && s.kind == SymbolKind::Function)
                        {
                            if func_symbol.id != command_symbol.id {
                                relationships.push(base.create_relationship(
                                    func_symbol.id.clone(),
                                    command_symbol.id.clone(),
                                    RelationshipKind::Calls,
                                    &node,
                                    None,
                                    None,
                                ));
                            }
                        }
                    }
                    break;
                }
                current = n.parent();
            }
        }
    }
}

/// Extract inheritance relationships between classes
fn extract_inheritance_relationships(
    base: &BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    if let Some(inheritance) = extract_inheritance(base, node) {
        if let Some(class_name_node) = find_class_name_node(node) {
            let class_name = base.get_node_text(&class_name_node);
            let child_class = symbols
                .iter()
                .find(|s| s.name == class_name && s.kind == SymbolKind::Class);
            let parent_class = symbols
                .iter()
                .find(|s| s.name == inheritance && s.kind == SymbolKind::Class);

            if let (Some(child), Some(parent)) = (child_class, parent_class) {
                relationships.push(base.create_relationship(
                    child.id.clone(),
                    parent.id.clone(),
                    RelationshipKind::Extends,
                    &node,
                    None,
                    None,
                ));
            }
        }
    }
}
