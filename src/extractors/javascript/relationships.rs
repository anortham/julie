//! Relationship extraction for JavaScript (calls, inheritance)
//!
//! This module handles extraction of relationships between symbols such as
//! function calls and class inheritance relationships.
//!
//! Adapted from TypeScript extractor (JavaScript and TypeScript share AST structure)

use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::extractors::javascript::JavaScriptExtractor;
use tree_sitter::{Node, Tree};

/// Extract all relationships from the syntax tree
pub(crate) fn extract_relationships(
    extractor: &JavaScriptExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    extract_call_relationships(extractor, tree.root_node(), symbols, &mut relationships);
    extract_inheritance_relationships(extractor, tree.root_node(), symbols, &mut relationships);
    relationships
}

/// Extract function call relationships
fn extract_call_relationships(
    extractor: &JavaScriptExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Look for call expressions
    if node.kind() == "call_expression" {
        if let Some(function_node) = node.child_by_field_name("function") {
            let function_name = extractor.base().get_node_text(&function_node);

            // Find the calling function (containing function)
            if let Some(caller_symbol) = find_containing_function(node, symbols) {
                // Find the called function symbol
                if let Some(called_symbol) = symbols
                    .iter()
                    .find(|s| s.name == function_name && matches!(s.kind, SymbolKind::Function))
                {
                    let relationship = Relationship {
                        id: format!(
                            "{}_{}_{:?}_{}",
                            caller_symbol.id,
                            called_symbol.id,
                            RelationshipKind::Calls,
                            node.start_position().row
                        ),
                        from_symbol_id: caller_symbol.id.clone(),
                        to_symbol_id: called_symbol.id.clone(),
                        kind: RelationshipKind::Calls,
                        file_path: extractor.base().file_path.clone(),
                        line_number: (node.start_position().row + 1) as u32,
                        confidence: 1.0,
                        metadata: None,
                    };
                    relationships.push(relationship);
                }
            }
        }
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_call_relationships(extractor, child, symbols, relationships);
    }
}

/// Extract inheritance relationships (extends)
fn extract_inheritance_relationships(
    extractor: &JavaScriptExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Look for extends_clause or class_heritage nodes
    match node.kind() {
        "extends_clause" | "class_heritage" => {
            if let Some(parent) = node.parent() {
                if parent.kind() == "class_declaration" {
                    // Get the class name from parent
                    if let Some(class_name_node) = parent.child_by_field_name("name") {
                        let class_name = extractor.base().get_node_text(&class_name_node);

                        // Find the class symbol
                        if let Some(class_symbol) = symbols
                            .iter()
                            .find(|s| s.name == class_name && s.kind == SymbolKind::Class)
                        {
                            // Look for identifier or type_identifier children to get superclass name
                            let mut cursor = node.walk();
                            for child in node.children(&mut cursor) {
                                if child.kind() == "identifier" || child.kind() == "type_identifier"
                                {
                                    let superclass_name = extractor.base().get_node_text(&child);

                                    // Find the superclass symbol
                                    if let Some(superclass_symbol) = symbols.iter().find(|s| {
                                        s.name == superclass_name && s.kind == SymbolKind::Class
                                    }) {
                                        let relationship = Relationship {
                                            id: format!(
                                                "{}_{}_{:?}_{}",
                                                class_symbol.id,
                                                superclass_symbol.id,
                                                RelationshipKind::Extends,
                                                child.start_position().row
                                            ),
                                            from_symbol_id: class_symbol.id.clone(),
                                            to_symbol_id: superclass_symbol.id.clone(),
                                            kind: RelationshipKind::Extends,
                                            file_path: extractor.base().file_path.clone(),
                                            line_number: (child.start_position().row + 1) as u32,
                                            confidence: 1.0,
                                            metadata: None,
                                        };
                                        relationships.push(relationship);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_inheritance_relationships(extractor, child, symbols, relationships);
    }
}

/// Helper to find the function that contains a given node
pub(crate) fn find_containing_function<'a>(
    node: Node,
    symbols: &'a [Symbol],
) -> Option<&'a Symbol> {
    let mut current = Some(node);

    while let Some(current_node) = current {
        let position = current_node.start_position();
        let pos_line = (position.row + 1) as u32;

        // Find function symbols that contain this position
        for symbol in symbols {
            if matches!(symbol.kind, SymbolKind::Function)
                && symbol.start_line <= pos_line
                && symbol.end_line >= pos_line
            {
                return Some(symbol);
            }
        }

        current = current_node.parent();
    }

    None
}
