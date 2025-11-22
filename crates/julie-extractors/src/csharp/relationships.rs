// C# Relationship Extraction

use crate::base::{PendingRelationship, Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::csharp::CSharpExtractor;
use tree_sitter::Tree;

/// Extract relationships from the tree
pub fn extract_relationships(
    extractor: &mut CSharpExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    visit_relationships(extractor, tree.root_node(), symbols, &mut relationships);
    relationships
}

fn visit_relationships(
    extractor: &mut CSharpExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "class_declaration" | "interface_declaration" | "struct_declaration" => {
            extract_inheritance_relationships(extractor, node, symbols, relationships);
        }
        "invocation_expression" => {
            extract_call_relationships(extractor, node, symbols, relationships);
        }
        // In C#, method calls are represented as member_access_expression followed by argument_list
        "member_access_expression" => {
            // Check if this is followed by an argument_list (i.e., a method call)
            if let Some(sibling) = node.next_sibling() {
                if sibling.kind() == "argument_list" {
                    extract_call_relationships(extractor, node, symbols, relationships);
                }
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_relationships(extractor, child, symbols, relationships);
    }
}

fn extract_inheritance_relationships(
    extractor: &mut CSharpExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.get_base();
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

/// Extract method call relationships
///
/// Creates resolved Relationship when target is a local method.
/// Creates PendingRelationship when target is:
/// - An Import symbol (needs cross-file resolution)
/// - Not found in local symbol_map (e.g., method on imported type)
fn extract_call_relationships(
    extractor: &mut CSharpExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // In C#, method calls can be:
    // 1. Direct identifier call: Method()
    // 2. Member access call: Helper.Process()
    // The node can be either an invocation_expression or a member_access_expression

    let method_name = {
        let base = extractor.get_base();
        match node.kind() {
            "identifier" => base.get_node_text(&node),
            "member_access_expression" => {
                // For something like Helper.Process(), find the method name (last identifier)
                let mut method_cursor = node.walk();
                let children: Vec<_> = node.children(&mut method_cursor).collect();
                children
                    .iter()
                    .rev()
                    .find(|c| c.kind() == "identifier")
                    .map(|n| base.get_node_text(n))
                    .unwrap_or_default()
            }
            _ => {
                // For invocation_expression, get the first child which is the function/method
                let mut cursor = node.walk();
                let children: Vec<_> = node.children(&mut cursor).collect();
                if let Some(first_child) = children.first() {
                    match first_child.kind() {
                        "identifier" => base.get_node_text(first_child),
                        "member_access_expression" => {
                            let mut method_cursor = first_child.walk();
                            let children: Vec<_> = first_child.children(&mut method_cursor).collect();
                            children
                                .iter()
                                .rev()
                                .find(|c| c.kind() == "identifier")
                                .map(|n| base.get_node_text(n))
                                .unwrap_or_default()
                        }
                        _ => String::new(),
                    }
                } else {
                    String::new()
                }
            }
        }
    };

    if !method_name.is_empty() {
        handle_call_target(extractor, node, &method_name, symbols, relationships);
    }
}

/// Handle a call target - create Relationship or PendingRelationship based on target type
fn handle_call_target(
    extractor: &mut CSharpExtractor,
    call_node: tree_sitter::Node,
    callee_name: &str,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.get_base();

    // Build a symbol_map for quick lookup
    let symbol_map: std::collections::HashMap<String, &Symbol> =
        symbols.iter().map(|s| (s.name.clone(), s)).collect();

    // Find the calling method context - look upward in the tree for parent method
    let mut parent = call_node.parent();
    let mut caller_symbol = None;
    while let Some(p) = parent {
        if p.kind() == "method_declaration" || p.kind() == "local_function_statement" {
            // Get the method name
            let mut p_cursor = p.walk();
            if let Some(name_node) = p.children(&mut p_cursor).find(|c| c.kind() == "identifier") {
                let method_name = base.get_node_text(&name_node);
                caller_symbol = symbol_map.get(&method_name).copied();
                break;
            }
        }
        parent = p.parent();
    }

    // No caller context means we can't create a meaningful relationship
    let Some(caller) = caller_symbol else {
        return;
    };

    let line_number = call_node.start_position().row as u32 + 1;
    let file_path = base.file_path.clone();

    // Check if we can resolve the callee locally
    match symbol_map.get(callee_name) {
        Some(called_symbol) if called_symbol.kind == SymbolKind::Import => {
            // Target is an Import symbol - need cross-file resolution
            // Don't create relationship pointing to Import (useless for trace_call_path)
            // Instead, create a PendingRelationship with the callee name
            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: caller.id.clone(),
                callee_name: callee_name.to_string(),
                kind: RelationshipKind::Calls,
                file_path,
                line_number,
                confidence: 0.8, // Lower confidence - needs resolution
            });
        }
        Some(called_symbol) => {
            // Target is a local method - create resolved Relationship
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
            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: caller.id.clone(),
                callee_name: callee_name.to_string(),
                kind: RelationshipKind::Calls,
                file_path,
                line_number,
                confidence: 0.7, // Lower confidence - unknown target
            });
        }
    }
}
