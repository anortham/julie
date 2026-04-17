mod helpers;
mod identifiers;
mod members;
mod relationships;
mod type_inference;
mod types;

use crate::base::{
    BaseExtractor, Identifier, PendingRelationship, Relationship, StructuredPendingRelationship,
    Symbol,
};
use std::collections::HashMap;
use tree_sitter::Tree;

pub struct VbNetExtractor {
    base: BaseExtractor,
    pending_relationships: Vec<PendingRelationship>,
    structured_pending_relationships: Vec<StructuredPendingRelationship>,
}

impl VbNetExtractor {
    pub fn new(
        language: String,
        file_path: String,
        content: String,
        workspace_root: &std::path::Path,
    ) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content, workspace_root),
            pending_relationships: Vec::new(),
            structured_pending_relationships: Vec::new(),
        }
    }

    pub fn get_pending_relationships(&self) -> Vec<PendingRelationship> {
        self.pending_relationships.clone()
    }

    pub fn add_pending_relationship(&mut self, pending: PendingRelationship) {
        self.pending_relationships.push(pending);
    }

    pub fn add_structured_pending_relationship(&mut self, pending: StructuredPendingRelationship) {
        self.pending_relationships.push(pending.pending.clone());
        self.structured_pending_relationships.push(pending);
    }

    pub fn get_structured_pending_relationships(&self) -> Vec<StructuredPendingRelationship> {
        self.structured_pending_relationships.clone()
    }

    pub(crate) fn get_base(&self) -> &BaseExtractor {
        &self.base
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        self.walk_tree(root, &mut symbols, None);
        symbols
    }

    fn walk_tree(
        &mut self,
        node: tree_sitter::Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<String>,
    ) {
        let symbol = self.extract_symbol(node, parent_id.clone());
        let current_parent_id = if let Some(ref sym) = symbol {
            symbols.push(sym.clone());
            Some(sym.id.clone())
        } else {
            parent_id
        };

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree(child, symbols, current_parent_id.clone());
        }
    }

    fn extract_symbol(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<String>,
    ) -> Option<Symbol> {
        match node.kind() {
            "namespace_block" => types::extract_namespace(&mut self.base, node, parent_id),
            "imports_statement" => types::extract_imports(&mut self.base, node, parent_id),
            "class_block" => types::extract_class(&mut self.base, node, parent_id),
            "module_block" => types::extract_module(&mut self.base, node, parent_id),
            "structure_block" => types::extract_structure(&mut self.base, node, parent_id),
            "interface_block" => types::extract_interface(&mut self.base, node, parent_id),
            "enum_block" => types::extract_enum(&mut self.base, node, parent_id),
            "enum_member" => types::extract_enum_member(&mut self.base, node, parent_id),
            "delegate_declaration" => types::extract_delegate(&mut self.base, node, parent_id),
            "method_declaration" => members::extract_method(&mut self.base, node, parent_id),
            "abstract_method_declaration" => {
                members::extract_abstract_method(&mut self.base, node, parent_id)
            }
            "constructor_declaration" => {
                members::extract_constructor(&mut self.base, node, parent_id)
            }
            "property_declaration" => members::extract_property(&mut self.base, node, parent_id),
            "field_declaration" => members::extract_field(&mut self.base, node, parent_id),
            "event_declaration" => members::extract_event(&mut self.base, node, parent_id),
            "operator_declaration" => members::extract_operator(&mut self.base, node, parent_id),
            "const_declaration" => members::extract_const(&mut self.base, node, parent_id),
            "declare_statement" => members::extract_declare(&mut self.base, node, parent_id),
            _ => None,
        }
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        relationships::extract_relationships(self, tree, symbols)
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        type_inference::infer_types(symbols)
    }

    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(&mut self.base, tree, symbols)
    }
}
