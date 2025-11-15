use crate::extractors::base::{BaseExtractor, Identifier, Relationship, Symbol};
use tree_sitter::{Node, Tree};

// Sub-modules
mod error_handling;
mod functions;
mod helpers;
mod identifiers;
mod relationships;
mod type_inference;
mod types;
mod variables;

pub struct ZigExtractor {
    base: BaseExtractor,
}

impl ZigExtractor {
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

    /// Main entry point for symbol extraction
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.visit_node(tree.root_node(), &mut symbols, None);
        symbols
    }

    /// Recursively visit AST nodes and extract symbols
    fn visit_node(
        &mut self,
        node: Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<String>,
    ) -> Option<String> {
        if node.kind().is_empty() {
            return parent_id;
        }

        let mut current_parent_id = parent_id.clone();

        if let Some(symbol) = self.extract_symbol_from_node(node, parent_id.as_ref()) {
            current_parent_id = Some(symbol.id.clone());
            symbols.push(symbol);
        }

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, current_parent_id.clone());
        }

        current_parent_id
    }

    /// Determine node type and delegate to appropriate extraction method
    fn extract_symbol_from_node(
        &mut self,
        node: Node,
        parent_id: Option<&String>,
    ) -> Option<Symbol> {
        match node.kind() {
            "function_declaration" | "function_definition" => functions::extract_function(
                &mut self.base,
                node,
                parent_id,
                helpers::is_public_function,
                helpers::is_export_function,
                helpers::is_inside_struct,
            ),
            "test_declaration" => functions::extract_test(&mut self.base, node, parent_id),
            "struct_declaration" => types::extract_struct(
                &mut self.base,
                node,
                parent_id,
                helpers::is_public_declaration,
            ),
            "union_declaration" => types::extract_union(
                &mut self.base,
                node,
                parent_id,
                helpers::is_public_declaration,
            ),
            "enum_declaration" => types::extract_enum(
                &mut self.base,
                node,
                parent_id,
                helpers::is_public_declaration,
            ),
            "variable_declaration" | "const_declaration" => variables::extract_variable(
                &mut self.base,
                node,
                parent_id,
                helpers::is_public_declaration,
            ),
            "error_declaration" => types::extract_error_type(&mut self.base, node, parent_id),
            "type_declaration" => types::extract_type_alias(
                &mut self.base,
                node,
                parent_id,
                helpers::is_public_declaration,
            ),
            "parameter" => functions::extract_parameter(&mut self.base, node, parent_id),
            "field_declaration" | "struct_field" | "container_field" => {
                types::extract_struct_field(&mut self.base, node, parent_id)
            }
            "enum_field" | "enum_variant" => {
                types::extract_enum_variant(&mut self.base, node, parent_id)
            }
            "ERROR" => error_handling::extract_from_error_node(&mut self.base, node, parent_id),
            _ => None,
        }
    }

    /// Extract relationships between symbols (calls, composition, etc.)
    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        relationships::extract_relationships(&mut self.base, tree, symbols)
    }

    /// Infer types from symbols using Zig-specific rules
    pub fn infer_types(&self, symbols: &[Symbol]) -> std::collections::HashMap<String, String> {
        type_inference::infer_types(symbols)
    }

    /// Extract identifier usages for LSP-quality find_references
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(&mut self.base, tree, symbols)
    }
}
