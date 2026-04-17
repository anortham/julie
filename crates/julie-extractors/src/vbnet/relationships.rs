use super::helpers;
use crate::base::{PendingRelationship, Relationship, RelationshipKind, Symbol, SymbolKind};
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
            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: current_symbol_id.clone(),
                callee_name: base_type_name,
                kind: RelationshipKind::Extends,
                file_path: file_path.clone(),
                line_number,
                confidence: 0.9,
            });
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
            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: current_symbol_id.clone(),
                callee_name: impl_type_name,
                kind: RelationshipKind::Implements,
                file_path: file_path.clone(),
                line_number,
                confidence: 0.9,
            });
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
            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: current_symbol_id.clone(),
                callee_name: base_type_name,
                kind: RelationshipKind::Extends,
                file_path: file_path.clone(),
                line_number,
                confidence: 0.9,
            });
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
            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: caller.id.clone(),
                callee_name: method_name,
                kind: RelationshipKind::Calls,
                file_path,
                line_number,
                confidence: 0.8,
            });
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
            extractor.add_pending_relationship(PendingRelationship {
                from_symbol_id: caller.id.clone(),
                callee_name: method_name,
                kind: RelationshipKind::Calls,
                file_path,
                line_number,
                confidence: 0.7,
            });
        }
    }
}
