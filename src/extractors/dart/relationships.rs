// Dart Extractor - Relationships Extraction
//
// Methods for extracting relationships between symbols (inheritance, uses, etc.)

use super::helpers::*;
use crate::extractors::base::{BaseExtractor, Relationship, RelationshipKind, Symbol, SymbolKind};
use tree_sitter::Node;

/// Extract relationships from the tree
pub(super) fn extract_relationships(
    base: &mut BaseExtractor,
    node: Node,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();

    traverse_tree(node, &mut |current_node| match current_node.kind() {
        "class_definition" => {
            extract_class_relationships(base, &current_node, symbols, &mut relationships);
        }
        "method_invocation" => {
            extract_method_call_relationships(base, &current_node, symbols, &mut relationships);
        }
        _ => {}
    });

    relationships
}

fn extract_class_relationships(
    base: &mut BaseExtractor,
    node: &Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let class_name = find_child_by_type(node, "identifier");
    if class_name.is_none() {
        return;
    }

    let class_symbol = symbols
        .iter()
        .find(|s| s.name == get_node_text(&class_name.unwrap()) && s.kind == SymbolKind::Class);
    if class_symbol.is_none() {
        return;
    }
    let class_symbol = class_symbol.unwrap();

    // Extract inheritance relationships
    if let Some(extends_clause) = find_child_by_type(node, "superclass") {
        // Extract the class name from the superclass node
        if let Some(type_node) = find_child_by_type(&extends_clause, "type_identifier") {
            let superclass_name = get_node_text(&type_node);
            if let Some(superclass_symbol) = symbols
                .iter()
                .find(|s| s.name == superclass_name && s.kind == SymbolKind::Class)
            {
                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        class_symbol.id,
                        superclass_symbol.id,
                        RelationshipKind::Extends,
                        node.start_position().row
                    ),
                    from_symbol_id: class_symbol.id.clone(),
                    to_symbol_id: superclass_symbol.id.clone(),
                    kind: RelationshipKind::Extends,
                    file_path: base.file_path.clone(),
                    line_number: node.start_position().row as u32 + 1,
                    confidence: 1.0,
                    metadata: None,
                });
            }

            // Also check for relationships with classes mentioned in generic type arguments
            if let Some(type_args_node) = type_node.next_sibling() {
                if type_args_node.kind() == "type_arguments" {
                    // Look for type_identifier nodes within the type arguments
                    let mut generic_types = Vec::new();
                    traverse_tree(type_args_node, &mut |arg_node| {
                        if arg_node.kind() == "type_identifier" {
                            generic_types.push(get_node_text(&arg_node));
                        }
                    });

                    // Create relationships for any generic types that are classes in our symbols
                    for generic_type_name in generic_types {
                        if let Some(generic_type_symbol) = symbols
                            .iter()
                            .find(|s| s.name == generic_type_name && s.kind == SymbolKind::Class)
                        {
                            relationships.push(Relationship {
                                id: format!(
                                    "{}_{}_{:?}_{}",
                                    class_symbol.id,
                                    generic_type_symbol.id,
                                    RelationshipKind::Uses,
                                    node.start_position().row
                                ),
                                from_symbol_id: class_symbol.id.clone(),
                                to_symbol_id: generic_type_symbol.id.clone(),
                                kind: RelationshipKind::Uses,
                                file_path: base.file_path.clone(),
                                line_number: node.start_position().row as u32 + 1,
                                confidence: 1.0,
                                metadata: None,
                            });
                        }
                    }
                }
            }

            // Extract mixin relationships (with clause)
            if let Some(mixin_clause) = find_child_by_type(&extends_clause, "mixins") {
                // Look for type_identifier nodes within the mixins clause
                let mut mixin_types = Vec::new();
                traverse_tree(mixin_clause, &mut |mixin_node| {
                    if mixin_node.kind() == "type_identifier" {
                        mixin_types.push(get_node_text(&mixin_node));
                    }
                });

                // Create 'uses' relationships for any mixin types that are interfaces in our symbols
                // Note: Using 'Uses' instead of 'with' since 'with' is not in RelationshipKind enum
                for mixin_type_name in mixin_types {
                    if let Some(mixin_type_symbol) = symbols
                        .iter()
                        .find(|s| s.name == mixin_type_name && s.kind == SymbolKind::Interface)
                    {
                        relationships.push(Relationship {
                            id: format!(
                                "{}_{}_{:?}_{}",
                                class_symbol.id,
                                mixin_type_symbol.id,
                                RelationshipKind::Uses,
                                node.start_position().row
                            ),
                            from_symbol_id: class_symbol.id.clone(),
                            to_symbol_id: mixin_type_symbol.id.clone(),
                            kind: RelationshipKind::Uses,
                            file_path: base.file_path.clone(),
                            line_number: node.start_position().row as u32 + 1,
                            confidence: 1.0,
                            metadata: None,
                        });
                    }
                }
            }
        }
    }
}

fn extract_method_call_relationships(
    _base: &mut BaseExtractor,
    _node: &Node,
    _symbols: &[Symbol],
    _relationships: &mut Vec<Relationship>,
) {
    // Extract method call relationships for cross-method dependencies
    // This could be expanded for more detailed call graph analysis
}
