//! Scala Extractor
//!
//! Comprehensive Scala symbol extraction including:
//! - Classes, case classes, abstract classes, sealed classes
//! - Traits, objects, companion objects
//! - Functions/methods, vals, vars
//! - Enums (Scala 3), type aliases
//! - Given instances, extension methods
//! - Imports, packages

mod declarations;
mod helpers;
mod identifiers;
mod properties;
mod relationships;
mod types;

use crate::base::{BaseExtractor, Identifier, PendingRelationship, Relationship, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

pub struct ScalaExtractor {
    base: BaseExtractor,
    /// Pending relationships that need cross-file resolution after workspace indexing
    pending_relationships: Vec<PendingRelationship>,
}

impl ScalaExtractor {
    pub fn new(
        language: String,
        file_path: String,
        content: String,
        workspace_root: &std::path::Path,
    ) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content, workspace_root),
            pending_relationships: Vec::new(),
        }
    }

    /// Get pending relationships that need cross-file resolution
    pub fn get_pending_relationships(&self) -> Vec<PendingRelationship> {
        self.pending_relationships.clone()
    }

    /// Add a pending relationship (used during extraction)
    pub fn add_pending_relationship(&mut self, pending: PendingRelationship) {
        self.pending_relationships.push(pending);
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.visit_node(tree.root_node(), &mut symbols, None);
        symbols
    }

    fn visit_node(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        if !node.is_named() {
            return;
        }

        let mut symbol: Option<Symbol> = None;
        let mut new_parent_id = parent_id.clone();

        match node.kind() {
            "class_definition" => {
                symbol = types::extract_class(&mut self.base, &node, parent_id.as_deref());
            }
            "trait_definition" => {
                symbol = types::extract_trait(&mut self.base, &node, parent_id.as_deref());
            }
            "object_definition" => {
                symbol =
                    types::extract_object(&mut self.base, &node, symbols, parent_id.as_deref());
            }
            "enum_definition" => {
                symbol = types::extract_enum(&mut self.base, &node, parent_id.as_deref());
            }
            "simple_enum_case" | "full_enum_case" => {
                symbol = types::extract_enum_case(&mut self.base, &node, parent_id.as_deref());
            }
            "function_definition" | "function_declaration" => {
                symbol =
                    declarations::extract_function(&mut self.base, &node, parent_id.as_deref());
            }
            "val_definition" | "val_declaration" => {
                symbol = properties::extract_val(&mut self.base, &node, parent_id.as_deref());
            }
            "var_definition" | "var_declaration" => {
                symbol = properties::extract_var(&mut self.base, &node, parent_id.as_deref());
            }
            "import_declaration" => {
                symbol =
                    declarations::extract_import(&mut self.base, &node, parent_id.as_deref());
            }
            "package_clause" => {
                symbol =
                    declarations::extract_package(&mut self.base, &node, parent_id.as_deref());
            }
            "type_definition" => {
                symbol =
                    declarations::extract_type_alias(&mut self.base, &node, parent_id.as_deref());
            }
            "given_definition" => {
                symbol =
                    declarations::extract_given(&mut self.base, &node, parent_id.as_deref());
            }
            "extension_definition" => {
                symbol =
                    declarations::extract_extension(&mut self.base, &node, parent_id.as_deref());
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
        &mut self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        match node.kind() {
            "class_definition" | "trait_definition" | "object_definition"
            | "enum_definition" => {
                relationships::extract_inheritance_relationships(
                    self,
                    &node,
                    symbols,
                    relationships,
                );
                relationships::extract_call_relationships(self, node, symbols, relationships);
            }
            "function_definition" | "function_declaration" => {
                relationships::extract_call_relationships(self, node, symbols, relationships);
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

    // ========================================================================
    // Accessors for sub-modules
    // ========================================================================

    pub(crate) fn base(&self) -> &BaseExtractor {
        &self.base
    }
}
