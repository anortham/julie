use super::helpers::{extract_method_name_from_call, extract_name_from_node};
/// Relationship extraction for Ruby symbols
/// Handles inheritance, module inclusion, and other symbol relationships
use crate::extractors::base::{BaseExtractor, Relationship, RelationshipKind, Symbol};
use tree_sitter::Node;

/// Extract all relationships from a tree
pub(super) fn extract_relationships(
    base: &BaseExtractor,
    tree: &tree_sitter::Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    extract_relationships_from_node(base, tree.root_node(), symbols, &mut relationships);
    relationships
}

/// Recursively extract relationships from a node
fn extract_relationships_from_node(
    base: &BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "class" => {
            extract_inheritance_relationship(base, node, symbols, relationships);
            extract_module_inclusion_relationships(base, node, symbols, relationships);
        }
        "module" => {
            extract_module_inclusion_relationships(base, node, symbols, relationships);
        }
        _ => {}
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_relationships_from_node(base, child, symbols, relationships);
    }
}

/// Extract inheritance relationship from class definition
fn extract_inheritance_relationship(
    base: &BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    if let Some(superclass_node) = node.child_by_field_name("superclass") {
        let class_name = extract_name_from_node(node, |n| base.get_node_text(n), "name")
            .or_else(|| extract_name_from_node(node, |n| base.get_node_text(n), "constant"))
            .or_else(|| {
                // Fallback: find first constant child
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "constant" {
                        return Some(base.get_node_text(&child));
                    }
                }
                None
            })
            .unwrap_or_else(|| "UnknownClass".to_string());

        let superclass_name = base
            .get_node_text(&superclass_node)
            .replace('<', "")
            .trim()
            .to_string();

        if let (Some(from_symbol), Some(to_symbol)) = (
            symbols.iter().find(|s| s.name == class_name),
            symbols.iter().find(|s| s.name == superclass_name),
        ) {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    from_symbol.id,
                    to_symbol.id,
                    RelationshipKind::Extends,
                    node.start_position().row
                ),
                from_symbol_id: from_symbol.id.clone(),
                to_symbol_id: to_symbol.id.clone(),
                kind: RelationshipKind::Extends,
                file_path: base.file_path.clone(),
                line_number: node.start_position().row as u32 + 1,
                confidence: 1.0,
                metadata: None,
            });
        }
    }
}

/// Extract module inclusion relationships (include, extend, prepend, using)
fn extract_module_inclusion_relationships(
    base: &BaseExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let class_or_module_name = extract_name_from_node(node, |n| base.get_node_text(n), "name")
        .or_else(|| extract_name_from_node(node, |n| base.get_node_text(n), "constant"))
        .or_else(|| {
            // Fallback: find first constant child
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "constant" {
                    return Some(base.get_node_text(&child));
                }
            }
            None
        })
        .unwrap_or_else(|| "Unknown".to_string());

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "call" {
            // Direct call node
            process_include_extend_call(base, child, &class_or_module_name, symbols, relationships);
        } else if child.kind() == "body_statement" {
            // Call might be inside a body_statement
            let mut body_cursor = child.walk();
            for body_child in child.children(&mut body_cursor) {
                if body_child.kind() == "call" {
                    process_include_extend_call(
                        base,
                        body_child,
                        &class_or_module_name,
                        symbols,
                        relationships,
                    );
                }
            }
        }
    }
}

/// Process a single include/extend call node
fn process_include_extend_call(
    base: &BaseExtractor,
    child: Node,
    class_or_module_name: &str,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    if let Some(method_name) = extract_method_name_from_call(child, |n| base.get_node_text(n)) {
        if matches!(
            method_name.as_str(),
            "include" | "extend" | "prepend" | "using"
        ) {
            if let Some(arg_node) = child.child_by_field_name("arguments") {
                if let Some(module_node) = arg_node.children(&mut arg_node.walk()).next() {
                    let module_name = base.get_node_text(&module_node);

                    let from_symbol = symbols.iter().find(|s| s.name == class_or_module_name);
                    let to_symbol = symbols.iter().find(|s| s.name == module_name);

                    if let (Some(from_symbol), Some(to_symbol)) = (from_symbol, to_symbol) {
                        relationships.push(Relationship {
                            id: format!(
                                "{}_{}_{:?}_{}",
                                from_symbol.id,
                                to_symbol.id,
                                RelationshipKind::Implements,
                                child.start_position().row
                            ),
                            from_symbol_id: from_symbol.id.clone(),
                            to_symbol_id: to_symbol.id.clone(),
                            kind: RelationshipKind::Implements,
                            file_path: base.file_path.clone(),
                            line_number: child.start_position().row as u32 + 1,
                            confidence: 1.0,
                            metadata: None,
                        });
                    }
                }
            }
        }
    }
}
