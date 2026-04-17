use super::helpers;
use crate::base::{Relationship, RelationshipKind, Symbol, SymbolKind, UnresolvedTarget};
use crate::vbnet::VbNetExtractor;
use tree_sitter::Tree;

pub fn extract_relationships(
    extractor: &mut VbNetExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    visit_relationships(extractor, tree.root_node(), symbols, &mut relationships);
    relationships
}

fn visit_relationships(
    extractor: &mut VbNetExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "class_block" | "structure_block" => {
            extract_type_relationships(extractor, node, symbols, relationships);
        }
        "interface_block" => {
            extract_interface_relationships(extractor, node, symbols, relationships);
        }
        "constructor_declaration" => {
            extract_constructor_uses_relationships(extractor, node, symbols, relationships);
        }
        "field_declaration" => {
            extract_field_type_relationships(extractor, node, symbols, relationships);
        }
        "property_declaration" => {
            extract_property_type_relationships(extractor, node, symbols, relationships);
        }
        "invocation_expression" | "invocation" => {
            extract_call_relationships(extractor, node, symbols, relationships);
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_relationships(extractor, child, symbols, relationships);
    }
}

fn extract_type_relationships(
    extractor: &mut VbNetExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let (current_symbol_id, inherits_list, implements_list, file_path, line_number) = {
        let base = extractor.get_base();
        let name_node = node.child_by_field_name("name");
        let Some(name_node) = name_node else { return };

        let current_symbol_name = base.get_node_text(&name_node);
        let Some(current_symbol) = symbols.iter().find(|s| s.name == current_symbol_name) else {
            return;
        };

        let inherits = helpers::extract_inherits(base, &node);
        let implements = helpers::extract_implements(base, &node);

        (
            current_symbol.id.clone(),
            inherits,
            implements,
            base.file_path.clone(),
            (node.start_position().row + 1) as u32,
        )
    };

    for base_type_name in inherits_list {
        if let Some(base_symbol) = symbols.iter().find(|s| s.name == base_type_name) {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    current_symbol_id,
                    base_symbol.id,
                    RelationshipKind::Extends,
                    node.start_position().row
                ),
                from_symbol_id: current_symbol_id.clone(),
                to_symbol_id: base_symbol.id.clone(),
                kind: RelationshipKind::Extends,
                file_path: file_path.clone(),
                line_number,
                confidence: 1.0,
                metadata: None,
            });
        } else {
            let pending = extractor.get_base().create_pending_relationship(
                current_symbol_id.clone(),
                helpers::unresolved_type_target(&base_type_name)
                    .unwrap_or_else(|| UnresolvedTarget::simple(base_type_name)),
                RelationshipKind::Extends,
                &node,
                Some(current_symbol_id.clone()),
                Some(0.9),
            );
            extractor.add_structured_pending_relationship(pending);
        }
    }

    for impl_type_name in implements_list {
        if let Some(impl_symbol) = symbols.iter().find(|s| s.name == impl_type_name) {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    current_symbol_id,
                    impl_symbol.id,
                    RelationshipKind::Implements,
                    node.start_position().row
                ),
                from_symbol_id: current_symbol_id.clone(),
                to_symbol_id: impl_symbol.id.clone(),
                kind: RelationshipKind::Implements,
                file_path: file_path.clone(),
                line_number,
                confidence: 1.0,
                metadata: None,
            });
        } else {
            let pending = extractor.get_base().create_pending_relationship(
                current_symbol_id.clone(),
                helpers::unresolved_type_target(&impl_type_name)
                    .unwrap_or_else(|| UnresolvedTarget::simple(impl_type_name)),
                RelationshipKind::Implements,
                &node,
                Some(current_symbol_id.clone()),
                Some(0.9),
            );
            extractor.add_structured_pending_relationship(pending);
        }
    }
}

fn extract_interface_relationships(
    extractor: &mut VbNetExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let (current_symbol_id, inherits_list, file_path, line_number) = {
        let base = extractor.get_base();
        let name_node = node.child_by_field_name("name");
        let Some(name_node) = name_node else { return };

        let current_symbol_name = base.get_node_text(&name_node);
        let Some(current_symbol) = symbols.iter().find(|s| s.name == current_symbol_name) else {
            return;
        };

        let inherits = helpers::extract_inherits(base, &node);

        (
            current_symbol.id.clone(),
            inherits,
            base.file_path.clone(),
            (node.start_position().row + 1) as u32,
        )
    };

    for base_type_name in inherits_list {
        if let Some(base_symbol) = symbols.iter().find(|s| s.name == base_type_name) {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    current_symbol_id,
                    base_symbol.id,
                    RelationshipKind::Extends,
                    node.start_position().row
                ),
                from_symbol_id: current_symbol_id.clone(),
                to_symbol_id: base_symbol.id.clone(),
                kind: RelationshipKind::Extends,
                file_path: file_path.clone(),
                line_number,
                confidence: 1.0,
                metadata: None,
            });
        } else {
            let pending = extractor.get_base().create_pending_relationship(
                current_symbol_id.clone(),
                helpers::unresolved_type_target(&base_type_name)
                    .unwrap_or_else(|| UnresolvedTarget::simple(base_type_name)),
                RelationshipKind::Extends,
                &node,
                Some(current_symbol_id.clone()),
                Some(0.9),
            );
            extractor.add_structured_pending_relationship(pending);
        }
    }
}

