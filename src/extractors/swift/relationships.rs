use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind};
use serde_json;
use std::collections::HashMap;
use tree_sitter::Node;

use super::SwiftExtractor;

/// Extracts inheritance and protocol conformance relationships between Swift types
impl SwiftExtractor {
    /// Extract relationships between Swift types (inheritance and protocol conformance)
    /// Implementation of extractRelationships method
    pub fn extract_relationships(
        &mut self,
        tree: &tree_sitter::Tree,
        symbols: &[Symbol],
    ) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.visit_node_for_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    fn visit_node_for_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        match node.kind() {
            "class_declaration" | "struct_declaration" | "extension_declaration" => {
                self.extract_inheritance_relationships(node, symbols, relationships);
            }
            _ => {}
        }

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node_for_relationships(child, symbols, relationships);
        }
    }

    /// Implementation of extractInheritanceRelationships method
    fn extract_inheritance_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        if let Some(type_symbol) = self.find_type_symbol(node, symbols) {
            // Try type_inheritance_clause first
            if let Some(inheritance) = node
                .children(&mut node.walk())
                .find(|c| c.kind() == "type_inheritance_clause")
            {
                for child in inheritance.children(&mut inheritance.walk()) {
                    if matches!(child.kind(), "type_identifier" | "type") {
                        let base_type_name = self.base.get_node_text(&child);
                        self.add_inheritance_relationship(
                            &type_symbol,
                            &base_type_name,
                            symbols,
                            relationships,
                            node,
                        );
                    }
                }
            }

            // Also handle direct inheritance_specifier nodes
            for spec in node
                .children(&mut node.walk())
                .filter(|c| c.kind() == "inheritance_specifier")
            {
                if let Some(type_node) = spec
                    .children(&mut spec.walk())
                    .find(|c| matches!(c.kind(), "user_type" | "type_identifier" | "type"))
                {
                    let base_type_name = if type_node.kind() == "user_type" {
                        if let Some(inner_type_node) = type_node
                            .children(&mut type_node.walk())
                            .find(|c| c.kind() == "type_identifier")
                        {
                            self.base.get_node_text(&inner_type_node)
                        } else {
                            self.base.get_node_text(&type_node)
                        }
                    } else {
                        self.base.get_node_text(&type_node)
                    };
                    self.add_inheritance_relationship(
                        &type_symbol,
                        &base_type_name,
                        symbols,
                        relationships,
                        node,
                    );
                }
            }
        }
    }

    /// Implementation of addInheritanceRelationship method
    fn add_inheritance_relationship(
        &self,
        type_symbol: &Symbol,
        base_type_name: &str,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
        node: Node,
    ) {
        // Find the actual base type symbol
        if let Some(base_type_symbol) = symbols.iter().find(|s| {
            s.name == base_type_name
                && matches!(
                    s.kind,
                    SymbolKind::Class | SymbolKind::Interface | SymbolKind::Struct
                )
        }) {
            // Determine relationship kind: classes extend, protocols implement
            let relationship_kind = if base_type_symbol.kind == SymbolKind::Interface {
                RelationshipKind::Implements
            } else {
                RelationshipKind::Extends
            };

            let metadata = HashMap::from([(
                "baseType".to_string(),
                serde_json::Value::String(base_type_name.to_string()),
            )]);

            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    type_symbol.id,
                    base_type_symbol.id,
                    relationship_kind,
                    node.start_position().row
                ),
                from_symbol_id: type_symbol.id.clone(),
                to_symbol_id: base_type_symbol.id.clone(),
                kind: relationship_kind,
                file_path: self.base.file_path.clone(),
                line_number: (node.start_position().row + 1) as u32,
                confidence: 1.0,
                metadata: Some(metadata),
            });
        }
    }

    /// Implementation of infer_types method
    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();
        for symbol in symbols {
            // For functions/methods, prefer returnType over generic type
            if matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method) {
                if let Some(return_type) =
                    symbol.metadata.as_ref().and_then(|m| m.get("returnType"))
                {
                    if let Some(return_type_str) = return_type.as_str() {
                        types.insert(symbol.id.clone(), return_type_str.to_string());
                        continue;
                    }
                }
            }
            // For properties/variables, prefer propertyType or variableType
            else if matches!(symbol.kind, SymbolKind::Property | SymbolKind::Variable) {
                if let Some(property_type) =
                    symbol.metadata.as_ref().and_then(|m| m.get("propertyType"))
                {
                    if let Some(property_type_str) = property_type.as_str() {
                        types.insert(symbol.id.clone(), property_type_str.to_string());
                        continue;
                    }
                }
                if let Some(variable_type) =
                    symbol.metadata.as_ref().and_then(|m| m.get("variableType"))
                {
                    if let Some(variable_type_str) = variable_type.as_str() {
                        types.insert(symbol.id.clone(), variable_type_str.to_string());
                        continue;
                    }
                }
            }

            // Fallback to generic type from metadata
            if let Some(symbol_type) = symbol.metadata.as_ref().and_then(|m| m.get("type")) {
                if let Some(symbol_type_str) = symbol_type.as_str() {
                    types.insert(symbol.id.clone(), symbol_type_str.to_string());
                }
            } else if let Some(return_type) =
                symbol.metadata.as_ref().and_then(|m| m.get("returnType"))
            {
                if let Some(return_type_str) = return_type.as_str() {
                    types.insert(symbol.id.clone(), return_type_str.to_string());
                }
            }
        }
        types
    }

    /// Implementation of findTypeSymbol method
    pub(super) fn find_type_symbol(&self, node: Node, symbols: &[Symbol]) -> Option<Symbol> {
        if let Some(name_node) = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier")
        {
            let type_name = self.base.get_node_text(&name_node);
            symbols
                .iter()
                .find(|s| {
                    s.name == type_name
                        && matches!(
                            s.kind,
                            SymbolKind::Class | SymbolKind::Struct | SymbolKind::Interface
                        )
                        && s.file_path == self.base.file_path
                })
                .cloned()
        } else {
            None
        }
    }
}
