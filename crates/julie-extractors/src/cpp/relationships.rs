//! Relationship extraction for C++
//! Handles inheritance and function call relationships

use crate::base::{
    LocalTargetResolution, Relationship, RelationshipKind, ScopedSymbolIndex, Symbol, SymbolKind,
    UnresolvedTarget,
};
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

    // Walk the tree looking for relationships
    walk_tree_for_relationships(
        extractor,
        tree.root_node(),
        symbols,
        &scoped_index,
        &mut relationships,
    );

    relationships
}

/// Recursively walk tree looking for inheritance and call relationships
fn walk_tree_for_relationships(
    extractor: &mut super::CppExtractor,
    node: Node,
    symbols: &[Symbol],
    scoped_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "class_specifier" | "struct_specifier" => {
            let inheritance = extract_inheritance_from_class(extractor, node, scoped_index);
            relationships.extend(inheritance);
        }
        "call_expression" | "function_call" => {
            extract_call_relationships(extractor, node, symbols, scoped_index, relationships);
        }
        "type_identifier" => {
            extract_type_use_relationship(extractor, node, symbols, scoped_index, relationships);
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_relationships(extractor, child, symbols, scoped_index, relationships);
    }
}

/// Extract inheritance relationships from a single class node
fn extract_inheritance_from_class(
    extractor: &mut super::CppExtractor,
    class_node: Node,
    scoped_index: &ScopedSymbolIndex<'_>,
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
    let Some(derived_symbol) = scoped_index.candidates_by_name(&class_name).find(|symbol| {
        is_inheritance_type(&symbol.kind) && symbol_span_matches_node(symbol, class_node)
    }) else {
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

        if let Some(base_symbol) =
            resolve_base_type_symbol(scoped_index, clean_base_name, derived_symbol)
        {
            relationships.push(base.create_relationship(
                derived_symbol.id.clone(),
                base_symbol.id.clone(),
                RelationshipKind::Extends,
                &class_node,
                Some(1.0),
                None,
            ));
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
    symbols: &[Symbol],
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
                symbols,
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
    symbols: &[Symbol],
    scoped_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    let Some(caller_symbol) = find_containing_callable_symbol(symbols, call_node) else {
        return;
    };

    let caller_id = caller_symbol.id.clone();

    // Check if we can resolve the callee locally
    match scoped_index.resolve_call_target(
        &target.terminal_name,
        Some(caller_symbol),
        target.receiver.as_deref(),
    ) {
        LocalTargetResolution::Resolved(called_symbol) => {
            relationships.push(extractor.get_base_mut().create_relationship(
                caller_id,
                called_symbol.id.clone(),
                RelationshipKind::Calls,
                &call_node,
                Some(0.9),
                None,
            ));
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

fn extract_type_use_relationship(
    extractor: &mut super::CppExtractor,
    node: Node,
    symbols: &[Symbol],
    scoped_index: &ScopedSymbolIndex<'_>,
    relationships: &mut Vec<Relationship>,
) {
    if helpers::is_type_declaration_name(&node) {
        return;
    }

    let base = extractor.get_base_mut();
    let type_name = base.get_node_text(&node);
    if helpers::is_noise_type(&type_name) {
        return;
    }

    let Some(source_symbol) = source_symbol_for_type_use(symbols, node) else {
        return;
    };
    let source_symbol_id = source_symbol.id.clone();

    if let Some(target_symbol) = resolve_type_use_symbol(scoped_index, &type_name) {
        if target_symbol.id == source_symbol_id {
            return;
        }
        push_unique_relationship(
            relationships,
            base.create_relationship(
                source_symbol_id,
                target_symbol.id.clone(),
                RelationshipKind::Uses,
                &node,
                Some(0.8),
                None,
            ),
        );
    } else {
        let pending = base.create_pending_relationship(
            source_symbol_id.clone(),
            UnresolvedTarget::simple(type_name),
            RelationshipKind::Uses,
            &node,
            Some(source_symbol_id),
            Some(0.7),
        );
        extractor.add_structured_pending_relationship(pending);
    }
}

fn find_containing_callable_symbol<'a>(
    symbols: &'a [Symbol],
    node: Node<'_>,
) -> Option<&'a Symbol> {
    symbols
        .iter()
        .filter(|symbol| is_callable(&symbol.kind) && symbol_contains_node(symbol, node))
        .min_by_key(|symbol| symbol.end_byte.saturating_sub(symbol.start_byte))
}

fn symbol_contains_node(symbol: &Symbol, node: Node) -> bool {
    let start_byte = node.start_byte() as u32;
    let end_byte = node.end_byte() as u32;
    symbol.start_byte <= start_byte && symbol.end_byte >= end_byte
}

fn symbol_span_matches_node(symbol: &Symbol, node: Node) -> bool {
    symbol.start_byte == node.start_byte() as u32 && symbol.end_byte == node.end_byte() as u32
}

fn is_callable(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor | SymbolKind::Operator
    )
}

fn resolve_base_type_symbol<'a>(
    scoped_index: &'a ScopedSymbolIndex<'a>,
    base_name: &str,
    derived_symbol: &Symbol,
) -> Option<&'a Symbol> {
    let type_candidates: Vec<&Symbol> = scoped_index
        .candidates_by_name(base_name)
        .filter(|symbol| is_inheritance_type(&symbol.kind))
        .collect();

    let same_parent: Vec<&Symbol> = type_candidates
        .iter()
        .copied()
        .filter(|symbol| symbol.parent_id == derived_symbol.parent_id)
        .collect();
    if let [base_symbol] = same_parent.as_slice() {
        return Some(*base_symbol);
    }

    if let [base_symbol] = type_candidates.as_slice() {
        return Some(*base_symbol);
    }

    None
}

