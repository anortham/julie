//! Relationship extraction for GDScript
//! Handles function call relationships (including cross-file pending relationships)

use super::super::base::{Relationship, RelationshipKind, Symbol, UnresolvedTarget};
use super::GDScriptExtractor;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract relationships from GDScript code
pub(super) fn extract_relationships(
    extractor: &mut GDScriptExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();

    // Create symbol map for fast lookups by name
    let symbol_map: HashMap<String, &Symbol> =
        symbols.iter().map(|s| (s.name.clone(), s)).collect();

    // Recursively visit all nodes to extract relationships
    visit_node_for_relationships(extractor, tree.root_node(), &symbol_map, &mut relationships);

    relationships
}

/// Visit a node and extract relationships from it
fn visit_node_for_relationships(
    extractor: &mut GDScriptExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "call" | "call_expression" => {
            extract_call_relationships(extractor, node, symbol_map, relationships);
        }
        _ => {}
    }

    // Recursively visit all children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_node_for_relationships(extractor, child, symbol_map, relationships);
    }
}

/// Extract call relationships from a function call
fn extract_call_relationships(
    extractor: &mut GDScriptExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
) {
    let base = &extractor.base;

    // For GDScript, a call node has the function name as the first child
    // The structure is: call -> (identifier | attribute) + arguments
    let target = extract_target_from_call(base, &node);
    let called_function_name = target.terminal_name.clone();

    if !called_function_name.is_empty() {
        // Find the enclosing function/method that contains this call
        // CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
        let file_symbols: Vec<Symbol> = symbol_map
            .values()
            .filter(|s| s.file_path == base.file_path)
            .map(|&s| s.clone())
            .collect();

        if let Some(caller_symbol) = base.find_containing_symbol(&node, &file_symbols) {
            let line_number = (node.start_position().row + 1) as u32;
            let file_path = base.file_path.clone();

            // Check if we can resolve the callee locally
            match symbol_map.get(&called_function_name) {
                Some(called_symbol) => {
                    // Target is a local function/method - create resolved Relationship
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
                        file_path,
                        line_number,
                        confidence: 0.9,
                        metadata: None,
                    };

                    relationships.push(relationship);
                }
                None => {
                    // Target not found in local symbols - likely a method on imported type
                    // or a call to an external function
                    // Create PendingRelationship for cross-file resolution
                    let pending = base.create_pending_relationship(
                        caller_symbol.id.clone(),
                        target,
                        RelationshipKind::Calls,
                        &node,
                        Some(caller_symbol.id.clone()),
                        Some(0.7),
                    );
                    extractor.add_structured_pending_relationship(pending);
                }
            }
        }
    }
}

/// Extract unresolved target from a call node
fn extract_target_from_call(base: &crate::base::BaseExtractor, node: &Node) -> UnresolvedTarget {
    // For GDScript, we need to get the function name from the call structure
    // call -> identifier (for simple calls like func_name())
    // call -> attribute (for method calls like obj.method() or self.method())

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                // Simple function call: func_name()
                return UnresolvedTarget::simple(base.get_node_text(&child));
            }
            "attribute" => {
                // Method call: obj.method() or self.method()
                // For an attribute node, the rightmost identifier is the member being accessed
                let mut attr_cursor = child.walk();
                let attr_children: Vec<Node> = child.children(&mut attr_cursor).collect();

                // The last identifier in the attribute is the method name
                if let Some(last_child) = attr_children.last() {
                    if last_child.kind() == "identifier" {
                        let terminal_name = base.get_node_text(last_child);
                        let attr_text = base.get_node_text(&child);
                        if let Some((receiver, _)) = attr_text.rsplit_once('.') {
                            let receiver = receiver.to_string();
                            return UnresolvedTarget {
                                display_name: attr_text,
                                terminal_name,
                                receiver: Some(receiver),
                                namespace_path: Vec::new(),
                                import_context: None,
                            };
                        }
                        return UnresolvedTarget::simple(terminal_name);
                    }
                }

                // Fallback: try to extract from attribute text
                let attr_text = base.get_node_text(&child);
                if let Some(last_dot) = attr_text.rfind('.') {
                    let terminal_name = attr_text[last_dot + 1..].to_string();
                    return UnresolvedTarget {
                        display_name: attr_text.clone(),
                        terminal_name,
                        receiver: Some(attr_text[..last_dot].to_string()),
                        namespace_path: Vec::new(),
                        import_context: None,
                    };
                }
                return UnresolvedTarget::simple(attr_text);
            }
            _ => {}
        }
    }

    UnresolvedTarget::simple(String::new())
}
