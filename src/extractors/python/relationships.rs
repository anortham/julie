/// Relationship extraction
/// Handles inheritance relationships and function call relationships
use super::super::base::{Relationship, RelationshipKind, Symbol, SymbolKind};
use super::{PythonExtractor, helpers};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract relationships from Python code
pub(crate) fn extract_relationships(
    extractor: &PythonExtractor,
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
    extractor: &PythonExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "class_definition" => {
            extract_class_relationships(extractor, node, symbol_map, relationships);
        }
        "call" => {
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

/// Extract inheritance relationships from a class definition
fn extract_class_relationships(
    extractor: &PythonExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.base();

    // Get class name from the name field
    let name_node = match node.child_by_field_name("name") {
        Some(node) => node,
        None => return,
    };

    let class_name = base.get_node_text(&name_node);
    let class_symbol = match symbol_map.get(&class_name) {
        Some(symbol) => symbol,
        None => return,
    };

    // Extract inheritance relationships
    if let Some(superclasses_node) = node.child_by_field_name("superclasses") {
        let bases = helpers::extract_argument_list(extractor, &superclasses_node);

        for base_name in bases {
            if let Some(base_symbol) = symbol_map.get(&base_name) {
                // Determine relationship kind: implements for interfaces/protocols, extends for classes
                let relationship_kind = if base_symbol.kind == SymbolKind::Interface {
                    RelationshipKind::Implements
                } else {
                    RelationshipKind::Extends
                };

                let relationship = Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        class_symbol.id,
                        base_symbol.id,
                        relationship_kind,
                        node.start_position().row
                    ),
                    from_symbol_id: class_symbol.id.clone(),
                    to_symbol_id: base_symbol.id.clone(),
                    kind: relationship_kind,
                    file_path: base.file_path.clone(),
                    line_number: (node.start_position().row + 1) as u32,
                    confidence: 0.95,
                    metadata: None,
                };

                relationships.push(relationship);
            }
        }
    }
}

/// Extract call relationships from a function call
fn extract_call_relationships(
    extractor: &PythonExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.base();

    // For a call node, extract the function/method being called
    if let Some(function_node) = node.child_by_field_name("function") {
        let called_method_name = extract_method_name_from_call(base, &function_node);

        if !called_method_name.is_empty() {
            if let Some(called_symbol) = symbol_map.get(&called_method_name) {
                // Find the enclosing function/method that contains this call
                if let Some(caller_symbol) = find_containing_function(extractor, node, symbol_map) {
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
                        file_path: base.file_path.clone(),
                        line_number: (node.start_position().row + 1) as u32,
                        confidence: 0.90,
                        metadata: None,
                    };

                    relationships.push(relationship);
                }
            }
        }
    }
}

/// Extract method name from a call node
fn extract_method_name_from_call(
    base: &crate::extractors::base::BaseExtractor,
    function_node: &Node,
) -> String {
    match function_node.kind() {
        "identifier" => {
            // Simple function call: foo()
            base.get_node_text(function_node)
        }
        "attribute" => {
            // Method call: obj.method() or self.db.connect()
            if let Some(attribute_node) = function_node.child_by_field_name("attribute") {
                base.get_node_text(&attribute_node)
            } else {
                String::new()
            }
        }
        _ => String::new(),
    }
}

/// Find the containing function of a node
fn find_containing_function<'a>(
    extractor: &'a PythonExtractor,
    node: Node,
    symbol_map: &HashMap<String, &'a Symbol>,
) -> Option<&'a Symbol> {
    let base = extractor.base();

    // Walk up the tree to find the containing function or method
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "function_definition" || parent.kind() == "async_function_definition" {
            // Found a function, extract its name
            if let Some(name_node) = parent.child_by_field_name("name") {
                let function_name = base.get_node_text(&name_node);
                return symbol_map.get(&function_name).copied();
            }
        }
        current = parent;
    }
    None
}
