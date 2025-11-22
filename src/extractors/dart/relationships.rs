// Dart Extractor - Relationships Extraction
//
// Methods for extracting relationships between symbols (inheritance, uses, etc.)

use super::helpers::*;
use crate::extractors::base::{BaseExtractor, Relationship, RelationshipKind, Symbol, SymbolKind};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract relationships from the tree
pub(super) fn extract_relationships(
    base: &mut BaseExtractor,
    node: Node,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    let symbol_map: HashMap<String, &Symbol> =
        symbols.iter().map(|s| (s.name.clone(), s)).collect();

    traverse_tree(node, &mut |current_node| match current_node.kind() {
        "class_definition" => {
            extract_class_relationships(base, &current_node, symbols, &mut relationships);
        }
        "member_access" | "assignable_expression" => {
            extract_method_call_relationships(base, &current_node, symbols, &symbol_map, &mut relationships);
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
    base: &mut BaseExtractor,
    node: &Node,
    symbols: &[Symbol],
    symbol_map: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
) {
    // Check if this is actually a function call (has argument_part)
    let is_call = if let Some(selector_node) = find_child_by_type(node, "selector") {
        find_child_by_type(&selector_node, "argument_part").is_some()
    } else {
        false
    };

    // Only process if this is a function call
    if !is_call {
        return;
    }

    // Extract the function/method name being called
    let function_name = if let Some(object_node) = node.child_by_field_name("object") {
        // For object.method(), get just "method"
        if let Some(selector_node) = node.child_by_field_name("selector") {
            if let Some(id_node) = find_child_by_type(&selector_node, "identifier") {
                get_node_text(&id_node)
            } else {
                get_node_text(&selector_node)
            }
        } else {
            get_node_text(&object_node)
        }
    } else if let Some(selector_node) = node.child_by_field_name("selector") {
        if let Some(id_node) = find_child_by_type(&selector_node, "identifier") {
            get_node_text(&id_node)
        } else {
            get_node_text(&selector_node)
        }
    } else {
        return;
    };

    // Find the called function in our symbols
    if let Some(called_symbol) = symbol_map.get(function_name.as_str()) {
        // Find the containing function that's making this call
        if let Some(caller_symbol) = find_containing_function(base, node, symbols) {
            // Create a Relationship for this call
            relationships.push(Relationship {
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
                file_path: base.file_path.clone(),
                line_number: node.start_position().row as u32 + 1,
                confidence: 0.9,
                metadata: None,
            });
        }
    }
}

/// Find the containing function for a node by walking up the tree
fn find_containing_function<'a>(
    base: &BaseExtractor,
    node: &Node,
    symbols: &'a [Symbol],
) -> Option<&'a Symbol> {
    let mut current = node.parent();

    while let Some(current_node) = current {
        // Check for function or method declarations
        if current_node.kind() == "function_declaration"
            || current_node.kind() == "method_signature"
            || current_node.kind() == "function_signature"
        {
            // Get the function name
            if let Some(name_node) = find_child_by_type(&current_node, "identifier") {
                let func_name = get_node_text(&name_node);
                // Find this function in symbols, but only from the current file
                if let Some(symbol) = symbols.iter().find(|s| {
                    s.name == func_name
                        && s.file_path == base.file_path
                        && matches!(s.kind, SymbolKind::Function | SymbolKind::Method)
                }) {
                    return Some(symbol);
                }
            }
        }

        current = current_node.parent();
    }

    None
}
