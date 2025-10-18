use super::helpers::find_containing_function;
/// Rust relationship extraction
/// - Trait implementations
/// - Type references in fields
/// - Function calls
use crate::extractors::base::{Relationship, RelationshipKind, Symbol};
use crate::extractors::rust::RustExtractor;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all relationships between Rust symbols
pub(super) fn extract_relationships(
    extractor: &mut RustExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    let symbol_map: HashMap<String, &Symbol> =
        symbols.iter().map(|s| (s.name.clone(), s)).collect();

    walk_tree_for_relationships(extractor, tree.root_node(), &symbol_map, &mut relationships);
    relationships
}

fn walk_tree_for_relationships(
    extractor: &mut RustExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "impl_item" => {
            extract_impl_relationships(extractor, node, symbol_map, relationships);
        }
        "struct_item" | "enum_item" => {
            extract_type_relationships(extractor, node, symbol_map, relationships);
        }
        "call_expression" => {
            extract_call_relationships(extractor, node, symbol_map, relationships);
        }
        _ => {}
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_relationships(extractor, child, symbol_map, relationships);
    }
}

/// Extract trait implementation relationships
fn extract_impl_relationships(
    extractor: &mut RustExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.get_base_mut();
    // Look for "impl TraitName for TypeName" pattern
    let children: Vec<_> = node.children(&mut node.walk()).collect();
    let mut trait_name = String::new();
    let mut type_name = String::new();
    let mut found_for = false;

    for child in children {
        if child.kind() == "type_identifier" {
            if !found_for {
                trait_name = base.get_node_text(&child);
            } else {
                type_name = base.get_node_text(&child);
                break;
            }
        } else if child.kind() == "for" {
            found_for = true;
        }
    }

    // If we found both trait and type, create implements relationship
    if !trait_name.is_empty() && !type_name.is_empty() {
        if let (Some(trait_symbol), Some(type_symbol)) =
            (symbol_map.get(&trait_name), symbol_map.get(&type_name))
        {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    type_symbol.id,
                    trait_symbol.id,
                    RelationshipKind::Implements,
                    node.start_position().row
                ),
                from_symbol_id: type_symbol.id.clone(),
                to_symbol_id: trait_symbol.id.clone(),
                kind: RelationshipKind::Implements,
                file_path: base.file_path.clone(),
                line_number: node.start_position().row as u32 + 1,
                confidence: 0.95,
                metadata: None,
            });
        }
    }
}

/// Extract type references in struct/enum fields
fn extract_type_relationships(
    extractor: &mut RustExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.get_base_mut();
    let name_node = node.child_by_field_name("name");
    if let Some(name_node) = name_node {
        let type_name = base.get_node_text(&name_node);
        if let Some(type_symbol) = symbol_map.get(&type_name) {
            // Look for field types that reference other symbols
            let declaration_list = node
                .children(&mut node.walk())
                .find(|c| c.kind() == "field_declaration_list" || c.kind() == "enum_variant_list");

            if let Some(decl_list) = declaration_list {
                for field in decl_list.children(&mut decl_list.walk()) {
                    if field.kind() == "field_declaration" || field.kind() == "enum_variant" {
                        extract_field_type_references(
                            extractor,
                            field,
                            type_symbol,
                            symbol_map,
                            relationships,
                        );
                    }
                }
            }
        }
    }
}

/// Extract type references within a field
fn extract_field_type_references(
    extractor: &mut RustExtractor,
    field_node: Node,
    container_symbol: &Symbol,
    symbol_map: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.get_base_mut();
    // Find type references in field declarations
    for child in field_node.children(&mut field_node.walk()) {
        if child.kind() == "type_identifier" {
            let referenced_type_name = base.get_node_text(&child);
            if let Some(referenced_symbol) = symbol_map.get(&referenced_type_name) {
                if referenced_symbol.id != container_symbol.id {
                    relationships.push(Relationship {
                        id: format!(
                            "{}_{}_{:?}_{}",
                            container_symbol.id,
                            referenced_symbol.id,
                            RelationshipKind::Uses,
                            field_node.start_position().row
                        ),
                        from_symbol_id: container_symbol.id.clone(),
                        to_symbol_id: referenced_symbol.id.clone(),
                        kind: RelationshipKind::Uses,
                        file_path: base.file_path.clone(),
                        line_number: field_node.start_position().row as u32 + 1,
                        confidence: 0.8,
                        metadata: None,
                    });
                }
            }
        }
    }
}

/// Extract function call relationships
fn extract_call_relationships(
    extractor: &mut RustExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.get_base_mut();
    // Extract function/method call relationships
    let function_node = node.child_by_field_name("function");
    if let Some(func_node) = function_node {
        // Handle method calls (receiver.method())
        if func_node.kind() == "field_expression" {
            let method_node = func_node.child_by_field_name("field");
            if let Some(method_node) = method_node {
                let method_name = base.get_node_text(&method_node);
                if let Some(called_symbol) = symbol_map.get(&method_name) {
                    // Find the calling function context
                    if let Some(calling_function) = find_containing_function(base, node) {
                        if let Some(caller_symbol) = symbol_map.get(&calling_function) {
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
            }
        }
        // Handle direct function calls
        else if func_node.kind() == "identifier" {
            let function_name = base.get_node_text(&func_node);
            if let Some(called_symbol) = symbol_map.get(&function_name) {
                if let Some(calling_function) = find_containing_function(base, node) {
                    if let Some(caller_symbol) = symbol_map.get(&calling_function) {
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
        }
    }
}
