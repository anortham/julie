use super::helpers::extract_impl_target_names;
/// Rust relationship extraction
/// - Trait implementations
/// - Type references in fields
/// - Function calls
use crate::base::{
    LocalTargetResolution, Relationship, RelationshipKind, ScopedSymbolIndex, Symbol, SymbolKind,
    UnresolvedTarget,
};
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
        crate::base::ScopedSymbolIndex::unique_symbol_map(symbols);
    let symbol_index = ScopedSymbolIndex::new(symbols);

    walk_tree_for_relationships(
        extractor,
        tree.root_node(),
        &symbol_map,
        symbols,
        &symbol_index,
        &mut relationships,
    );
    relationships
}

fn walk_tree_for_relationships(
    extractor: &mut RustExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    symbols: &[Symbol],
    symbol_index: &ScopedSymbolIndex<'_>,
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
            extract_call_relationships(extractor, node, symbols, symbol_index, relationships);
        }
        "use_declaration" => {
            extract_use_import_relationship(extractor, node, symbols);
        }
        _ => {}
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_relationships(
            extractor,
            child,
            symbol_map,
            symbols,
            symbol_index,
            relationships,
        );
    }
}

fn extract_use_import_relationship(extractor: &mut RustExtractor, node: Node, symbols: &[Symbol]) {
    let import_symbol = {
        let base = extractor.get_base_mut();
        base.find_containing_symbol(&node, symbols)
            .filter(|symbol| symbol.kind == SymbolKind::Import)
            .cloned()
    };
    let Some(import_symbol) = import_symbol else {
        return;
    };

    let use_text = import_symbol
        .signature
        .clone()
        .unwrap_or_else(|| extractor.get_base_mut().get_node_text(&node));
    let Some(target) = unresolved_import_target_from_use_text(&use_text) else {
        return;
    };

    let pending = extractor.get_base_mut().create_pending_relationship(
        import_symbol.id.clone(),
        target,
        RelationshipKind::Imports,
        &node,
        Some(import_symbol.id.clone()),
        Some(1.0),
    );
    extractor.add_structured_pending_relationship(pending);
}

fn unresolved_import_target_from_use_text(use_text: &str) -> Option<UnresolvedTarget> {
    let normalized_path = normalize_use_path(use_text)?;
    let segments: Vec<String> = normalized_path
        .split("::")
        .map(str::trim)
        .filter(|segment| !segment.is_empty() && *segment != "*")
        .map(ToOwned::to_owned)
        .collect();

    if segments.is_empty() {
        return Some(UnresolvedTarget {
            display_name: normalized_path.clone(),
            terminal_name: normalized_path,
            receiver: None,
            namespace_path: Vec::new(),
            import_context: Some(use_text.trim().to_string()),
        });
    }

    let terminal_name = segments.last().cloned()?;
    let namespace_path = segments[..segments.len().saturating_sub(1)].to_vec();

    Some(UnresolvedTarget {
        display_name: normalized_path,
        terminal_name,
        receiver: None,
        namespace_path,
        import_context: Some(use_text.trim().to_string()),
    })
}

fn normalize_use_path(use_text: &str) -> Option<String> {
    let path_text = use_text
        .trim()
        .trim_start_matches("pub(crate) use ")
        .trim_start_matches("pub(super) use ")
        .trim_start_matches("pub use ")
        .trim_start_matches("use ")
        .trim_end_matches(';')
        .trim();
    if path_text.is_empty() {
        return None;
    }

    let without_alias = path_text.split(" as ").next().unwrap_or(path_text).trim();
    let normalized = if without_alias.contains('{') {
        without_alias
            .split("::{")
            .next()
            .unwrap_or(without_alias)
            .trim()
    } else if without_alias.ends_with("::*") {
        without_alias.trim_end_matches("::*").trim()
    } else {
        without_alias
    };

    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
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
    symbols: &[Symbol],
    symbol_index: &ScopedSymbolIndex<'_>,
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
                    symbols,
                    symbol_index,
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
                symbols,
                symbol_index,
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
                    symbols,
                    symbol_index,
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

fn add_structured_pending_call(
    extractor: &mut RustExtractor,
    caller: &Symbol,
    call_node: Node,
    unresolved_target: UnresolvedTarget,
    confidence: f32,
) {
    let mut pending = extractor.get_base_mut().create_pending_relationship(
        caller.id.clone(),
        unresolved_target,
        RelationshipKind::Calls,
        &call_node,
        Some(caller.id.clone()),
        Some(confidence),
    );
    pending.pending.callee_name = pending.target.terminal_name.clone();
    extractor.add_structured_pending_relationship(pending);
}

/// Handle a call target and decide whether it can be resolved inside this file.
fn handle_call_target(
    extractor: &mut RustExtractor,
    call_node: Node,
    callee_name: &str,
    unresolved_target: UnresolvedTarget,
    symbols: &[Symbol],
    symbol_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    let caller = extractor
        .get_base_mut()
        .find_containing_symbol(&call_node, symbols)
        .cloned();
    let Some(caller) = caller else {
        return;
    };

    let line_number = call_node.start_position().row as u32 + 1;
    let file_path = extractor.get_base_mut().file_path.clone();

    if !unresolved_target.namespace_path.is_empty() {
        add_structured_pending_call(extractor, &caller, call_node, unresolved_target, 0.7);
        return;
    }

    match symbol_index.resolve_call_target(
        callee_name,
        Some(&caller),
        unresolved_target.receiver.as_deref(),
    ) {
        LocalTargetResolution::Import(_) => {
            add_structured_pending_call(extractor, &caller, call_node, unresolved_target, 0.8);
        }
        LocalTargetResolution::ReceiverQualified => {
            add_structured_pending_call(extractor, &caller, call_node, unresolved_target, 0.7);
        }
        LocalTargetResolution::Resolved(called_symbol) => {
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
        LocalTargetResolution::Ambiguous | LocalTargetResolution::Missing => {
            add_structured_pending_call(extractor, &caller, call_node, unresolved_target, 0.7);
        }
    }
}
