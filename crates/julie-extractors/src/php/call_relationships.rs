// PHP Extractor - call and object creation relationships

use super::{
    PhpExtractor,
    relationships::{strip_php_namespace, unresolved_php_type_target},
};
use crate::base::{
    LocalTargetResolution, Relationship, RelationshipKind, ScopedSymbolIndex, Symbol, SymbolKind,
    UnresolvedTarget,
};
use std::collections::HashMap;
use tree_sitter::Node;

pub(super) fn extract_call_relationships(
    extractor: &mut PhpExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let base = extractor.get_base();

    let called_function_name = match node.kind() {
        "function_call_expression" => {
            if let Some(name_node) = node.child_by_field_name("function") {
                base.get_node_text(&name_node)
            } else {
                return;
            }
        }
        "member_call_expression" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                base.get_node_text(&name_node)
            } else {
                return;
            }
        }
        "scoped_call_expression" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                base.get_node_text(&name_node)
            } else {
                return;
            }
        }
        "object_creation_expression" => {
            if let Some(class_name_node) = node.named_child(0) {
                let raw = base.get_node_text(&class_name_node);
                strip_php_namespace(raw.trim()).to_string()
            } else {
                return;
            }
        }
        _ => return,
    };

    if called_function_name.is_empty() {
        return;
    }

    let rel_kind = if node.kind() == "object_creation_expression" {
        RelationshipKind::Instantiates
    } else {
        RelationshipKind::Calls
    };

    if let Some(caller_symbol) = base
        .find_containing_symbol(&node, symbols)
        .filter(|symbol| {
            matches!(
                symbol.kind,
                SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor
            )
        })
    {
        let line_number = (node.start_position().row + 1) as u32;
        let file_path = base.file_path.clone();
        let target = unresolved_call_target(extractor, node, &called_function_name);

        if rel_kind == RelationshipKind::Calls {
            resolve_call_relationship(
                extractor,
                node,
                symbols,
                relationships,
                caller_symbol,
                target,
                file_path,
                line_number,
            );
            return;
        }

        resolve_instantiates_relationship(
            extractor,
            node,
            symbols,
            relationships,
            caller_symbol,
            &called_function_name,
            target,
            file_path,
            line_number,
        );
    }
}

fn resolve_call_relationship(
    extractor: &mut PhpExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
    caller_symbol: &Symbol,
    target: UnresolvedTarget,
    file_path: String,
    line_number: u32,
) {
    let symbol_index = ScopedSymbolIndex::new(symbols);
    match symbol_index.resolve_call_target(
        &target.terminal_name,
        Some(caller_symbol),
        target.receiver.as_deref(),
    ) {
        LocalTargetResolution::Resolved(called_symbol) => {
            relationships.push(Relationship {
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
            });
        }
        LocalTargetResolution::Import(_) => {
            add_pending_relationship(
                extractor,
                node,
                caller_symbol,
                target,
                RelationshipKind::Calls,
                0.8,
            );
        }
        LocalTargetResolution::Ambiguous
        | LocalTargetResolution::ReceiverQualified
        | LocalTargetResolution::Missing => {
            add_pending_relationship(
                extractor,
                node,
                caller_symbol,
                target,
                RelationshipKind::Calls,
                0.7,
            );
        }
    }
}

