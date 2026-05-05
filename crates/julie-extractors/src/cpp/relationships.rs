//! Relationship extraction for C++
//! Handles inheritance and function call relationships

use crate::base::{
    LocalTargetResolution, Relationship, RelationshipKind, ScopedSymbolIndex, Symbol,
    UnresolvedTarget,
};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

use super::helpers;

/// Extract inheritance and call relationships from C++ code
pub(super) fn extract_relationships(
    extractor: &mut super::CppExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    let scoped_index = ScopedSymbolIndex::new(symbols);
    let symbol_map = crate::base::ScopedSymbolIndex::unique_symbol_map(symbols);

    // Walk the tree looking for relationships
    walk_tree_for_relationships(
        extractor,
        tree.root_node(),
        &symbol_map,
        &scoped_index,
        &mut relationships,
    );

    relationships
}

/// Recursively walk tree looking for inheritance and call relationships
fn walk_tree_for_relationships(
    extractor: &mut super::CppExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    scoped_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "class_specifier" | "struct_specifier" => {
            let inheritance = extract_inheritance_from_class(extractor, node, symbol_map);
            relationships.extend(inheritance);
        }
        "call_expression" | "function_call" => {
            extract_call_relationships(extractor, node, symbol_map, scoped_index, relationships);
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_relationships(extractor, child, symbol_map, scoped_index, relationships);
    }
}

/// Extract inheritance relationships from a single class node
fn extract_inheritance_from_class(
    extractor: &mut super::CppExtractor,
    class_node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    let base = extractor.get_base_mut();

    // Get the class name
    let mut cursor = class_node.walk();
    let name_node = class_node
        .children(&mut cursor)
        .find(|c| c.kind() == "type_identifier");

    let Some(name_node) = name_node else {
        return relationships;
    };

    let class_name = base.get_node_text(&name_node);
    let Some(derived_symbol) = symbol_map.get(&class_name) else {
        return relationships;
    };

    // Look for base class clause
    let base_clause = class_node
        .children(&mut class_node.walk())
        .find(|c| c.kind() == "base_class_clause");

    let Some(base_clause) = base_clause else {
        return relationships;
    };

    // Extract base classes
    let base_classes = helpers::extract_base_classes(base, base_clause);
    for base_class in base_classes {
        // Clean base class name (remove access specifiers)
        let clean_base_name = base_class
            .strip_prefix("public ")
            .or_else(|| base_class.strip_prefix("private "))
            .or_else(|| base_class.strip_prefix("protected "))
            .unwrap_or(&base_class);

        if let Some(base_symbol) = symbol_map.get(clean_base_name) {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    derived_symbol.id,
                    base_symbol.id,
                    RelationshipKind::Extends,
                    class_node.start_position().row
                ),
                from_symbol_id: derived_symbol.id.clone(),
                to_symbol_id: base_symbol.id.clone(),
                kind: RelationshipKind::Extends,
                file_path: base.file_path.clone(),
                line_number: (class_node.start_position().row + 1) as u32,
                confidence: 1.0,
                metadata: None,
            });
        }
    }

    relationships
}

/// Extract function call relationships from C++ code
///
/// Creates resolved Relationship when target is a local function.
/// Creates PendingRelationship when target is:
/// - Not found in local symbol_map (e.g., function from included header)
fn extract_call_relationships(
    extractor: &mut super::CppExtractor,
    call_node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    scoped_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.get_base_mut();

    // Get the function name being called
    // For C++, call_expression has a "function" field which is the called entity
    if let Some(func_node) = call_node.child_by_field_name("function") {
        // Extract the unresolved call target from the function node.
        let target = match func_node.kind() {
            // Direct function call: helper() or std::vector::push_back()
            "identifier" => UnresolvedTarget::simple(base.get_node_text(&func_node)),
            // Method call: obj.method() or ptr->method()
            "field_expression" | "pointer_expression" => {
                // Get the rightmost identifier (the method name)
                // For field_expression: obj.method
                // For pointer_expression: ptr->method
                if let Some(field_node) = func_node.child_by_field_name("field") {
                    let terminal_name = base.get_node_text(&field_node);
                    let expression_text = base.get_node_text(&func_node);
                    let receiver = expression_text
                        .rsplit_once("->")
                        .or_else(|| expression_text.rsplit_once('.'))
                        .map(|(left, _)| left.trim().to_string())
                        .filter(|left| !left.is_empty());

                    if let Some(receiver) = receiver {
                        UnresolvedTarget {
                            display_name: expression_text,
                            terminal_name,
                            receiver: Some(receiver),
                            namespace_path: Vec::new(),
                            import_context: None,
                        }
                    } else {
                        UnresolvedTarget::simple(terminal_name)
                    }
                } else {
                    return; // Can't extract field name
                }
            }
            // Template calls like std::vector<int>()
            "template_function" => {
                // Try to get the function name from the template
                let mut name = String::new();
                let mut cursor = func_node.walk();
                for child in func_node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        name = base.get_node_text(&child);
                        break;
                    }
                }
                UnresolvedTarget::simple(name)
            }
            // For other cases, try to extract any identifier in the function node
            _ => {
                // Try to find an identifier child
                let mut name = String::new();
                let mut cursor = func_node.walk();
                for child in func_node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        name = base.get_node_text(&child);
                        break;
                    }
                }
                if name.is_empty() {
                    return; // Can't extract name
                }
                UnresolvedTarget::simple(name)
            }
        };

        if !target.terminal_name.is_empty() {
            handle_call_target(
                extractor,
                call_node,
                target,
                symbol_map,
                scoped_index,
                relationships,
            );
        }
    }
}

