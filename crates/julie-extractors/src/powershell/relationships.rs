//! PowerShell relationship extraction
//! Handles inheritance, method calls, and other symbol relationships

use crate::base::{BaseExtractor, PendingRelationship, Relationship, RelationshipKind, Symbol, SymbolKind};
use tree_sitter::Node;

use super::helpers::{extract_inheritance, find_class_name_node, find_command_name_node};

/// Extract relationships from the AST
pub(super) fn walk_tree_for_relationships(
    extractor: &mut super::PowerShellExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "command" | "command_expression" | "pipeline" | "pipeline_expression" => {
            extract_command_relationships(extractor, node, symbols, relationships);
        }
        "class_definition" | "class_statement" => {
            extract_inheritance_relationships(&extractor.base, node, symbols, relationships);
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_relationships(extractor, child, symbols, relationships);
    }
}

/// Extract relationships from command calls
fn extract_command_relationships(
    extractor: &mut super::PowerShellExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    if let Some(command_name_node) = find_command_name_node(node) {
        let command_name = extractor.base.get_node_text(&command_name_node);

        // Create a symbol_map for quick lookup
        let symbol_map: std::collections::HashMap<&str, &Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .map(|s| (s.name.as_str(), s))
            .collect();

        // Find the parent function that calls this command
        let mut current = node.parent();
        while let Some(n) = current {
            if n.kind() == "function_statement" {
                if let Some(func_name_node) = super::helpers::find_function_name_node(n) {
                    let func_name = extractor.base.get_node_text(&func_name_node);
                    if let Some(func_symbol) = symbols
                        .iter()
                        .find(|s| s.name == func_name && s.kind == SymbolKind::Function)
                    {
                        // Check if the called command is in the local symbols
                        match symbol_map.get(command_name.as_str()) {
                            Some(command_symbol) => {
                                // Local function call - create resolved relationship
                                if func_symbol.id != command_symbol.id {
                                    relationships.push(extractor.base.create_relationship(
                                        func_symbol.id.clone(),
                                        command_symbol.id.clone(),
                                        RelationshipKind::Calls,
                                        &node,
                                        None,
                                        None,
                                    ));
                                }
                            }
                            None => {
                                // Command not in local symbols - create pending relationship
                                let line_number = (node.start_position().row + 1) as u32;
                                let file_path = extractor.base.file_path.clone();

                                extractor.add_pending_relationship(PendingRelationship {
                                    from_symbol_id: func_symbol.id.clone(),
                                    callee_name: command_name.clone(),
                                    kind: RelationshipKind::Calls,
                                    file_path,
                                    line_number,
                                    confidence: 0.7, // Lower confidence - cross-file call
                                });
                            }
                        }
                    }
                    break;
                }
                current = n.parent();
            } else {
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
