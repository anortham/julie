/// Relationship extraction
/// Handles inheritance relationships and function call relationships
use super::super::base::{
    LocalTargetResolution, Relationship, RelationshipKind, ScopedSymbolIndex, Symbol, SymbolKind,
    UnresolvedTarget,
};
use super::{PythonExtractor, helpers};
use tree_sitter::{Node, Tree};

/// Extract relationships from Python code
pub(crate) fn extract_relationships(
    extractor: &mut PythonExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();

    let symbol_index = ScopedSymbolIndex::new(symbols);

    // Recursively visit all nodes to extract relationships
    visit_node_for_relationships(
        extractor,
        tree.root_node(),
        symbols,
        &symbol_index,
        &mut relationships,
    );

    relationships
}

/// Visit a node and extract relationships from it
fn visit_node_for_relationships(
    extractor: &mut PythonExtractor,
    node: Node,
    symbols: &[Symbol],
    symbol_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "class_definition" => {
            extract_class_relationships(extractor, node, symbol_index, relationships);
        }
        "call" => {
            extract_call_relationships(extractor, node, symbols, symbol_index, relationships);
        }
        _ => {}
    }

    // Recursively visit all children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_node_for_relationships(extractor, child, symbols, symbol_index, relationships);
    }
}

/// Extract inheritance relationships from a class definition
fn extract_class_relationships(
    extractor: &mut PythonExtractor,
    node: Node,
    symbol_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.base();

    // Get class name from the name field
    let name_node = match node.child_by_field_name("name") {
        Some(node) => node,
        None => return,
    };

    let class_name = base.get_node_text(&name_node);
    let class_symbol = match symbol_index.first_by_name(&class_name) {
        Some(symbol) => symbol,
        None => return,
    };

    // Extract inheritance relationships
    if let Some(superclasses_node) = node.child_by_field_name("superclasses") {
        let bases = helpers::extract_argument_list(extractor, &superclasses_node);

        for base_name in bases {
            if let Some(base_symbol) = symbol_index.first_by_name(&base_name) {
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
    extractor: &mut PythonExtractor,
    node: Node,
    symbols: &[Symbol],
    symbol_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    // For a call node, extract the function/method being called
    if let Some(function_node) = node.child_by_field_name("function") {
        let target = extract_target_from_call(extractor.base(), &function_node);
        let called_method_name = target.terminal_name.clone();

        if !called_method_name.is_empty() {
            // Find the enclosing function/method that contains this call
            if let Some(caller_symbol) = extractor.base().find_containing_symbol(&node, symbols) {
                let line_number = (node.start_position().row + 1) as u32;
                let file_path = extractor.base().file_path.clone();

                // Check if we can resolve the callee locally
                match symbol_index.resolve_call_target(
                    &called_method_name,
                    Some(caller_symbol),
                    target.receiver.as_deref(),
                ) {
                    LocalTargetResolution::Import(_) => {
                        // Target is an Import symbol - need cross-file resolution
                        // Don't create relationship pointing to Import (useless for trace_call_path)
                        // Instead, create a PendingRelationship with the callee name
                        let pending = extractor.base().create_pending_relationship(
                            caller_symbol.id.clone(),
                            target.clone(),
                            RelationshipKind::Calls,
                            &node,
                            Some(caller_symbol.id.clone()),
                            Some(0.8),
                        );
                        extractor.add_structured_pending_relationship(pending);
                    }
                    LocalTargetResolution::Resolved(called_symbol) => {
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
                    LocalTargetResolution::Ambiguous
                    | LocalTargetResolution::ReceiverQualified
                    | LocalTargetResolution::Missing => {
                        // Target not found in local symbols - likely a method on imported type
                        // Create PendingRelationship for cross-file resolution
                        let pending = extractor.base().create_pending_relationship(
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
}

/// Extract method name from a call node
fn extract_target_from_call(
    base: &crate::base::BaseExtractor,
    function_node: &Node,
) -> UnresolvedTarget {
    match function_node.kind() {
        "identifier" => {
            // Simple function call: foo()
            UnresolvedTarget::simple(base.get_node_text(function_node))
        }
        "attribute" => {
            // Method call: obj.method() or self.db.connect()
            if let Some(attribute_node) = function_node.child_by_field_name("attribute") {
                let terminal_name = base.get_node_text(&attribute_node);
                let receiver = function_node
                    .child_by_field_name("object")
                    .map(|node| base.get_node_text(&node));
                if let Some(receiver) = receiver {
                    UnresolvedTarget {
                        display_name: format!("{receiver}.{terminal_name}"),
                        terminal_name,
                        receiver: Some(receiver),
                        namespace_path: Vec::new(),
                        import_context: None,
                    }
                } else {
                    UnresolvedTarget::simple(terminal_name)
                }
            } else {
                UnresolvedTarget::simple(String::new())
            }
        }
        _ => UnresolvedTarget::simple(String::new()),
    }
}
