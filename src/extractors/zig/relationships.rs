use crate::extractors::base::{BaseExtractor, Relationship, RelationshipKind, Symbol, SymbolKind};
use tree_sitter::{Node, Tree};

/// Extract relationships between symbols (calls, composition, inheritance)
pub(super) fn extract_relationships(
    base: &mut BaseExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    traverse_for_relationships(base, tree.root_node(), symbols, &mut relationships);
    relationships
}

fn traverse_for_relationships(
    base: &mut BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "struct_declaration" => {
            extract_struct_relationships(base, node, symbols, relationships);
        }
        "const_declaration" => {
            // Check const declarations for struct definitions
            if base
                .find_child_by_type(&node, "struct_declaration")
                .is_some()
            {
                extract_struct_relationships(base, node, symbols, relationships);
            }
        }
        "call_expression" => {
            extract_function_call_relationships(base, node, symbols, relationships);
        }
        _ => {}
    }

    // Recursively traverse children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        traverse_for_relationships(base, child, symbols, relationships);
    }
}

fn extract_struct_relationships(
    base: &mut BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    if node.kind() != "struct_declaration" {
        return;
    }

    // Find a symbol that matches this struct_declaration by position
    let struct_symbol = symbols
        .iter()
        .find(|s| {
            s.kind == SymbolKind::Class
                && s.start_line == (node.start_position().row + 1) as u32
                && s.start_column == node.start_position().column as u32
        })
        .or_else(|| {
            // Try finding by nearby position (within a few lines)
            symbols.iter().find(|s| {
                s.kind == SymbolKind::Class
                    && (s.start_line as i32 - (node.start_position().row + 1) as i32).abs() <= 2
            })
        });

    if let Some(target_symbol) = struct_symbol {
        traverse_struct_fields(base, node, symbols, relationships, target_symbol);
    }
}

fn traverse_struct_fields(
    base: &mut BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
    target_symbol: &Symbol,
) {
    let mut cursor = node.walk();
    for field_node in node.children(&mut cursor) {
        if field_node.kind() == "container_field" {
            if let Some(field_name_node) = base.find_child_by_type(&field_node, "identifier") {
                let _field_name = base.get_node_text(&field_name_node);

                // Look for type information
                let type_node = base
                    .find_child_by_type(&field_node, "type_expression")
                    .or_else(|| base.find_child_by_type(&field_node, "builtin_type"))
                    .or_else(|| base.find_child_by_type(&field_node, "slice_type"))
                    .or_else(|| base.find_child_by_type(&field_node, "pointer_type"))
                    .or_else(|| {
                        // Look for identifier after colon
                        let mut field_cursor = field_node.walk();
                        let field_children: Vec<Node> =
                            field_node.children(&mut field_cursor).collect();
                        let colon_index = field_children.iter().position(|c| c.kind() == ":")?;
                        field_children.get(colon_index + 1).copied()
                    });

                if let Some(type_node) = type_node {
                    let type_name = base.get_node_text(&type_node).trim().to_string();

                    // Look for referenced symbols that are struct types
                    let referenced_symbol = symbols.iter().find(|s| {
                        s.name == type_name
                            && matches!(
                                s.kind,
                                SymbolKind::Class | SymbolKind::Interface | SymbolKind::Struct
                            )
                    });

                    if let Some(referenced_symbol) = referenced_symbol {
                        if referenced_symbol.id != target_symbol.id {
                            // Create composition relationship
                            relationships.push(Relationship {
                                id: format!(
                                    "{}_{}_{:?}_{}",
                                    target_symbol.id,
                                    referenced_symbol.id,
                                    RelationshipKind::Composition,
                                    field_node.start_position().row
                                ),
                                from_symbol_id: target_symbol.id.clone(),
                                to_symbol_id: referenced_symbol.id.clone(),
                                kind: RelationshipKind::Composition,
                                file_path: base.file_path.clone(),
                                line_number: (field_node.start_position().row + 1) as u32,
                                confidence: 0.8,
                                metadata: None,
                            });
                        }
                    }
                }
            }
        }
    }
}

fn extract_function_call_relationships(
    base: &mut BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let mut called_func_name: Option<String> = None;

    // Check for direct function call (identifier + arguments)
    if let Some(func_name_node) = base.find_child_by_type(&node, "identifier") {
        called_func_name = Some(base.get_node_text(&func_name_node));
    } else if let Some(field_expr_node) = base.find_child_by_type(&node, "field_expression") {
        // Check for method call (field_expression + arguments)
        let identifiers = base.find_children_by_type(&field_expr_node, "identifier");
        if identifiers.len() >= 2 {
            called_func_name = Some(base.get_node_text(&identifiers[1]));
            // Second identifier is the method name
        }
    }

    if let Some(called_func_name) = called_func_name {
        let called_symbol = symbols
            .iter()
            .find(|s| s.name == called_func_name && s.kind == SymbolKind::Function);

        if let Some(called_symbol) = called_symbol {
            // Find the calling function
            let mut current = node.parent();
            while let Some(parent) = current {
                if matches!(
                    parent.kind(),
                    "function_declaration" | "function_definition"
                ) {
                    if let Some(caller_name_node) = base.find_child_by_type(&parent, "identifier") {
                        let caller_name = base.get_node_text(&caller_name_node);
                        let caller_symbol = symbols
                            .iter()
                            .find(|s| s.name == caller_name && s.kind == SymbolKind::Function);

                        if let Some(caller_symbol) = caller_symbol {
                            if caller_symbol.id != called_symbol.id {
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
                                    line_number: (node.start_position().row + 1) as u32,
                                    confidence: 0.9,
                                    metadata: None,
                                });
                            }
                        }
                    }
                    break;
                }
                current = parent.parent();
            }
        }
    }
}
