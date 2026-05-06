use super::helpers::{extract_method_name_from_call, extract_name_from_node};
/// Relationship extraction for Ruby symbols
/// Handles inheritance, module inclusion, and other symbol relationships
use crate::base::{
    LocalTargetResolution, Relationship, RelationshipKind, ScopedSymbolIndex, Symbol,
    UnresolvedTarget,
};
use tree_sitter::Node;

/// Extract all relationships from a tree
pub(super) fn extract_relationships(
    extractor: &mut super::RubyExtractor,
    tree: &tree_sitter::Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    let symbol_index = ScopedSymbolIndex::new(symbols);

    extract_relationships_from_node(
        extractor,
        tree.root_node(),
        symbols,
        &symbol_index,
        &mut relationships,
    );
    relationships
}

/// Recursively extract relationships from a node
fn extract_relationships_from_node(
    extractor: &mut super::RubyExtractor,
    node: Node,
    symbols: &[Symbol],
    symbol_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "class" => {
            extract_inheritance_relationship(extractor, node, symbols, relationships);
            extract_module_inclusion_relationships(extractor, node, symbols, relationships);
        }
        "module" => {
            extract_module_inclusion_relationships(extractor, node, symbols, relationships);
        }
        "call" => {
            extract_call_relationships(extractor, node, symbols, symbol_index, relationships);
        }
        _ => {}
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_relationships_from_node(extractor, child, symbols, symbol_index, relationships);
    }
}

/// Extract inheritance relationship from class definition
fn extract_inheritance_relationship(
    extractor: &mut super::RubyExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.base();
    if let Some(superclass_node) = node.child_by_field_name("superclass") {
        let Some(class_name) = extract_name_from_node(node, |n| base.get_node_text(n), "name")
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
        else {
            return;
        };

        let superclass_name = base
            .get_node_text(&superclass_node)
            .replace('<', "")
            .trim()
            .to_string();

        let Some(from_symbol) = symbols.iter().find(|s| s.name == class_name) else {
            return;
        };

        if let Some(to_symbol) = symbols.iter().find(|s| s.name == superclass_name) {
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
        } else {
            let pending = extractor.base().create_pending_relationship(
                from_symbol.id.clone(),
                unresolved_ruby_constant(superclass_name),
                RelationshipKind::Extends,
                &node,
                Some(from_symbol.id.clone()),
                Some(0.9),
            );
            extractor.add_structured_pending_relationship(pending);
        }
    }
}

/// Extract module inclusion relationships (include, extend, prepend, using)
fn extract_module_inclusion_relationships(
    extractor: &mut super::RubyExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.base();
    let Some(class_or_module_name) =
        extract_name_from_node(node, |n| base.get_node_text(n), "name")
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
    else {
        return;
    };

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "call" {
            // Direct call node
            process_include_extend_call(
                extractor,
                child,
                &class_or_module_name,
                symbols,
                relationships,
            );
        } else if child.kind() == "body_statement" {
            // Call might be inside a body_statement
            let mut body_cursor = child.walk();
            for body_child in child.children(&mut body_cursor) {
                if body_child.kind() == "call" {
                    process_include_extend_call(
                        extractor,
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
    extractor: &mut super::RubyExtractor,
    child: Node,
    class_or_module_name: &str,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.base();
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
                    } else if let Some(from_symbol) = from_symbol {
                        let pending = extractor.base().create_pending_relationship(
                            from_symbol.id.clone(),
                            unresolved_ruby_constant(module_name),
                            RelationshipKind::Implements,
                            &child,
                            Some(from_symbol.id.clone()),
                            Some(0.9),
                        );
                        extractor.add_structured_pending_relationship(pending);
                    }
                }
            }
        }
    }
}

/// Extract call relationships from a function/method call
fn extract_call_relationships(
    extractor: &mut super::RubyExtractor,
    node: Node,
    symbols: &[Symbol],
    symbol_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.base();

    // For a call node, extract the method being called
    if let Some(method_name_opt) = extract_method_name_from_call(node, |n| base.get_node_text(n)) {
        if !method_name_opt.is_empty() {
            let target = extract_pending_target(base, node, &method_name_opt);
            // Find the enclosing function/method that contains this call
            if let Some(caller_symbol) = base.find_containing_symbol(&node, symbols) {
                let line_number = (node.start_position().row + 1) as u32;
                let file_path = base.file_path.clone();

                match symbol_index.resolve_call_target(
                    &method_name_opt,
                    Some(caller_symbol),
                    target.receiver.as_deref(),
                ) {
                    LocalTargetResolution::Resolved(called_symbol) => {
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
                    LocalTargetResolution::Import(_)
                    | LocalTargetResolution::Ambiguous
                    | LocalTargetResolution::ReceiverQualified
                    | LocalTargetResolution::Missing => {
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
}

fn unresolved_ruby_constant(name: String) -> UnresolvedTarget {
    let parts: Vec<_> = name
        .split("::")
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();

    let terminal_name = parts.last().cloned().unwrap_or_else(|| name.clone());
    let namespace_path = parts
        .get(..parts.len().saturating_sub(1))
        .unwrap_or(&[])
        .to_vec();

    UnresolvedTarget {
        display_name: name,
        terminal_name,
        receiver: None,
        namespace_path,
        import_context: None,
    }
}

fn extract_pending_target(
    base: &crate::base::BaseExtractor,
    node: Node,
    method_name: &str,
) -> UnresolvedTarget {
    let call_text = base.get_node_text(&node);
    let call_head = call_text.split('(').next().unwrap_or(call_text.as_str());

    if let Some((receiver, _)) = call_head.rsplit_once('.') {
        if !receiver.is_empty() {
            return UnresolvedTarget {
                display_name: call_head.to_string(),
                terminal_name: method_name.to_string(),
                receiver: Some(receiver.to_string()),
                namespace_path: Vec::new(),
                import_context: None,
            };
        }
    }

    UnresolvedTarget::simple(method_name.to_string())
}
