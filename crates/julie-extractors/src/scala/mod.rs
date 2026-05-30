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
mod test_calls;
mod types;

use crate::base::{
    BaseExtractor, Identifier, PendingRelationship, Relationship, StructuredPendingRelationship,
    Symbol,
};
use std::collections::{HashMap, HashSet};
use tree_sitter::{Node, Tree};

pub struct ScalaExtractor {
    base: BaseExtractor,
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
        }
    }

    /// Get pending relationships that need cross-file resolution
    pub fn get_pending_relationships(&self) -> Vec<PendingRelationship> {
        self.base.get_pending_relationships()
    }

    pub fn get_type_argument_usages(&self) -> Vec<crate::base::TypeArgumentUsage> {
        self.base.get_type_argument_usages()
    }

    /// Clone captured call-argument literals (Miller bridge Phase 3).
    pub fn get_literals(&self) -> Vec<crate::base::Literal> {
        self.base.get_literals()
    }

    pub fn get_structured_pending_relationships(&self) -> Vec<StructuredPendingRelationship> {
        self.base.get_structured_pending_relationships()
    }

    /// Add a pending relationship (used during extraction)
    pub fn add_pending_relationship(&mut self, pending: PendingRelationship) {
        self.base.add_pending_relationship(pending);
    }

    pub fn add_structured_pending_relationship(&mut self, pending: StructuredPendingRelationship) {
        self.base.add_structured_pending_relationship(pending);
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
        let mut class_symbol_id: Option<String> = None;

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
                symbol = declarations::extract_import(&mut self.base, &node, parent_id.as_deref());
            }
            "package_clause" => {
                symbol = declarations::extract_package(&mut self.base, &node, parent_id.as_deref());
            }
            "type_definition" => {
                symbol =
                    declarations::extract_type_alias(&mut self.base, &node, parent_id.as_deref());
            }
            "given_definition" => {
                symbol = declarations::extract_given(&mut self.base, &node, parent_id.as_deref());
            }
            "extension_definition" => {
                symbol =
                    declarations::extract_extension(&mut self.base, &node, parent_id.as_deref());
            }
            // ScalaTest / MUnit call-style tests (Miller bridge Wave-3). Curried
            // call form `test("n") { }` / `describe(...) { it(...) }`, and FlatSpec
            // infix form `"subject" should "behaviour" in { }`. Both return None
            // for non-test nodes, so ordinary calls/infix fall through untouched.
            "call_expression" => {
                symbol = test_calls::extract_scala_test_call(
                    &mut self.base,
                    &node,
                    parent_id.as_deref(),
                );
            }
            "infix_expression" => {
                symbol = test_calls::extract_scala_flatspec_test(
                    &mut self.base,
                    &node,
                    parent_id.as_deref(),
                );
            }
            _ => {}
        }

        if let Some(ref sym) = symbol {
            symbols.push(sym.clone());
            new_parent_id = Some(sym.id.clone());
            if node.kind() == "class_definition" {
                class_symbol_id = Some(sym.id.clone());
            }
        }

        if let Some(class_id) = class_symbol_id {
            properties::extract_case_class_constructor_fields(
                &mut self.base,
                &node,
                symbols,
                Some(class_id.as_str()),
            );
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
        dedupe_relationships(&mut relationships);
        relationships
    }

    fn visit_node_for_relationships(
        &mut self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        match node.kind() {
            "class_definition" | "trait_definition" | "object_definition" | "enum_definition" => {
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
            "val_definition" | "var_definition" | "given_definition" | "extension_definition" => {
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

fn dedupe_relationships(relationships: &mut Vec<Relationship>) {
    let mut seen = HashSet::new();
    relationships.retain(|relationship| seen.insert(relationship.id.clone()));
}