fn resolve_instantiates_relationship(
    extractor: &mut PhpExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
    caller_symbol: &Symbol,
    called_function_name: &str,
    target: UnresolvedTarget,
    file_path: String,
    line_number: u32,
) {
    let symbol_map: HashMap<String, &Symbol> = ScopedSymbolIndex::unique_symbol_map(symbols);
    match symbol_map.get(called_function_name) {
        Some(called_symbol)
            if !matches!(
                called_symbol.kind,
                SymbolKind::Class | SymbolKind::Interface | SymbolKind::Struct | SymbolKind::Enum
            ) =>
        {
            add_pending_relationship(
                extractor,
                node,
                caller_symbol,
                target,
                RelationshipKind::Instantiates,
                0.7,
            );
        }
        Some(called_symbol) if called_symbol.kind == SymbolKind::Import => {
            add_pending_relationship(
                extractor,
                node,
                caller_symbol,
                target,
                RelationshipKind::Instantiates,
                0.8,
            );
        }
        Some(called_symbol) => {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    caller_symbol.id,
                    called_symbol.id,
                    RelationshipKind::Instantiates,
                    node.start_position().row
                ),
                from_symbol_id: caller_symbol.id.clone(),
                to_symbol_id: called_symbol.id.clone(),
                kind: RelationshipKind::Instantiates,
                file_path,
                line_number,
                confidence: 0.9,
                metadata: None,
            });
        }
        None => {
            add_pending_relationship(
                extractor,
                node,
                caller_symbol,
                target,
                RelationshipKind::Instantiates,
                0.7,
            );
        }
    }
}

fn add_pending_relationship(
    extractor: &mut PhpExtractor,
    node: Node,
    caller_symbol: &Symbol,
    target: UnresolvedTarget,
    kind: RelationshipKind,
    confidence: f32,
) {
    let pending = extractor.get_base().create_pending_relationship(
        caller_symbol.id.clone(),
        target,
        kind,
        &node,
        Some(caller_symbol.id.clone()),
        Some(confidence),
    );
    extractor.add_structured_pending_relationship(pending);
}

fn unresolved_call_target(
    extractor: &PhpExtractor,
    node: Node,
    fallback_name: &str,
) -> UnresolvedTarget {
    match node.kind() {
        "member_call_expression" => member_call_target(extractor, node, fallback_name),
        "scoped_call_expression" => scoped_call_target(extractor, node, fallback_name),
        "object_creation_expression" => node
            .named_child(0)
            .map(|class_node| {
                unresolved_php_type_target(&extractor.get_base().get_node_text(&class_node))
            })
            .unwrap_or_else(|| unresolved_php_type_target(fallback_name)),
        _ => unresolved_php_type_target(fallback_name),
    }
}

fn member_call_target(
    extractor: &PhpExtractor,
    node: Node,
    fallback_name: &str,
) -> UnresolvedTarget {
    let receiver = node
        .child_by_field_name("object")
        .map(|object| extractor.get_base().get_node_text(&object))
        .map(|name| name.trim_start_matches('$').to_string());
    let terminal_name = node
        .child_by_field_name("name")
        .map(|name| extractor.get_base().get_node_text(&name))
        .unwrap_or_else(|| fallback_name.to_string());
    let display_name = receiver
        .as_ref()
        .map(|receiver| format!("{receiver}.{terminal_name}"))
        .unwrap_or_else(|| terminal_name.clone());
    UnresolvedTarget {
        display_name,
        terminal_name,
        receiver,
        namespace_path: Vec::new(),
        import_context: None,
    }
}

fn scoped_call_target(
    extractor: &PhpExtractor,
    node: Node,
    fallback_name: &str,
) -> UnresolvedTarget {
    let receiver_target = node
        .child_by_field_name("scope")
        .or_else(|| node.child_by_field_name("class"))
        .map(|scope| unresolved_php_type_target(&extractor.get_base().get_node_text(&scope)));
    let receiver = receiver_target
        .as_ref()
        .map(|target| target.terminal_name.clone());
    let namespace_path = receiver_target
        .map(|target| target.namespace_path)
        .unwrap_or_default();
    let terminal_name = node
        .child_by_field_name("name")
        .map(|name| extractor.get_base().get_node_text(&name))
        .unwrap_or_else(|| fallback_name.to_string());
    let display_name = receiver
        .as_ref()
        .map(|receiver| format!("{receiver}.{terminal_name}"))
        .unwrap_or_else(|| terminal_name.clone());
    UnresolvedTarget {
        display_name,
        terminal_name,
        receiver,
        namespace_path,
        import_context: None,
    }
}
