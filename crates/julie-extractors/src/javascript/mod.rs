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

use crate::base::{BaseExtractor, PendingRelationship, Relationship, Symbol};
use tree_sitter::Tree;

pub struct JavaScriptExtractor {
    base: BaseExtractor,
    /// Pending relationships that need cross-file resolution after workspace indexing
    pending_relationships: Vec<PendingRelationship>,
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
            pending_relationships: Vec::new(),
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
        let rels = relationships::extract_relationships(self, tree, symbols);
        // Extract pending relationships (cross-file calls) and add them to our internal list
        self.extract_pending_relationships(tree, symbols);
        rels
    }

    /// Extract pending relationships from the syntax tree
    /// This handles cross-file function calls that need resolution
    fn extract_pending_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) {
        let symbol_map: std::collections::HashMap<String, &Symbol> =
            symbols.iter().map(|s| (s.name.clone(), s)).collect();

        self.walk_for_pending_calls(tree.root_node(), &symbol_map);
    }

    /// Walk the tree looking for function calls that reference imported symbols
    fn walk_for_pending_calls(&mut self, node: tree_sitter::Node, symbol_map: &std::collections::HashMap<String, &Symbol>) {
        // Look for call expressions
        if node.kind() == "call_expression" {
            if let Some(function_node) = node.child_by_field_name("function") {
                // Extract function name - handle both direct calls and member access
                let function_name = if function_node.kind() == "member_expression" {
                    // For obj.method(), get just "method"
                    if let Some(property) = function_node.child_by_field_name("property") {
                        self.base.get_node_text(&property)
                    } else {
                        self.base.get_node_text(&function_node)
                    }
                } else {
                    self.base.get_node_text(&function_node)
                };

                // Check if this is a call to an import or unknown function
                match symbol_map.get(function_name.as_str()) {
                    Some(called_symbol) if called_symbol.kind == crate::base::SymbolKind::Import => {
                        // This is a call to an imported function - create pending relationship
                        // Find the containing function
                        if let Some(caller_symbol) = self.find_containing_function_in_symbols(node, symbol_map) {
                            let line_number = node.start_position().row as u32 + 1;
                            self.add_pending_relationship(PendingRelationship {
                                from_symbol_id: caller_symbol.id.clone(),
                                callee_name: function_name.clone(),
                                kind: crate::base::RelationshipKind::Calls,
                                file_path: self.base.file_path.clone(),
                                line_number,
                                confidence: 0.8,
                            });
                        }
                    }
                    None => {
                        // Unknown function - could be from another file
                        // Check if it's being called from within a function
                        if let Some(caller_symbol) = self.find_containing_function_in_symbols(node, symbol_map) {
                            let line_number = node.start_position().row as u32 + 1;
                            self.add_pending_relationship(PendingRelationship {
                                from_symbol_id: caller_symbol.id.clone(),
                                callee_name: function_name.clone(),
                                kind: crate::base::RelationshipKind::Calls,
                                file_path: self.base.file_path.clone(),
                                line_number,
                                confidence: 0.7,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        // Recursively process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_for_pending_calls(child, symbol_map);
        }
    }

    /// Find the containing function for a node by walking up the tree
    fn find_containing_function_in_symbols<'a>(
        &self,
        node: tree_sitter::Node,
        symbol_map: &'a std::collections::HashMap<String, &'a Symbol>,
    ) -> Option<&'a Symbol> {
        let mut current = node.parent();

        while let Some(current_node) = current {
            // Check for function declarations
            if current_node.kind() == "function_declaration"
                || current_node.kind() == "method_definition"
                || current_node.kind() == "arrow_function"
            {
                // Get the function name
                if let Some(name_node) = current_node.child_by_field_name("name") {
                    let func_name = self.base.get_node_text(&name_node);
                    if let Some(symbol) = symbol_map.get(&func_name) {
                        if matches!(symbol.kind, crate::base::SymbolKind::Function | crate::base::SymbolKind::Method) {
                            return Some(symbol);
                        }
                    }
                }
            }

            current = current_node.parent();
        }

        None
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

    fn extract_jsdoc_type(&self, doc_comment: &str, kind: &crate::base::SymbolKind) -> Option<String> {
        use crate::base::SymbolKind;

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
                symbol = self.extract_class(node, parent_id.clone());
            }
            "function_declaration"
            | "function"
            | "arrow_function"
            | "function_expression"
            | "generator_function"
            | "generator_function_declaration" => {
                symbol = self.extract_function(node, parent_id.clone());
            }
            "method_definition" => {
                symbol = self.extract_method(node, parent_id.clone());
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
                        symbol = self.extract_variable(node, parent_id.clone());
                    }
                } else {
                    symbol = self.extract_variable(node, parent_id.clone());
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
                symbol = self.extract_export(node, parent_id.clone());
            }
            "property_definition" | "public_field_definition" | "field_definition" | "pair" => {
                symbol = self.extract_property(node, parent_id.clone());
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

    /// Get pending relationships that need cross-file resolution
    pub fn get_pending_relationships(&self) -> Vec<PendingRelationship> {
        self.pending_relationships.clone()
    }

    /// Add a pending relationship (used during extraction)
    pub fn add_pending_relationship(&mut self, pending: PendingRelationship) {
        self.pending_relationships.push(pending);
    }
}
