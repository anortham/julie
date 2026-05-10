//! Relationship extraction for JavaScript (calls, inheritance)
//!
//! This module handles extraction of relationships between symbols such as
//! function calls and class inheritance relationships.
//!
//! Adapted from TypeScript extractor (JavaScript and TypeScript share AST structure)

use crate::base::{
    LocalTargetResolution, Relationship, RelationshipKind, ScopedSymbolIndex, Symbol, SymbolKind,
    UnresolvedTarget,
};
use crate::javascript::JavaScriptExtractor;
use tree_sitter::{Node, Tree};

/// Extract all relationships from the syntax tree
pub(crate) fn extract_relationships(
    extractor: &mut JavaScriptExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    let symbol_index = ScopedSymbolIndex::new(symbols);
    extract_call_relationships(
        extractor,
        tree.root_node(),
        symbols,
        &symbol_index,
        &mut relationships,
    );
    extract_new_expression_relationships(
        extractor,
        tree.root_node(),
        symbols,
        &symbol_index,
        &mut relationships,
    );
    extract_inheritance_relationships(extractor, tree.root_node(), symbols, &mut relationships);
    relationships
}

fn extract_new_expression_relationships(
    extractor: &mut JavaScriptExtractor,
    node: Node,
    symbols: &[Symbol],
    symbol_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    if node.kind() == "new_expression" {
        if let Some(constructor_node) = node.child_by_field_name("constructor") {
            let target = extract_call_target(extractor, constructor_node);
            let caller = find_containing_callable_symbol(node, symbols).cloned();
            if let Some(caller) = caller {
                let resolution = symbol_index.resolve_call_target(
                    &target.terminal_name,
                    Some(&caller),
                    target.receiver.as_deref(),
                );
                if let LocalTargetResolution::Resolved(type_symbol) = &resolution {
                    if matches!(
                        type_symbol.kind,
                        SymbolKind::Class | SymbolKind::Type | SymbolKind::Interface
                    ) {
                        relationships.push(Relationship {
                            id: format!(
                                "{}_{}_{:?}_{}",
                                caller.id,
                                type_symbol.id,
                                RelationshipKind::Instantiates,
                                node.start_position().row
                            ),
                            from_symbol_id: caller.id.clone(),
                            to_symbol_id: type_symbol.id.clone(),
                            kind: RelationshipKind::Instantiates,
                            file_path: extractor.base().file_path.clone(),
                            line_number: (node.start_position().row + 1) as u32,
                            confidence: 1.0,
                            metadata: None,
                        });
                    }
                } else {
                    let pending = extractor.base().create_pending_relationship(
                        caller.id.clone(),
                        target,
                        RelationshipKind::Instantiates,
                        &node,
                        Some(caller.id.clone()),
                        Some(0.9),
                    );
                    extractor.add_structured_pending_relationship(pending);
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_new_expression_relationships(extractor, child, symbols, symbol_index, relationships);
    }
}

/// Extract function call relationships
fn extract_call_relationships(
    extractor: &mut JavaScriptExtractor,
    node: Node,
    symbols: &[Symbol],
    symbol_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    // Look for call expressions
    if node.kind() == "call_expression" {
        if let Some(function_node) = node.child_by_field_name("function") {
            let target = extract_call_target(extractor, function_node);

            // Find the calling function (containing function)
            if let Some(caller_symbol) = find_containing_callable_symbol(node, symbols) {
                let resolved_symbol = match symbol_index.resolve_call_target(
                    &target.terminal_name,
                    Some(caller_symbol),
                    target.receiver.as_deref(),
                ) {
                    LocalTargetResolution::Resolved(symbol) => Some(symbol),
                    _ if target.receiver.is_none() => {
                        unique_callable_symbol(symbols, &target.terminal_name)
                    }
                    _ => None,
                };

                if let Some(called_symbol) = resolved_symbol {
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
                        file_path: extractor.base().file_path.clone(),
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
        extract_call_relationships(extractor, child, symbols, symbol_index, relationships);
    }
}

fn unique_callable_symbol<'a>(symbols: &'a [Symbol], name: &str) -> Option<&'a Symbol> {
    let mut matches = symbols.iter().filter(|symbol| {
        symbol.name == name
            && matches!(
                symbol.kind,
                SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor
            )
    });
    let symbol = matches.next()?;
    matches.next().is_none().then_some(symbol)
}

fn find_containing_callable_symbol<'a>(node: Node, symbols: &'a [Symbol]) -> Option<&'a Symbol> {
    let byte = node.start_byte() as u32;
    symbols
        .iter()
        .filter(|symbol| {
            matches!(
                symbol.kind,
                SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor
            ) && symbol.start_byte <= byte
                && symbol.end_byte >= byte
        })
        .min_by_key(|symbol| symbol.end_byte - symbol.start_byte)
}

fn extract_call_target(extractor: &JavaScriptExtractor, function_node: Node) -> UnresolvedTarget {
    if function_node.kind() == "member_expression" {
        let receiver = function_node
            .child_by_field_name("object")
            .map(|node| extractor.base().get_node_text(&node));
        let terminal_name = function_node
            .child_by_field_name("property")
            .map(|node| extractor.base().get_node_text(&node))
            .unwrap_or_else(|| extractor.base().get_node_text(&function_node));
        let display_name = receiver
            .as_ref()
            .map(|receiver| format!("{receiver}.{terminal_name}"))
            .unwrap_or_else(|| terminal_name.clone());

        return UnresolvedTarget {
            display_name,
            terminal_name,
            receiver,
            namespace_path: Vec::new(),
            import_context: None,
        };
    }

    let terminal_name = extractor.base().get_node_text(&function_node);
    UnresolvedTarget::simple(terminal_name)
}

/// Extract inheritance relationships (extends)
fn extract_inheritance_relationships(
    extractor: &mut JavaScriptExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Phase 1: Collect data using immutable borrow
    let heritage_data = match node.kind() {
        "extends_clause" | "class_heritage" => collect_heritage_data(extractor, node, symbols),
        _ => None,
    };

    // Phase 2: Create relationships (may need &mut extractor for pending)
    if let Some((class_symbol_id, base_types, file_path)) = heritage_data {
        for (target, line_number) in base_types {
            let lookup_name = target.terminal_name.clone();
            // JS only has extends, check for Class (JS has no Interface kind)
            if let Some(base_symbol) = symbols.iter().find(|s| {
                s.name == lookup_name && matches!(s.kind, SymbolKind::Class | SymbolKind::Interface)
            }) {
                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        class_symbol_id,
                        base_symbol.id,
                        RelationshipKind::Extends,
                        line_number - 1
                    ),
                    from_symbol_id: class_symbol_id.clone(),
                    to_symbol_id: base_symbol.id.clone(),
                    kind: RelationshipKind::Extends,
                    file_path: file_path.clone(),
                    line_number,
                    confidence: 1.0,
                    metadata: None,
                });
            } else {
                // Cross-file: superclass is defined in another file
                let mut pending = extractor.base().create_pending_relationship(
                    class_symbol_id.clone(),
                    target.clone(),
                    RelationshipKind::Extends,
                    &node,
                    Some(class_symbol_id.clone()),
                    Some(0.9),
                );
                pending.pending.callee_name = target.terminal_name;
                pending.pending.line_number = line_number;
                extractor.add_structured_pending_relationship(pending);
            }
        }
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_inheritance_relationships(extractor, child, symbols, relationships);
    }
}

