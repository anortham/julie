//! Kotlin Extractor
//!
//! Implementation of Kotlin extractor to idiomatic Rust.
//!
//! This extractor handles comprehensive Kotlin symbol extraction including:
//! - Classes, data classes, sealed classes, enums
//! - Objects, companion objects
//! - Functions, extension functions, operators
//! - Interfaces, type aliases, annotations
//! - Generics with variance
//! - Property delegation
//! - Constructor parameters

mod helpers;
mod identifiers;
mod properties;
mod relationships;
mod types;

use crate::extractors::base::{BaseExtractor, Identifier, Relationship, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

pub struct KotlinExtractor {
    base: BaseExtractor,
}

impl KotlinExtractor {
    pub fn new(
        language: String,
        file_path: String,
        content: String,
        workspace_root: &std::path::Path,
    ) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content, workspace_root),
        }
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.visit_node(tree.root_node(), &mut symbols, None);
        symbols
    }

    fn visit_node(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        if !node.is_named() {
            return; // Skip unnamed nodes
        }

        let mut symbol: Option<Symbol> = None;
        let mut new_parent_id = parent_id.clone();

        match node.kind() {
            "class_declaration" | "enum_declaration" => {
                symbol = Some(types::extract_class(
                    &mut self.base,
                    &node,
                    parent_id.as_deref(),
                ));
            }
            "interface_declaration" => {
                symbol = Some(types::extract_interface(
                    &mut self.base,
                    &node,
                    parent_id.as_deref(),
                ));
            }
            "object_declaration" => {
                symbol = Some(types::extract_object(
                    &mut self.base,
                    &node,
                    parent_id.as_deref(),
                ));
            }
            "companion_object" => {
                symbol = Some(types::extract_companion_object(
                    &mut self.base,
                    &node,
                    parent_id.as_deref(),
                ));
            }
            "function_declaration" => {
                symbol = Some(types::extract_function(
                    &mut self.base,
                    &node,
                    parent_id.as_deref(),
                ));
            }
            "property_declaration" | "property_signature" => {
                symbol = Some(properties::extract_property(
                    &mut self.base,
                    &node,
                    parent_id.as_deref(),
                ));
            }
            "enum_class_body" => {
                types::extract_enum_members(&mut self.base, &node, symbols, parent_id.as_deref());
            }
            "primary_constructor" => {
                properties::extract_constructor_parameters(
                    &mut self.base,
                    &node,
                    symbols,
                    parent_id.as_deref(),
                );
            }
            "package_header" => {
                symbol = Some(types::extract_package(
                    &mut self.base,
                    &node,
                    parent_id.as_deref(),
                ));
            }
            "import" => {
                symbol = Some(types::extract_import(
                    &mut self.base,
                    &node,
                    parent_id.as_deref(),
                ));
            }
            "type_alias" => {
                symbol = Some(types::extract_type_alias(
                    &mut self.base,
                    &node,
                    parent_id.as_deref(),
                ));
            }
            _ => {}
        }

        if let Some(ref sym) = symbol {
            symbols.push(sym.clone());
            new_parent_id = Some(sym.id.clone());
        }

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, new_parent_id.clone());
        }
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();
        for symbol in symbols {
            if let Some(serde_json::Value::String(s)) =
                symbol.metadata.as_ref().and_then(|m| m.get("returnType"))
            {
                types.insert(symbol.id.clone(), s.clone());
            } else if let Some(serde_json::Value::String(s)) =
                symbol.metadata.as_ref().and_then(|m| m.get("propertyType"))
            {
                types.insert(symbol.id.clone(), s.clone());
            } else if let Some(serde_json::Value::String(s)) =
                symbol.metadata.as_ref().and_then(|m| m.get("dataType"))
            {
                types.insert(symbol.id.clone(), s.clone());
            }
        }
        types
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
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
            "class_declaration"
            | "enum_declaration"
            | "object_declaration"
            | "interface_declaration" => {
                relationships::extract_inheritance_relationships(
                    &self.base,
                    &node,
                    symbols,
                    relationships,
                );
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node_for_relationships(child, symbols, relationships);
        }
    }

    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(&mut self.base, tree, symbols)
    }
}
