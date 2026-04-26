use super::helpers::{extract_impl_target_names, find_containing_function};
/// Rust relationship extraction
/// - Trait implementations
/// - Type references in fields
/// - Function calls
use crate::base::{Relationship, RelationshipKind, Symbol, SymbolKind, UnresolvedTarget};
use crate::rust::RustExtractor;
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
    let targets = extract_impl_target_names(base, node);

    if let (Some(trait_name), Some(type_name)) = (targets.trait_name, targets.type_name) {
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
///
/// Creates resolved Relationship when target is a local function/method.
/// Creates PendingRelationship when target is:
/// - An Import symbol (needs cross-file resolution)
/// - Not found in local symbol_map (e.g., method on imported type)
fn extract_call_relationships(
    extractor: &mut RustExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
) {
    let function_node = node.child_by_field_name("function");
    if let Some(func_node) = function_node {
        // Handle method calls (receiver.method())
        if func_node.kind() == "field_expression" {
            let method_node = func_node.child_by_field_name("field");
            if let Some(method_node) = method_node {
                let method_name = extractor.get_base_mut().get_node_text(&method_node);
                let target = if let Some(receiver_node) = func_node.child_by_field_name("value") {
                    let receiver = extractor.get_base_mut().get_node_text(&receiver_node);
                    UnresolvedTarget {
                        display_name: format!("{receiver}.{method_name}"),
                        terminal_name: method_name.clone(),
                        receiver: Some(receiver),
                        namespace_path: Vec::new(),
                        import_context: None,
                    }
                } else {
                    UnresolvedTarget::simple(method_name.clone())
                };
                handle_call_target(
                    extractor,
                    node,
                    &method_name,
                    target,
                    symbol_map,
                    relationships,
                );
            }
        }
        // Handle direct function calls
        else if func_node.kind() == "identifier" {
            let function_name = extractor.get_base_mut().get_node_text(&func_node);
            handle_call_target(
                extractor,
                node,
                &function_name,
                UnresolvedTarget::simple(function_name.clone()),
                symbol_map,
                relationships,
            );
        }
        // Handle qualified/scoped calls: crate::module::function()
        else if func_node.kind() == "scoped_identifier" {
            if let Some(target) = scoped_identifier_to_unresolved_target(extractor, func_node) {
                let function_name = target.terminal_name.clone();
                handle_call_target(
                    extractor,
                    node,
                    &function_name,
                    target,
                    symbol_map,
                    relationships,
                );
            }
        }
    }
}

fn scoped_identifier_to_unresolved_target(
    extractor: &mut RustExtractor,
    scoped_identifier: Node,
) -> Option<UnresolvedTarget> {
    let display_name = extractor.get_base_mut().get_node_text(&scoped_identifier);
    let segments: Vec<String> = display_name
        .split("::")
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    let terminal_name = segments.last()?.clone();
    let namespace_path = segments[..segments.len().saturating_sub(1)].to_vec();

    Some(UnresolvedTarget {
        display_name,
        terminal_name,
        receiver: None,
        namespace_path,
        import_context: None,
    })
}

/// Handle a call target - create Relationship or PendingRelationship based on target type
fn handle_call_target(
    extractor: &mut RustExtractor,
    call_node: Node,
    callee_name: &str,
    unresolved_target: UnresolvedTarget,
    symbol_map: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
) {
    // Find the calling function context
    let calling_function = find_containing_function(extractor.get_base_mut(), call_node);
    let caller_symbol = calling_function
        .as_ref()
        .and_then(|name| symbol_map.get(name));

    // No caller context means we can't create a meaningful relationship
    let Some(caller) = caller_symbol else {
        return;
    };

    let line_number = call_node.start_position().row as u32 + 1;
    let file_path = extractor.get_base_mut().file_path.clone();

    if !unresolved_target.namespace_path.is_empty() {
        let pending = extractor.get_base_mut().create_pending_relationship(
            caller.id.clone(),
            unresolved_target,
            RelationshipKind::Calls,
            &call_node,
            Some(caller.id.clone()),
            Some(0.7),
        );
        extractor.add_structured_pending_relationship(pending);
        return;
    }

    // Check if we can resolve the callee locally
    match symbol_map.get(callee_name) {
        Some(called_symbol) if called_symbol.kind == SymbolKind::Import => {
            // Target is an Import symbol - need cross-file resolution
            // Don't create relationship pointing to Import (useless for trace_call_path)
            // Instead, create a PendingRelationship with the callee name
            let pending = extractor.get_base_mut().create_pending_relationship(
                caller.id.clone(),
                unresolved_target,
                RelationshipKind::Calls,
                &call_node,
                Some(caller.id.clone()),
                Some(0.8),
            );
            extractor.add_structured_pending_relationship(pending);
        }
        Some(called_symbol) => {
            // Target is a local function/method - create resolved Relationship
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    caller.id,
                    called_symbol.id,
                    RelationshipKind::Calls,
                    call_node.start_position().row
                ),
                from_symbol_id: caller.id.clone(),
                to_symbol_id: called_symbol.id.clone(),
                kind: RelationshipKind::Calls,
                file_path,
                line_number,
                confidence: 0.9,
                metadata: None,
            });
        }
        None => {
            // Target not found in local symbols - likely a method on imported type
            // Create PendingRelationship for cross-file resolution
            let pending = extractor.get_base_mut().create_pending_relationship(
                caller.id.clone(),
                unresolved_target,
                RelationshipKind::Calls,
                &call_node,
                Some(caller.id.clone()),
                Some(0.7),
            );
            extractor.add_structured_pending_relationship(pending);
        }
    }
}