fn extract_constructor_uses_relationships(
    extractor: &mut VbNetExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let parameter_list = node.child_by_field_name("parameters");
    let Some(parameter_list) = parameter_list else {
        return;
    };

    let Some(container) = find_containing_type(extractor, node, symbols) else {
        return;
    };

    let mut cursor = parameter_list.walk();
    for parameter in parameter_list.children(&mut cursor) {
        if parameter.kind() != "parameter" {
            continue;
        }
        if let Some(type_name) = helpers::extract_as_clause_type(extractor.get_base(), &parameter) {
            emit_uses_relationship(
                extractor,
                node,
                &container.id,
                &type_name,
                symbols,
                relationships,
            );
        }
    }
}

fn extract_field_type_relationships(
    extractor: &mut VbNetExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let Some(container) = find_containing_type(extractor, node, symbols) else {
        return;
    };

    let mut cursor = node.walk();
    let declarator = node
        .children(&mut cursor)
        .find(|child| child.kind() == "variable_declarator");
    let Some(declarator) = declarator else {
        return;
    };

    if let Some(type_name) = helpers::extract_as_clause_type(extractor.get_base(), &declarator) {
        emit_uses_relationship(
            extractor,
            node,
            &container.id,
            &type_name,
            symbols,
            relationships,
        );
    }
}

fn extract_property_type_relationships(
    extractor: &mut VbNetExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let Some(container) = find_containing_type(extractor, node, symbols) else {
        return;
    };

    if let Some(type_name) = helpers::extract_as_clause_type(extractor.get_base(), &node) {
        emit_uses_relationship(
            extractor,
            node,
            &container.id,
            &type_name,
            symbols,
            relationships,
        );
    }
}

fn find_containing_type<'a>(
    extractor: &VbNetExtractor,
    node: tree_sitter::Node,
    symbols: &'a [Symbol],
) -> Option<&'a Symbol> {
    let base = extractor.get_base();
    let mut current = Some(node);
    while let Some(candidate) = current {
        let expected_kind = match candidate.kind() {
            "class_block" | "module_block" => SymbolKind::Class,
            "structure_block" => SymbolKind::Struct,
            "interface_block" => SymbolKind::Interface,
            _ => {
                current = candidate.parent();
                continue;
            }
        };

        if let Some(name_node) = candidate.child_by_field_name("name") {
            let type_name = base.get_node_text(&name_node);
            let start_line = candidate.start_position().row as u32 + 1;

            if let Some(symbol) = symbols.iter().find(|symbol| {
                symbol.name == type_name
                    && symbol.kind == expected_kind
                    && symbol.file_path == base.file_path
                    && symbol.start_line == start_line
            }) {
                return Some(symbol);
            }

            return symbols.iter().find(|symbol| {
                symbol.name == type_name
                    && symbol.kind == expected_kind
                    && symbol.file_path == base.file_path
            });
        }

        current = candidate.parent();
    }

    None
}

fn emit_uses_relationship(
    extractor: &mut VbNetExtractor,
    node: tree_sitter::Node,
    container_symbol_id: &str,
    type_name: &str,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let Some(target) = helpers::unresolved_type_target(type_name) else {
        return;
    };

    let already_resolved = relationships.iter().any(|relationship| {
        relationship.from_symbol_id == container_symbol_id
            && relationship.kind == RelationshipKind::Uses
            && (relationship.to_symbol_id == target.terminal_name
                || symbols
                    .iter()
                    .find(|symbol| symbol.id == relationship.to_symbol_id)
                    .is_some_and(|symbol| symbol.name == target.terminal_name))
    });
    if already_resolved {
        return;
    }

    let already_pending = extractor.get_pending_relationships().iter().any(|pending| {
        pending.from_symbol_id == container_symbol_id
            && pending.kind == RelationshipKind::Uses
            && pending.callee_name == target.display_name
    });
    if already_pending {
        return;
    }

    let mut symbol_map: std::collections::HashMap<String, &Symbol> = symbols
        .iter()
        .map(|symbol| (symbol.name.clone(), symbol))
        .collect();
    for symbol in symbols.iter().filter(|symbol| {
        matches!(
            symbol.kind,
            SymbolKind::Class
                | SymbolKind::Interface
                | SymbolKind::Struct
                | SymbolKind::Enum
                | SymbolKind::Trait
                | SymbolKind::Type
        )
    }) {
        symbol_map.insert(symbol.name.clone(), symbol);
    }

    match symbol_map.get(&target.terminal_name) {
        Some(type_symbol) => {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    container_symbol_id,
                    type_symbol.id,
                    RelationshipKind::Uses,
                    node.start_position().row
                ),
                from_symbol_id: container_symbol_id.to_string(),
                to_symbol_id: type_symbol.id.clone(),
                kind: RelationshipKind::Uses,
                file_path: extractor.get_base().file_path.clone(),
                line_number: node.start_position().row as u32 + 1,
                confidence: 0.9,
                metadata: None,
            });
        }
        None => {
            let pending = extractor.get_base().create_pending_relationship(
                container_symbol_id.to_string(),
                target,
                RelationshipKind::Uses,
                &node,
                Some(container_symbol_id.to_string()),
                Some(0.9),
            );
            extractor.add_structured_pending_relationship(pending);
        }
    }
}

