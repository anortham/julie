// Dart Extractor - Relationships Extraction
//
// Methods for extracting relationships between symbols (inheritance, uses, etc.)

use super::helpers::*;
use crate::base::{
    BaseExtractor, Relationship, RelationshipKind, Symbol, SymbolKind, UnresolvedTarget,
};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract relationships from the tree
pub(super) fn extract_relationships(
    base: &mut BaseExtractor,
    node: Node,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    let symbol_map: HashMap<String, &Symbol> =
        crate::base::ScopedSymbolIndex::unique_symbol_map(symbols);

    traverse_tree(node, &mut |current_node| match current_node.kind() {
        "class_definition" | "class_declaration" => {
            extract_class_relationships(base, &current_node, symbols, &mut relationships);
        }
        "member_access" | "assignable_expression" => {
            extract_method_call_relationships(
                base,
                &current_node,
                symbols,
                &symbol_map,
                &mut relationships,
            );
        }
        _ => {}
    });

    relationships
}

fn extract_class_relationships(
    base: &mut BaseExtractor,
    node: &Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let class_name = find_child_by_type(node, "identifier");
    if class_name.is_none() {
        return;
    }

    let class_symbol = symbols
        .iter()
        .find(|s| s.name == get_node_text(&class_name.unwrap()) && s.kind == SymbolKind::Class);
    if class_symbol.is_none() {
        return;
    }
    let class_symbol = class_symbol.unwrap();

    for (target_name, kind) in extract_class_header_targets(node) {
        emit_type_relationship_or_pending(
            base,
            class_symbol,
            symbols,
            node,
            &target_name,
            kind,
            relationships,
        );
    }

    if let Some(extends_clause) = find_child_by_type(node, "superclass") {
        let type_root = find_child_by_type(&extends_clause, "type")
            .or_else(|| find_child_by_type(&extends_clause, "type_identifier"));

        if let Some(type_node) = type_root {
            let mut type_names = Vec::new();
            traverse_tree(type_node, &mut |type_child| {
                if type_child.kind() == "type_identifier" {
                    type_names.push(get_node_text(&type_child));
                }
            });
            for generic_type_name in type_names.iter().skip(1) {
                if let Some(generic_type_symbol) = symbols
                    .iter()
                    .find(|s| s.name == *generic_type_name && s.kind == SymbolKind::Class)
                {
                    relationships.push(Relationship {
                        id: format!(
                            "{}_{}_{:?}_{}",
                            class_symbol.id,
                            generic_type_symbol.id,
                            RelationshipKind::Uses,
                            node.start_position().row
                        ),
                        from_symbol_id: class_symbol.id.clone(),
                        to_symbol_id: generic_type_symbol.id.clone(),
                        kind: RelationshipKind::Uses,
                        file_path: base.file_path.clone(),
                        line_number: node.start_position().row as u32 + 1,
                        confidence: 1.0,
                        metadata: None,
                    });
                }
            }
        }
    }
}

fn extract_class_header_targets(node: &Node) -> Vec<(String, RelationshipKind)> {
    let mut targets = Vec::new();

    if let Some(superclass_clause) = find_child_by_type(node, "superclass") {
        let type_root = find_child_by_type(&superclass_clause, "type")
            .or_else(|| find_child_by_type(&superclass_clause, "type_identifier"));
        if let Some(type_node) = type_root {
            if let Some(target_name) = normalize_type_name(&get_node_text(&type_node)) {
                targets.push((target_name, RelationshipKind::Extends));
            }
        }

        if let Some(mixin_clause) = find_child_by_type(&superclass_clause, "mixins") {
            append_clause_targets(&mut targets, &mixin_clause, "with", RelationshipKind::Uses);
        }
    }

    if let Some(interfaces_clause) = find_child_by_type(node, "interfaces") {
        append_clause_targets(
            &mut targets,
            &interfaces_clause,
            "implements",
            RelationshipKind::Implements,
        );
    }

    targets
}

fn append_clause_targets(
    targets: &mut Vec<(String, RelationshipKind)>,
    clause_node: &Node,
    keyword: &str,
    kind: RelationshipKind,
) {
    let clause_text = get_node_text(clause_node);
    let clause_text = strip_leading_keyword(clause_text.trim(), keyword);
    for target_name in extract_type_list(clause_text) {
        targets.push((target_name, kind.clone()));
    }
}

fn strip_leading_keyword<'a>(source: &'a str, keyword: &str) -> &'a str {
    source.strip_prefix(keyword).unwrap_or(source).trim_start()
}

fn extract_type_list(clause: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut start = 0;
    let mut depth = 0_u32;

    for (index, ch) in clause.char_indices() {
        match ch {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                if let Some(target_name) = normalize_type_name(&clause[start..index]) {
                    targets.push(target_name);
                }
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }

    if let Some(target_name) = normalize_type_name(&clause[start..]) {
        targets.push(target_name);
    }

    targets
}

fn normalize_type_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let end = trimmed
        .char_indices()
        .find_map(|(index, ch)| (!is_type_name_char(ch)).then_some(index))
        .unwrap_or(trimmed.len());
    let name = trimmed[..end].trim_matches('.');

    (!name.is_empty()).then(|| name.to_string())
}

fn is_type_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'
}

