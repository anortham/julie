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

pub(crate) mod di_relationships;
mod fields;
mod helpers;
mod identifiers;
mod local_callables;
pub(crate) mod member_type_relationships;
mod members;
mod operators;
mod partial_classes;
mod relationships;
mod type_inference;
mod types;

use crate::base::{
    BaseExtractor, Identifier, PendingRelationship, Relationship, StructuredPendingRelationship,
    Symbol, SymbolKind,
};
use std::collections::HashMap;
use tree_sitter::Tree;

/// C# extractor using tree-sitter-c-sharp parser
pub struct CSharpExtractor {
    base: BaseExtractor,
}

impl CSharpExtractor {
    fn has_top_level_global_statement(root: tree_sitter::Node) -> bool {
        // Global statements are always direct children of the compilation_unit root
        // in C# — no need to recurse into classes or namespaces.
        let mut cursor = root.walk();
        root.children(&mut cursor)
            .any(|c| c.kind() == "global_statement")
    }

    fn ensure_file_scope_symbol(&mut self, root: tree_sitter::Node, symbols: &mut Vec<Symbol>) {
        if !Self::has_top_level_global_statement(root) {
            return;
        }

        let file_symbol_id = format!("file::{}", self.base.file_path);
        if symbols.iter().any(|s| s.id == file_symbol_id) {
            return;
        }

        let start_pos = root.start_position();
        let end_pos = root.end_position();
        let file_symbol = Symbol {
            id: file_symbol_id.clone(),
            name: self.base.file_path.clone(),
            kind: SymbolKind::Module,
            language: self.base.language.clone(),
            file_path: self.base.file_path.clone(),
            start_line: (start_pos.row + 1) as u32,
            start_column: start_pos.column as u32,
            end_line: (end_pos.row + 1) as u32,
            end_column: end_pos.column as u32,
            start_byte: root.start_byte() as u32,
            end_byte: root.end_byte() as u32,
            body_span: None,
            body_hash: None,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: Some(1.0),
            code_context: None,
            content_type: None,
            annotations: Vec::new(),
        };

        self.base
            .symbol_map
            .insert(file_symbol_id, file_symbol.clone());
        symbols.push(file_symbol);
    }

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

    /// Get immutable reference to base extractor
    pub(crate) fn get_base(&self) -> &BaseExtractor {
        &self.base
    }

    /// Extract symbols from C# code - port of extractSymbols method
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let root = tree.root_node();
        self.walk_tree(root, &mut symbols, None);
        self.ensure_file_scope_symbol(root, &mut symbols);
        symbols
    }

    /// Walk tree and extract symbols - port of walkTree method
    fn walk_tree(
        &mut self,
        node: tree_sitter::Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<String>,
    ) {
        if node.kind() == "field_declaration" {
            let field_symbols = fields::extract_fields(&mut self.base, node, parent_id.clone());
            let current_parent_id = field_symbols
                .first()
                .map(|symbol| symbol.id.clone())
                .or(parent_id);
            symbols.extend(field_symbols);

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.walk_tree(child, symbols, current_parent_id.clone());
            }
            return;
        }

        if node.kind() == "event_field_declaration" {
            let event_symbols = fields::extract_events(&mut self.base, node, parent_id.clone());
            let current_parent_id = event_symbols
                .first()
                .map(|symbol| symbol.id.clone())
                .or(parent_id);
            symbols.extend(event_symbols);

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.walk_tree(child, symbols, current_parent_id.clone());
            }
            return;
        }

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
            "local_function_statement" => {
                local_callables::extract_local_function(&mut self.base, node, parent_id)
            }
            "constructor_declaration" => {
                members::extract_constructor(&mut self.base, node, parent_id)
            }
            "property_declaration" => members::extract_property(&mut self.base, node, parent_id),
            "field_declaration" => fields::extract_field(&mut self.base, node, parent_id),
            "event_field_declaration" => fields::extract_event(&mut self.base, node, parent_id),
            "delegate_declaration" => members::extract_delegate(&mut self.base, node, parent_id),
            "record_declaration" => types::extract_record(&mut self.base, node, parent_id),
            "lambda_expression" | "anonymous_method_expression" => {
                local_callables::extract_lambda(&mut self.base, node, parent_id)
            }
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
        relationships::extract_relationships(self, tree, symbols)
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