fn is_inheritance_type(kind: &SymbolKind) -> bool {
    matches!(kind, SymbolKind::Class | SymbolKind::Struct)
}

fn source_symbol_for_type_use<'a>(symbols: &'a [Symbol], node: Node<'_>) -> Option<&'a Symbol> {
    let containing = symbols
        .iter()
        .filter(|symbol| symbol_contains_node(symbol, node))
        .min_by_key(|symbol| symbol.end_byte.saturating_sub(symbol.start_byte))?;
    if matches!(containing.kind, SymbolKind::Field | SymbolKind::Property) {
        if let Some(parent_id) = containing.parent_id.as_deref() {
            if let Some(parent) = symbols.iter().find(|symbol| symbol.id == parent_id) {
                return Some(parent);
            }
        }
    }
    Some(containing)
}

fn resolve_type_use_symbol<'a>(
    scoped_index: &'a ScopedSymbolIndex<'a>,
    type_name: &str,
) -> Option<&'a Symbol> {
    let candidates: Vec<&Symbol> = scoped_index
        .candidates_by_name(type_name)
        .filter(|symbol| is_type_use_symbol(&symbol.kind))
        .collect();
    if let [candidate] = candidates.as_slice() {
        return Some(*candidate);
    }

    let top_level: Vec<&Symbol> = candidates
        .iter()
        .copied()
        .filter(|symbol| symbol.parent_id.is_none())
        .collect();
    if let [candidate] = top_level.as_slice() {
        return Some(*candidate);
    }

    None
}

fn is_type_use_symbol(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Class
            | SymbolKind::Struct
            | SymbolKind::Union
            | SymbolKind::Enum
            | SymbolKind::Type
            | SymbolKind::Interface
            | SymbolKind::Trait
    )
}

fn push_unique_relationship(relationships: &mut Vec<Relationship>, relationship: Relationship) {
    if relationships.iter().any(|existing| {
        existing.kind == relationship.kind
            && existing.from_symbol_id == relationship.from_symbol_id
            && existing.to_symbol_id == relationship.to_symbol_id
    }) {
        return;
    }
    relationships.push(relationship);
}
