// QML Relationship Extraction
// Extracts relationships between QML symbols: function calls, signal connections, component instantiation

use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::extractors::qml::QmlExtractor;
use tree_sitter::{Node, Tree};

/// Extract all relationships from QML code
pub(super) fn extract_relationships(
    extractor: &QmlExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    extract_call_relationships(extractor, tree.root_node(), symbols, &mut relationships);
    extract_instantiation_relationships(extractor, tree.root_node(), symbols, &mut relationships);
    extract_property_binding_relationships(
        extractor,
        tree.root_node(),
        symbols,
        &mut relationships,
    );
    relationships
}

/// Extract function call relationships
fn extract_call_relationships(
    extractor: &QmlExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Match JavaScript call expressions (QML uses TypeScript/JavaScript grammar)
    if node.kind() == "call_expression" {
        if let Some(function_node) = node.child_by_field_name("function") {
            let function_name = match function_node.kind() {
                "identifier" => extractor.base.get_node_text(&function_node),
                "member_expression" => {
                    // For member_expression like object.method(), extract just the method name
                    if let Some(property) = function_node.child_by_field_name("property") {
                        extractor.base.get_node_text(&property)
                    } else {
                        extractor.base.get_node_text(&function_node)
                    }
                }
                _ => extractor.base.get_node_text(&function_node),
            };

            // Find the containing function (caller)
            if let Some(caller_symbol) = find_containing_function(node, symbols) {
                // Find the called function symbol
                if let Some(called_symbol) = symbols.iter().find(|s| {
                    s.name == function_name
                        && (s.kind == SymbolKind::Function || s.kind == SymbolKind::Event)
                }) {
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
                        file_path: extractor.base.file_path.clone(),
                        line_number: (node.start_position().row + 1) as u32,
                        confidence: 1.0,
                        metadata: None,
                    };
                    relationships.push(relationship);
                }
            }
        }
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_call_relationships(extractor, child, symbols, relationships);
    }
}

/// Extract component instantiation relationships (Rectangle {}, Button {}, etc.)
fn extract_instantiation_relationships(
    extractor: &QmlExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // QML component definitions are ui_object_definition nodes
    if node.kind() == "ui_object_definition" {
        if let Some(type_name_node) = node.child_by_field_name("type_name") {
            let component_type = extractor.base.get_node_text(&type_name_node);

            // Find the containing QML component (parent)
            if let Some(parent_symbol) = find_containing_component(node, symbols) {
                // The instantiated component is the symbol we created for this node
                // Find it by matching the node position
                let node_line = node.start_position().row + 1;
                if let Some(instantiated_symbol) = symbols.iter().find(|s| {
                    s.start_line == node_line as u32
                        && s.kind == SymbolKind::Class
                        && s.name.contains(&component_type)
                }) {
                    let relationship = Relationship {
                        id: format!(
                            "{}_{}_{:?}_{}",
                            parent_symbol.id,
                            instantiated_symbol.id,
                            RelationshipKind::Instantiates,
                            node.start_position().row
                        ),
                        from_symbol_id: parent_symbol.id.clone(),
                        to_symbol_id: instantiated_symbol.id.clone(),
                        kind: RelationshipKind::Instantiates,
                        file_path: extractor.base.file_path.clone(),
                        line_number: node_line as u32,
                        confidence: 1.0,
                        metadata: None,
                    };
                    relationships.push(relationship);
                }
            }
        }
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_instantiation_relationships(extractor, child, symbols, relationships);
    }
}

/// Extract property binding relationships (width: parent.width, etc.)
fn extract_property_binding_relationships(
    extractor: &QmlExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Look for member expressions anywhere (they represent property access)
    if node.kind() == "member_expression" {
        if let Some(property_node) = node.child_by_field_name("property") {
            let property_name = extractor.base.get_node_text(&property_node);

            // Find containing component
            if let Some(container_symbol) = find_containing_component(node, symbols) {
                // Create a Uses relationship for the property access
                let relationship = Relationship {
                    id: format!(
                        "{}_{:?}_{}_{}",
                        container_symbol.id,
                        RelationshipKind::Uses,
                        property_name,
                        node.start_position().row
                    ),
                    from_symbol_id: container_symbol.id.clone(),
                    to_symbol_id: format!("property_{}", property_name),
                    kind: RelationshipKind::Uses,
                    file_path: extractor.base.file_path.clone(),
                    line_number: (node.start_position().row + 1) as u32,
                    confidence: 0.8,
                    metadata: None,
                };
                relationships.push(relationship);
            }
        }
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_property_binding_relationships(extractor, child, symbols, relationships);
    }
}

/// Find the containing function for a node
/// Also checks for QML signal handlers (ui_script_binding, ui_binding with function bodies)
fn find_containing_function<'a>(node: Node, symbols: &'a [Symbol]) -> Option<&'a Symbol> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "function_declaration" => {
                // Find the symbol that matches this function
                let func_line = parent.start_position().row + 1;
                if let Some(symbol) = symbols
                    .iter()
                    .find(|s| s.kind == SymbolKind::Function && s.start_line == func_line as u32)
                {
                    return Some(symbol);
                }
            }
            "ui_script_binding" | "ui_binding" => {
                // Signal handlers like onClicked: { ... } are represented as ui_script_binding
                // Find the containing component instead
                return find_containing_component(parent, symbols);
            }
            _ => {}
        }
        current = parent;
    }
    None
}

/// Find the containing QML component for a node
fn find_containing_component<'a>(node: Node, symbols: &'a [Symbol]) -> Option<&'a Symbol> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "ui_object_definition" {
            // Find the symbol that matches this component
            let comp_line = parent.start_position().row + 1;
            if let Some(symbol) = symbols
                .iter()
                .find(|s| s.kind == SymbolKind::Class && s.start_line == comp_line as u32)
            {
                return Some(symbol);
            }
        }
        current = parent;
    }
    None
}