fn extract_call_relationships(
    extractor: &mut VbNetExtractor,
    node: tree_sitter::Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let method_name = {
        let base = extractor.get_base();
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        if let Some(first_child) = children.first() {
            match first_child.kind() {
                "identifier" => base.get_node_text(first_child),
                "member_access_expression" | "member_access" => {
                    let mut mc = first_child.walk();
                    let inner: Vec<_> = first_child.children(&mut mc).collect();
                    inner
                        .iter()
                        .rev()
                        .find(|c| c.kind() == "identifier")
                        .map(|n| base.get_node_text(n))
                        .unwrap_or_default()
                }
                _ => String::new(),
            }
        } else {
            String::new()
        }
    };

    if method_name.is_empty() {
        return;
    }

    let base = extractor.get_base();
    let symbol_map: std::collections::HashMap<String, &Symbol> =
        symbols.iter().map(|s| (s.name.clone(), s)).collect();

    let mut parent = node.parent();
    let mut caller_symbol = None;
    while let Some(p) = parent {
        if p.kind() == "method_declaration" || p.kind() == "abstract_method_declaration" {
            if let Some(name_node) = p.child_by_field_name("name") {
                let mn = base.get_node_text(&name_node);
                caller_symbol = symbol_map.get(&mn).copied();
                break;
            }
        }
        parent = p.parent();
    }

    let Some(caller) = caller_symbol else {
        return;
    };

    let line_number = node.start_position().row as u32 + 1;
    let file_path = base.file_path.clone();

    match symbol_map.get(&method_name) {
        Some(called_symbol) if called_symbol.kind == SymbolKind::Import => {
            let pending = extractor.get_base().create_pending_relationship(
                caller.id.clone(),
                unresolved_call_target(extractor, node, &method_name),
                RelationshipKind::Calls,
                &node,
                Some(caller.id.clone()),
                Some(0.8),
            );
            extractor.add_structured_pending_relationship(pending);
        }
        Some(called_symbol) => {
            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    caller.id,
                    called_symbol.id,
                    RelationshipKind::Calls,
                    node.start_position().row
                ),
                from_symbol_id: caller.id.clone(),
                to_symbol_id: called_symbol.id.clone(),
                kind: RelationshipKind::Calls,
                file_path,
                line_number,
                confidence: 0.9,
                metadata: None,
            });
        }
        None => {
            let pending = extractor.get_base().create_pending_relationship(
                caller.id.clone(),
                unresolved_call_target(extractor, node, &method_name),
                RelationshipKind::Calls,
                &node,
                Some(caller.id.clone()),
                Some(0.7),
            );
            extractor.add_structured_pending_relationship(pending);
        }
    }
}

fn unresolved_call_target(
    extractor: &VbNetExtractor,
    node: tree_sitter::Node,
    fallback_name: &str,
) -> UnresolvedTarget {
    let callee_expression = match node.kind() {
        "invocation_expression" | "invocation" => {
            let mut cursor = node.walk();
            node.children(&mut cursor).next()
        }
        _ => Some(node),
    };

    let Some(callee_expression) = callee_expression else {
        return UnresolvedTarget::simple(fallback_name.to_string());
    };

    let mut identifiers = Vec::new();
    collect_identifiers(extractor, callee_expression, &mut identifiers);

    if identifiers.len() >= 2 {
        let terminal_name = identifiers
            .pop()
            .unwrap_or_else(|| fallback_name.to_string());
        let receiver = identifiers.pop();
        let namespace_path = identifiers;
        let mut display_parts = namespace_path.clone();
        if let Some(receiver_name) = receiver.as_ref() {
            display_parts.push(receiver_name.clone());
        }
        display_parts.push(terminal_name.clone());
        return UnresolvedTarget {
            display_name: display_parts.join("."),
            terminal_name,
            receiver,
            namespace_path,
            import_context: None,
        };
    }

    UnresolvedTarget::simple(fallback_name.to_string())
}

fn collect_identifiers(
    extractor: &VbNetExtractor,
    node: tree_sitter::Node,
    identifiers: &mut Vec<String>,
) {
    if node.kind() == "identifier" {
        identifiers.push(extractor.get_base().get_node_text(&node));
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_identifiers(extractor, child, identifiers);
    }
}