fn emit_type_relationship_or_pending(
    base: &mut BaseExtractor,
    class_symbol: &Symbol,
    symbols: &[Symbol],
    node: &Node,
    target_name: &str,
    kind: RelationshipKind,
    relationships: &mut Vec<Relationship>,
) {
    let target = unresolved_type_target(symbols, target_name);
    if let Some(target_symbol) = find_local_type_symbol(symbols, &target.terminal_name) {
        relationships.push(Relationship {
            id: format!(
                "{}_{}_{:?}_{}",
                class_symbol.id,
                target_symbol.id,
                kind,
                node.start_position().row
            ),
            from_symbol_id: class_symbol.id.clone(),
            to_symbol_id: target_symbol.id.clone(),
            kind,
            file_path: base.file_path.clone(),
            line_number: node.start_position().row as u32 + 1,
            confidence: 1.0,
            metadata: None,
        });
    } else {
        let pending = base.create_pending_relationship(
            class_symbol.id.clone(),
            target,
            kind,
            node,
            Some(class_symbol.id.clone()),
            Some(0.8),
        );
        base.add_structured_pending_relationship(pending);
    }
}

fn find_local_type_symbol<'a>(symbols: &'a [Symbol], target_name: &str) -> Option<&'a Symbol> {
    symbols.iter().find(|symbol| {
        symbol.name == target_name
            && matches!(
                symbol.kind,
                SymbolKind::Class | SymbolKind::Interface | SymbolKind::Type
            )
    })
}

fn unresolved_type_target(symbols: &[Symbol], target_name: &str) -> UnresolvedTarget {
    let (receiver, terminal_name) = target_name
        .rsplit_once('.')
        .map(|(receiver, terminal)| (Some(receiver.to_string()), terminal.to_string()))
        .unwrap_or_else(|| (None, target_name.to_string()));
    let namespace_path = receiver
        .as_deref()
        .map(|receiver| receiver.split('.').map(str::to_string).collect())
        .unwrap_or_default();

    UnresolvedTarget {
        display_name: target_name.to_string(),
        terminal_name,
        receiver,
        namespace_path,
        import_context: import_context_for_target(symbols, target_name),
    }
}

fn import_context_for_target(symbols: &[Symbol], target_name: &str) -> Option<String> {
    let imports: Vec<&Symbol> = symbols
        .iter()
        .filter(|symbol| symbol.kind == SymbolKind::Import)
        .collect();

    if let Some((receiver, _)) = target_name.rsplit_once('.') {
        let alias = format!(" as {receiver}");
        return imports.iter().find_map(|symbol| {
            symbol
                .signature
                .as_ref()
                .filter(|signature| signature.contains(&alias))
                .cloned()
        });
    }

    if imports.len() == 1 {
        imports[0]
            .signature
            .clone()
            .or_else(|| Some(imports[0].name.clone()))
    } else {
        None
    }
}

fn extract_method_call_relationships(
    base: &mut BaseExtractor,
    node: &Node,
    symbols: &[Symbol],
    symbol_map: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
) {
    // Check if this is actually a function call (has argument_part)
    let is_call = if let Some(selector_node) = find_child_by_type(node, "selector") {
        find_child_by_type(&selector_node, "argument_part").is_some()
    } else {
        false
    };

    // Only process if this is a function call
    if !is_call {
        return;
    }

    // Extract the function/method name being called
    let function_name = if let Some(object_node) = node.child_by_field_name("object") {
        // For object.method(), get just "method"
        if let Some(selector_node) = node.child_by_field_name("selector") {
            if let Some(id_node) = find_child_by_type(&selector_node, "identifier") {
                get_node_text(&id_node)
            } else {
                get_node_text(&selector_node)
            }
        } else {
            get_node_text(&object_node)
        }
    } else if let Some(selector_node) = node.child_by_field_name("selector") {
        if let Some(id_node) = find_child_by_type(&selector_node, "identifier") {
            get_node_text(&id_node)
        } else {
            get_node_text(&selector_node)
        }
    } else {
        return;
    };

    // Find the called function in our symbols
    if let Some(called_symbol) = symbol_map.get(function_name.as_str()) {
        // Find the containing function that's making this call
        if let Some(caller_symbol) = find_containing_function(base, node, symbols) {
            // Create a Relationship for this call
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
                file_path: base.file_path.clone(),
                line_number: node.start_position().row as u32 + 1,
                confidence: 0.9,
                metadata: None,
            });
        }
    }
}

/// Find the containing function for a node by walking up the tree
fn find_containing_function<'a>(
    base: &BaseExtractor,
    node: &Node,
    symbols: &'a [Symbol],
) -> Option<&'a Symbol> {
    let mut current = node.parent();

    while let Some(current_node) = current {
        // Check for function or method declarations
        if current_node.kind() == "function_declaration"
            || current_node.kind() == "method_signature"
            || current_node.kind() == "function_signature"
        {
            // Get the function name
            if let Some(name_node) = find_child_by_type(&current_node, "identifier") {
                let func_name = get_node_text(&name_node);
                // Find this function in symbols, but only from the current file
                if let Some(symbol) = symbols.iter().find(|s| {
                    s.name == func_name
                        && s.file_path == base.file_path
                        && matches!(s.kind, SymbolKind::Function | SymbolKind::Method)
                }) {
                    return Some(symbol);
                }
            }
        }

        current = current_node.parent();
    }

    None
}
