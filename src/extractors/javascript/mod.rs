//! JavaScript Extractor for Julie
//!
//! Direct Implementation of JavaScript extractor logic ported to idiomatic Rust
//!
//! This follows the exact extraction strategy using Rust patterns:
//! - Uses node type switch statement logic
//! - Preserves signature building algorithms
//! - Maintains same edge case handling
//! - Converts to Rust Option<T>, Result<T>, iterators, ownership system

mod assignments;
mod functions;
mod helpers;
mod identifiers;
mod imports;
mod relationships;
mod signatures;
mod types;
mod variables;
mod visibility;

use crate::extractors::base::{BaseExtractor, Relationship, Symbol};
use tree_sitter::Tree;

pub struct JavaScriptExtractor {
    base: BaseExtractor,
}

impl JavaScriptExtractor {
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

    /// Access base extractor (needed by relationship module)
    pub(super) fn base(&self) -> &BaseExtractor {
        &self.base
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.visit_node(tree.root_node(), &mut symbols, None);
        symbols
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        relationships::extract_relationships(self, tree, symbols)
    }

    /// Infer types from JSDoc comments (@returns, @type)
    pub fn infer_types(&self, symbols: &[Symbol]) -> std::collections::HashMap<String, String> {
        let mut type_map = std::collections::HashMap::new();

        for symbol in symbols {
            if let Some(ref doc_comment) = symbol.doc_comment {
                // Extract type from JSDoc
                if let Some(inferred_type) = self.extract_jsdoc_type(doc_comment, &symbol.kind) {
                    type_map.insert(symbol.id.clone(), inferred_type);
                }
            }
        }

        type_map
    }

    fn extract_jsdoc_type(&self, doc_comment: &str, kind: &crate::extractors::base::SymbolKind) -> Option<String> {
        use crate::extractors::base::SymbolKind;

        match kind {
            SymbolKind::Function | SymbolKind::Method => {
                // Extract return type from @returns {Type} or @return {Type}
                if let Some(captures) = regex::Regex::new(r"@returns?\s*\{([^}]+)\}")
                    .ok()?
                    .captures(doc_comment)
                {
                    return Some(captures[1].trim().to_string());
                }
            }
            SymbolKind::Variable | SymbolKind::Property => {
                // Extract type from @type {Type}
                if let Some(captures) = regex::Regex::new(r"@type\s*\{([^}]+)\}")
                    .ok()?
                    .captures(doc_comment)
                {
                    return Some(captures[1].trim().to_string());
                }
            }
            _ => {}
        }

        None
    }

    /// Main tree traversal - ports visitNode function exactly
    fn visit_node(
        &mut self,
        node: tree_sitter::Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<String>,
    ) {
        let mut symbol: Option<Symbol> = None;

        // Port switch statement exactly
        match node.kind() {
            "class_declaration" => {
                symbol = Some(self.extract_class(node, parent_id.clone()));
            }
            "function_declaration"
            | "function"
            | "arrow_function"
            | "function_expression"
            | "generator_function"
            | "generator_function_declaration" => {
                symbol = Some(self.extract_function(node, parent_id.clone()));
            }
            "method_definition" => {
                symbol = Some(self.extract_method(node, parent_id.clone()));
            }
            "variable_declarator" => {
                // Handle destructuring patterns that create multiple symbols (reference logic)
                let name_node = node.child_by_field_name("name");
                if let Some(name) = name_node {
                    if name.kind() == "object_pattern" || name.kind() == "array_pattern" {
                        let destructured_symbols =
                            self.extract_destructuring_variables(node, parent_id.clone());
                        symbols.extend(destructured_symbols);
                    } else {
                        symbol = Some(self.extract_variable(node, parent_id.clone()));
                    }
                } else {
                    symbol = Some(self.extract_variable(node, parent_id.clone()));
                }
            }
            "import_statement" | "import_declaration" => {
                // Handle multiple import specifiers (reference logic)
                let import_symbols = self.extract_import_specifiers(&node);
                for specifier in import_symbols {
                    let import_symbol =
                        self.create_import_symbol(node, &specifier, parent_id.clone());
                    symbols.push(import_symbol);
                }
            }
            "export_statement" | "export_declaration" => {
                symbol = Some(self.extract_export(node, parent_id.clone()));
            }
            "property_definition" | "public_field_definition" | "field_definition" | "pair" => {
                symbol = Some(self.extract_property(node, parent_id.clone()));
            }
            "assignment_expression" => {
                if let Some(assignment_symbol) = self.extract_assignment(node, parent_id.clone()) {
                    symbol = Some(assignment_symbol);
                }
            }
            _ => {}
        }

        let current_parent_id = if let Some(sym) = &symbol {
            symbols.push(sym.clone());
            Some(sym.id.clone())
        } else {
            parent_id
        };

        // Recursively visit children (pattern)
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, current_parent_id.clone());
        }
    }
}
