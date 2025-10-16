use crate::extractors::base::{Relationship, RelationshipKind, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

/// Relationship extraction for Go (method receivers, interface implementations, embedding)
impl super::GoExtractor {
    pub(super) fn walk_tree_for_relationships(
        &self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
        relationships: &mut Vec<Relationship>,
    ) {
        // Handle interface implementations (implicit in Go)
        if node.kind() == "method_declaration" {
            self.extract_method_relationships_from_node(node, symbol_map, relationships);
        }

        // Handle struct embedding
        if node.kind() == "struct_type" {
            self.extract_embedding_relationships(node, symbol_map, relationships);
        }

        // Recursively process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_relationships(child, symbol_map, relationships);
        }
    }

    pub(super) fn extract_method_relationships_from_node(
        &self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
        relationships: &mut Vec<Relationship>,
    ) {
        // Extract method to receiver type relationship
        let receiver_list = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "parameter_list");
        if let Some(receiver_list) = receiver_list {
            let param_decl = receiver_list
                .children(&mut receiver_list.walk())
                .find(|c| c.kind() == "parameter_declaration");
            if let Some(param_decl) = param_decl {
                // Extract receiver type
                let receiver_type = self.extract_receiver_type_from_param(param_decl);
                let receiver_symbol = symbol_map.get(&receiver_type);

                let name_node = node
                    .children(&mut node.walk())
                    .find(|c| c.kind() == "field_identifier");
                if let Some(name_node) = name_node {
                    let method_name = self.get_node_text(name_node);
                    let method_symbol = symbol_map.get(&method_name);

                    if let (Some(receiver_sym), Some(method_sym)) = (receiver_symbol, method_symbol)
                    {
                        // Create Uses relationship from method to receiver type
                        relationships.push(self.base.create_relationship(
                            method_sym.id.clone(),
                            receiver_sym.id.clone(),
                            RelationshipKind::Uses,
                            &node,
                            Some(0.9),
                            None,
                        ));
                    }
                }
            }
        }
    }

    pub(super) fn extract_embedding_relationships(
        &self,
        _node: Node,
        _symbol_map: &HashMap<String, &Symbol>,
        _relationships: &mut Vec<Relationship>,
    ) {
        // Go struct embedding creates implicit relationships
        // This would need more complex parsing to detect embedded types
        // For now, we'll skip this advanced feature
    }
}
