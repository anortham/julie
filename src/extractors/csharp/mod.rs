// C# Language Extractor
//
// Direct Implementation of csharp-extractor.ts (1027 lines) to idiomatic Rust
//
// This extractor handles C#-specific constructs including:
// - Namespaces and using statements (regular, static, global)
// - Classes, interfaces, structs, and enums
// - Methods, constructors, and properties
// - Fields, events, and delegates
// - Records and nested types
// - Attributes and generics
// - Inheritance and implementation relationships
// - Modern C# features (nullable types, records, pattern matching)

mod helpers;
mod identifiers;
mod members;
mod operators;
mod relationships;
mod type_inference;
mod types;

use crate::extractors::base::{BaseExtractor, Identifier, Relationship, Symbol};
use std::collections::HashMap;
use tree_sitter::Tree;

/// C# extractor using tree-sitter-c-sharp parser
pub struct CSharpExtractor {
    base: BaseExtractor,
}

impl CSharpExtractor {
    /// Create new C# extractor
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

    /// Extract symbols from C# code - port of extractSymbols method
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.walk_tree(tree.root_node(), &mut symbols, None);
        symbols
    }

    /// Walk tree and extract symbols - port of walkTree method
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

        // Recursively process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree(child, symbols, current_parent_id.clone());
        }
    }

    /// Extract symbol from node - port of extractSymbol method
    fn extract_symbol(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<String>,
    ) -> Option<Symbol> {
        match node.kind() {
            "namespace_declaration" => types::extract_namespace(&mut self.base, node, parent_id),
            "using_directive" => types::extract_using(&mut self.base, node, parent_id),
            "class_declaration" => types::extract_class(&mut self.base, node, parent_id),
            "interface_declaration" => types::extract_interface(&mut self.base, node, parent_id),
            "struct_declaration" => types::extract_struct(&mut self.base, node, parent_id),
            "enum_declaration" => types::extract_enum(&mut self.base, node, parent_id),
            "enum_member_declaration" => {
                types::extract_enum_member(&mut self.base, node, parent_id)
            }
            "method_declaration" => members::extract_method(&mut self.base, node, parent_id),
            "constructor_declaration" => {
                members::extract_constructor(&mut self.base, node, parent_id)
            }
            "property_declaration" => members::extract_property(&mut self.base, node, parent_id),
            "field_declaration" => members::extract_field(&mut self.base, node, parent_id),
            "event_field_declaration" => members::extract_event(&mut self.base, node, parent_id),
            "delegate_declaration" => members::extract_delegate(&mut self.base, node, parent_id),
            "record_declaration" => types::extract_record(&mut self.base, node, parent_id),
            "destructor_declaration" => {
                members::extract_destructor(&mut self.base, node, parent_id)
            }
            "operator_declaration" => operators::extract_operator(&mut self.base, node, parent_id),
            "conversion_operator_declaration" => {
                operators::extract_conversion_operator(&mut self.base, node, parent_id)
            }
            "indexer_declaration" => operators::extract_indexer(&mut self.base, node, parent_id),
            _ => None,
        }
    }

    /// Extract relationships - port of extractRelationships
    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        relationships::extract_relationships(&self.base, tree, symbols)
    }

    /// Infer types - port of inferTypes
    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        type_inference::infer_types(symbols)
    }

    /// Extract all identifier usages (function calls, member access, etc.)
    /// Following the Rust extractor reference implementation pattern
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(&mut self.base, tree, symbols)
    }
}