/// Collect heritage clause data without needing mutable access
fn collect_heritage_data(
    extractor: &JavaScriptExtractor,
    node: Node,
    symbols: &[Symbol],
) -> Option<(String, Vec<(UnresolvedTarget, u32)>, String)> {
    let mut parent = node.parent()?;
    while parent.kind() != "class_declaration" {
        parent = parent.parent()?;
    }

    let class_name_node = parent.child_by_field_name("name")?;
    let class_name = extractor.base().get_node_text(&class_name_node);
    let class_symbol = symbols
        .iter()
        .find(|s| s.name == class_name && s.kind == SymbolKind::Class)?;

    let mut base_types = Vec::new();
    match node.kind() {
        "extends_clause" => collect_explicit_superclass_targets(extractor, node, &mut base_types),
        "class_heritage" => {
            let mut found_structured_clause = false;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "extends_clause" {
                    found_structured_clause = true;
                }
            }

            if found_structured_clause {
                return None;
            }

            // Direct equivalent: some grammars model class_heritage without nested extends_clause.
            collect_explicit_superclass_targets(extractor, node, &mut base_types);
        }
        _ => return None,
    }

    Some((
        class_symbol.id.clone(),
        base_types,
        extractor.base().file_path.clone(),
    ))
}

fn extract_terminal_heritage_identifier(
    extractor: &JavaScriptExtractor,
    node: Node,
) -> Option<(UnresolvedTarget, u32)> {
    match node.kind() {
        "identifier" | "type_identifier" | "property_identifier" => {
            let name = extractor.base().get_node_text(&node);
            let line = (node.start_position().row + 1) as u32;
            Some((UnresolvedTarget::simple(name), line))
        }
        "member_expression" => {
            let object = node
                .child_by_field_name("object")
                .or_else(|| node.child_by_field_name("left"))?;
            let property = node
                .child_by_field_name("property")
                .or_else(|| node.child_by_field_name("right"))?;

            // Restrict to explicit identifier/member chains.
            extract_terminal_heritage_identifier(extractor, object)?;
            let (_, line) = extract_terminal_heritage_identifier(extractor, property)?;
            let display_name = extractor.base().get_node_text(&node).replace(' ', "");
            let segments: Vec<String> = display_name
                .split('.')
                .filter(|segment| !segment.is_empty())
                .map(|segment| segment.to_string())
                .collect();
            let terminal_name = segments.last()?.clone();
            let namespace_path = if segments.len() > 1 {
                segments[..segments.len() - 1].to_vec()
            } else {
                Vec::new()
            };
            Some((
                UnresolvedTarget {
                    display_name,
                    terminal_name,
                    receiver: None,
                    namespace_path,
                    import_context: None,
                },
                line,
            ))
        }
        "parenthesized_expression" => {
            let expression = node.child_by_field_name("expression")?;
            extract_terminal_heritage_identifier(extractor, expression)
        }
        "call_expression" | "new_expression" => None,
        _ => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if let Some(candidate) = extract_terminal_heritage_identifier(extractor, child) {
                    return Some(candidate);
                }
            }
            None
        }
    }
}

fn collect_explicit_superclass_targets(
    extractor: &JavaScriptExtractor,
    node: Node,
    base_types: &mut Vec<(UnresolvedTarget, u32)>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some((name, line)) = extract_terminal_heritage_identifier(extractor, child) {
            base_types.push((name, line));
            break;
        }
    }
}