/// Handle a call target - create Relationship or PendingRelationship based on target type
fn handle_call_target(
    extractor: &mut super::CppExtractor,
    call_node: Node,
    target: UnresolvedTarget,
    symbol_map: &HashMap<String, &Symbol>,
    scoped_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    // Find the containing function for this call
    let containing_function_name = find_containing_function_name(extractor, call_node);

    let Some(caller_name) = containing_function_name else {
        return;
    };

    // Look up the caller in the symbol map to get its ID
    let Some(caller_symbol) = symbol_map.get(&caller_name) else {
        return;
    };

    let caller_id = caller_symbol.id.clone();
    let file_path = extractor.get_base_mut().file_path.clone();

    // Check if we can resolve the callee locally
    match scoped_index.resolve_call_target(
        &target.terminal_name,
        Some(caller_symbol),
        target.receiver.as_deref(),
    ) {
        LocalTargetResolution::Resolved(called_symbol) => {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    caller_id,
                    called_symbol.id,
                    RelationshipKind::Calls,
                    call_node.start_position().row
                ),
                from_symbol_id: caller_id,
                to_symbol_id: called_symbol.id.clone(),
                kind: RelationshipKind::Calls,
                file_path,
                line_number: call_node.start_position().row as u32 + 1,
                confidence: 0.9,
                metadata: None,
            });
        }
        LocalTargetResolution::Import(_)
        | LocalTargetResolution::Ambiguous
        | LocalTargetResolution::Missing
        | LocalTargetResolution::ReceiverQualified => {
            // Target not found/ambiguous in local symbols - keep unresolved for
            // cross-file resolution.
            let pending = extractor.get_base_mut().create_pending_relationship(
                caller_id.clone(),
                target,
                RelationshipKind::Calls,
                &call_node,
                Some(caller_id),
                Some(0.7),
            );
            extractor.add_structured_pending_relationship(pending);
        }
    }
}

/// Find the containing function for a given call node by traversing up the tree
fn find_containing_function_name(
    extractor: &mut super::CppExtractor,
    mut node: Node,
) -> Option<String> {
    let base = extractor.get_base_mut();

    while let Some(parent) = node.parent() {
        match parent.kind() {
            "function_definition" | "function_declarator" => {
                // Extract the function name from the declarator or the function itself
                // For function_definition, the name is in a declarator child
                // For function_declarator, the name is the first identifier
                let mut cursor = parent.walk();
                for child in parent.children(&mut cursor) {
                    if child.kind() == "declarator"
                        || child.kind() == "pointer_declarator"
                        || child.kind() == "reference_declarator"
                        || child.kind() == "function_declarator"
                    {
                        // Find the identifier in the declarator
                        let mut decl_cursor = child.walk();
                        for decl_child in child.children(&mut decl_cursor) {
                            if decl_child.kind() == "identifier" {
                                let name = base.get_node_text(&decl_child);
                                return Some(name);
                            }
                        }
                    } else if child.kind() == "identifier" {
                        return Some(base.get_node_text(&child));
                    }
                }
                return None; // Found a function node but couldn't extract name
            }
            "declaration" => {
                // Top-level declaration with function_declarator child
                // Check if this is a function declaration
                let mut cursor = parent.walk();
                for child in parent.children(&mut cursor) {
                    if child.kind() == "function_declarator" || child.kind() == "declarator" {
                        // This might be a function, try to extract the name
                        let mut decl_cursor = child.walk();
                        for decl_child in child.children(&mut decl_cursor) {
                            if decl_child.kind() == "identifier" {
                                return Some(base.get_node_text(&decl_child));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        node = parent;
    }
    None
}
