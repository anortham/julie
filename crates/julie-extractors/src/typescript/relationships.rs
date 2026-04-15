//! Relationship extraction (calls, inheritance)
//!
//! This module handles extraction of relationships between symbols such as
//! function calls and class inheritance relationships.

use crate::base::{Relationship, RelationshipKind, Symbol, SymbolKind, UnresolvedTarget};
use crate::typescript::TypeScriptExtractor;
use tree_sitter::{Node, Tree};

/// Extract all relationships from the syntax tree
pub(crate) fn extract_relationships(
    extractor: &mut TypeScriptExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    extract_call_relationships(extractor, tree.root_node(), symbols, &mut relationships);
    extract_inheritance_relationships(extractor, tree.root_node(), symbols, &mut relationships);
    relationships
}

/// Extract function call relationships
fn extract_call_relationships(
    extractor: &mut TypeScriptExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Look for call expressions
    if node.kind() == "call_expression" {
        if let Some(function_node) = node.child_by_field_name("function") {
            let function_name = extractor.base().get_node_text(&function_node);

            // Find the calling function (containing function)
            if let Some(caller_symbol) = find_containing_function(node, symbols) {
                // Find the called function symbol
                if let Some(called_symbol) = symbols
                    .iter()
                    .find(|s| s.name == function_name && matches!(s.kind, SymbolKind::Function))
                {
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
        extract_call_relationships(extractor, child, symbols, relationships);
    }
}

/// Extract inheritance relationships (extends, implements)
fn extract_inheritance_relationships(
    extractor: &mut TypeScriptExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Phase 1: Collect data using immutable borrow
    let heritage_data = match node.kind() {
        "extends_clause"
        | "class_heritage"
        | "implements_clause"
        | "extends_type_clause"
        | "implements_type_clause" => collect_heritage_data(extractor, node, symbols),
        _ => None,
    };

    // Phase 2: Create relationships (may need &mut extractor for pending)
    if let Some((class_symbol_id, base_types, file_path)) = heritage_data {
        for (target, line_number, pending_kind) in base_types {
            let lookup_name = target.terminal_name.clone();
            if let Some(base_symbol) = symbols.iter().find(|s| {
                s.name == lookup_name
                    && matches!(
                        s.kind,
                        SymbolKind::Class | SymbolKind::Interface | SymbolKind::Struct
                    )
            }) {
                // Same-file: resolve directly using target's actual kind
                let kind = if base_symbol.kind == SymbolKind::Interface {
                    RelationshipKind::Implements
                } else {
                    RelationshipKind::Extends
                };
                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        class_symbol_id,
                        base_symbol.id,
                        kind,
                        line_number - 1
                    ),
                    from_symbol_id: class_symbol_id.clone(),
                    to_symbol_id: base_symbol.id.clone(),
                    kind,
                    file_path: file_path.clone(),
                    line_number,
                    confidence: 1.0,
                    metadata: None,
                });
            } else {
                // Cross-file: base type is defined in another file
                let mut pending = extractor.base().create_pending_relationship(
                    class_symbol_id.clone(),
                    target.clone(),
                    pending_kind,
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
    extractor: &TypeScriptExtractor,
    node: Node,
    symbols: &[Symbol],
) -> Option<(
    String,
    Vec<(UnresolvedTarget, u32, RelationshipKind)>,
    String,
)> {
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
        "extends_clause" => {
            collect_clause_targets(extractor, node, RelationshipKind::Extends, &mut base_types)
        }
        "extends_type_clause" => {
            collect_clause_targets(extractor, node, RelationshipKind::Extends, &mut base_types)
        }
        "implements_clause" => collect_clause_targets(
            extractor,
            node,
            RelationshipKind::Implements,
            &mut base_types,
        ),
        "implements_type_clause" => collect_clause_targets(
            extractor,
            node,
            RelationshipKind::Implements,
            &mut base_types,
        ),
        "class_heritage" => {
            let mut found_structured_clause = false;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "extends_clause"
                    | "implements_clause"
                    | "extends_type_clause"
                    | "implements_type_clause" => found_structured_clause = true,
                    _ => {}
                }
            }

            if found_structured_clause {
                return None;
            }

            // Direct equivalent: some grammars model class_heritage without nested clause nodes.
            collect_clause_targets(extractor, node, RelationshipKind::Extends, &mut base_types);
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
    extractor: &TypeScriptExtractor,
    node: Node,
) -> Option<(UnresolvedTarget, u32)> {
    match node.kind() {
        "identifier" | "type_identifier" | "property_identifier" => {
            let name = extractor.base().get_node_text(&node);
            let line = (node.start_position().row + 1) as u32;
            Some((UnresolvedTarget::simple(name), line))
        }
        "generic_type" => {
            let display_name = extractor
                .base()
                .get_node_text(&node)
                .replace(' ', "")
                .split('<')
                .next()?
                .to_string();
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
            let line = (node.start_position().row + 1) as u32;
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
        "nested_type_identifier" | "member_expression" | "qualified_name" => {
            let left = node
                .child_by_field_name("object")
                .or_else(|| node.child_by_field_name("left"))
                .or_else(|| node.child_by_field_name("qualifier"));
            let right = node
                .child_by_field_name("name")
                .or_else(|| node.child_by_field_name("property"))
                .or_else(|| node.child_by_field_name("right"));

            if let (Some(left), Some(right)) = (left, right) {
                extract_terminal_heritage_identifier(extractor, left)?;
                let (_, line) = extract_terminal_heritage_identifier(extractor, right)?;
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
                return Some((
                    UnresolvedTarget {
                        display_name,
                        terminal_name,
                        receiver: None,
                        namespace_path,
                        import_context: None,
                    },
                    line,
                ));
            }

            let mut cursor = node.walk();
            let mut named_children: Vec<Node> = node.named_children(&mut cursor).collect();
            while let Some(child) = named_children.pop() {
                if let Some(target) = extract_terminal_heritage_identifier(extractor, child) {
                    return Some(target);
                }
            }
            None
        }
        "type_arguments" | "type_parameter" | "type_parameters" => None,
        _ => None,
    }
}

fn collect_clause_targets(
    extractor: &TypeScriptExtractor,
    node: Node,
    relationship_kind: RelationshipKind,
    base_types: &mut Vec<(UnresolvedTarget, u32, RelationshipKind)>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_arguments" {
            continue;
        }

        if let Some((name, line)) = extract_terminal_heritage_identifier(extractor, child) {
            base_types.push((name, line, relationship_kind.clone()));
            // TypeScript only allows a single superclass in extends_clause;
            // break after the first target to match JS semantics.
            if relationship_kind == RelationshipKind::Extends {
                break;
            }
        }
    }
}

/// Helper to find the function that contains a given node
fn find_containing_function<'a>(node: Node, symbols: &'a [Symbol]) -> Option<&'a Symbol> {
    let mut current = Some(node);

    while let Some(current_node) = current {
        let position = current_node.start_position();
        let pos_line = (position.row + 1) as u32;

        // Find function symbols that contain this position
        for symbol in symbols {
            if matches!(
                symbol.kind,
                SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor
            ) && symbol.start_line <= pos_line
                && symbol.end_line >= pos_line
            {
                return Some(symbol);
            }
        }

        current = current_node.parent();
    }

    None
}
