//! TypeScript/JavaScript symbol extractor with modular architecture
//!
//! This module provides comprehensive symbol extraction for TypeScript, JavaScript, and TSX/JSX files.
//! The architecture is organized into specialized modules for clarity and maintainability:
//!
//! - **symbols**: Core symbol extraction logic for classes, functions, interfaces, etc.
//! - **functions**: Function and method extraction with signature building
//! - **classes**: Class extraction with inheritance and modifiers
//! - **interfaces**: Interface and type alias extraction
//! - **imports_exports**: Import/export statement extraction
//! - **relationships**: Function call and inheritance relationship tracking
//! - **inference**: Type inference from assignments and return statements
//! - **identifiers**: Identifier usage extraction (calls, member access, etc.)
//! - **helpers**: Utility functions for tree traversal and text extraction

mod classes;
mod functions;
mod helpers;
mod identifiers;
mod imports_exports;
pub mod inference;
mod interfaces;
pub(crate) mod relationships;
mod symbols;

use crate::base::{
    BaseExtractor, Identifier, PendingRelationship, Relationship, RelationshipKind,
    StructuredPendingRelationship, Symbol, SymbolKind, UnresolvedTarget,
};
use std::collections::HashMap;
use tree_sitter::Tree;

/// Main TypeScript extractor that orchestrates modular extraction components
pub struct TypeScriptExtractor {
    base: BaseExtractor,
    /// Pending relationships that need cross-file resolution after workspace indexing
    pending_relationships: Vec<PendingRelationship>,
    structured_pending_relationships: Vec<StructuredPendingRelationship>,
}

impl TypeScriptExtractor {
    /// Create a new TypeScript extractor
    ///
    /// # Phase 2: Relative Unix-Style Path Storage
    /// Now accepts workspace_root to enable relative path storage
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

    /// Extract all symbols from the syntax tree
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        symbols::extract_symbols(self, tree)
    }

    /// Extract all relationships (calls, inheritance, etc.)
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
    fn walk_for_pending_calls(
        &mut self,
        node: tree_sitter::Node,
        symbol_map: &std::collections::HashMap<String, &Symbol>,
    ) {
        // Look for call expressions
        if node.kind() == "call_expression" {
            if let Some(function_node) = node.child_by_field_name("function") {
                let target = self.build_unresolved_target(node, function_node, symbol_map);
                let function_name = target.terminal_name.clone();

                // Check if this is a call to an import or unknown function
                match symbol_map.get(function_name.as_str()) {
                    Some(called_symbol) if called_symbol.kind == SymbolKind::Import => {
                        // This is a call to an imported function - create pending relationship
                        // Find the containing function
                        if let Some(caller_symbol) =
                            self.find_containing_function_in_symbols(node, symbol_map)
                        {
                            let pending = self.base.create_pending_relationship(
                                caller_symbol.id.clone(),
                                target.clone(),
                                RelationshipKind::Calls,
                                &node,
                                Some(caller_symbol.id.clone()),
                                Some(0.8),
                            );
                            self.add_structured_pending_relationship(pending);
                        }
                    }
                    None => {
                        // Unknown function - could be from another file
                        // Check if it's being called from within a function
                        if let Some(caller_symbol) =
                            self.find_containing_function_in_symbols(node, symbol_map)
                        {
                            let pending = self.base.create_pending_relationship(
                                caller_symbol.id.clone(),
                                target,
                                RelationshipKind::Calls,
                                &node,
                                Some(caller_symbol.id.clone()),
                                Some(0.7),
                            );
                            self.add_structured_pending_relationship(pending);
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
                        if matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method) {
                            return Some(symbol);
                        }
                    }
                }
            }

            // Check for test call expressions (it, test, describe, beforeEach, etc.)
            // The arrow_function inside it("name", () => {...}) has no name field,
            // so we look at the parent call_expression and use the test name.
            if current_node.kind() == "call_expression" {
                if let Some(function_node) = current_node.child_by_field_name("function") {
                    let callee = match function_node.kind() {
                        "identifier" => self.base.get_node_text(&function_node),
                        "member_expression" => {
                            if let Some(obj) = function_node.child_by_field_name("object") {
                                self.base.get_node_text(&obj)
                            } else {
                                String::new()
                            }
                        }
                        _ => String::new(),
                    };

                    if crate::test_calls::is_test_runner_call(&callee) {
                        // Get test name from first string argument
                        if let Some(args) = current_node.child_by_field_name("arguments") {
                            let mut cursor = args.walk();
                            if let Some(first_str) = args
                                .children(&mut cursor)
                                .find(|c| c.kind() == "string" || c.kind() == "template_string")
                            {
                                let name = self
                                    .base
                                    .get_node_text(&first_str)
                                    .trim_matches(|c| c == '"' || c == '\'' || c == '`')
                                    .to_string();
                                if let Some(symbol) = symbol_map.get(&name) {
                                    return Some(symbol);
                                }
                            }
                            // For lifecycle (no string arg), look up by callee name
                            let base_name = callee.split('.').next().unwrap_or(&callee).to_string();
                            if let Some(symbol) = symbol_map.get(&base_name) {
                                return Some(symbol);
                            }
                        }
                    }
                }
            }

            current = current_node.parent();
        }

        None
    }

    fn build_unresolved_target(
        &self,
        call_node: tree_sitter::Node,
        function_node: tree_sitter::Node,
        symbol_map: &std::collections::HashMap<String, &Symbol>,
    ) -> UnresolvedTarget {
        if function_node.kind() == "member_expression" {
            let receiver = function_node
                .child_by_field_name("object")
                .map(|node| self.base.get_node_text(&node));
            let property = function_node
                .child_by_field_name("property")
                .map(|node| self.base.get_node_text(&node))
                .unwrap_or_else(|| self.base.get_node_text(&function_node));
            let display_name = receiver
                .as_ref()
                .map(|receiver| format!("{receiver}.{property}"))
                .unwrap_or_else(|| property.clone());
            let import_context = receiver.as_deref().and_then(|receiver| {
                self.find_receiver_import_context(call_node, receiver, symbol_map)
            });

            return UnresolvedTarget {
                display_name,
                terminal_name: property,
                receiver,
                namespace_path: Vec::new(),
                import_context,
            };
        }

        let function_name = self.base.get_node_text(&function_node);
        let import_context = symbol_map
            .get(&function_name)
            .and_then(|symbol| (symbol.kind == SymbolKind::Import).then(|| symbol.name.clone()))
            .or_else(|| {
                self.file_imports_binding(call_node, &function_name)
                    .then_some(function_name.clone())
            });
        UnresolvedTarget {
            display_name: function_name.clone(),
            terminal_name: function_name,
            receiver: None,
            namespace_path: Vec::new(),
            import_context,
        }
    }

    fn find_receiver_import_context(
        &self,
        call_node: tree_sitter::Node,
        receiver_name: &str,
        symbol_map: &std::collections::HashMap<String, &Symbol>,
    ) -> Option<String> {
        let caller_scope = self.find_containing_scope_node(call_node)?;
        let mut stack = vec![caller_scope];
        while let Some(candidate) = stack.pop() {
            let mut cursor = candidate.walk();
            for child in candidate.children(&mut cursor) {
                stack.push(child);
            }

            if candidate.kind() != "variable_declarator" {
                continue;
            }

            let Some(name_node) = candidate.child_by_field_name("name") else {
                continue;
            };
            if self.base.get_node_text(&name_node) != receiver_name {
                continue;
            }

            let Some(value_node) = candidate.child_by_field_name("value") else {
                continue;
            };
            if value_node.kind() != "new_expression" {
                continue;
            }

            let constructor_node = value_node
                .child_by_field_name("constructor")
                .or_else(|| value_node.child_by_field_name("callee"))
                .or_else(|| {
                    let mut cursor = value_node.walk();
                    value_node
                        .named_children(&mut cursor)
                        .find(|child| !matches!(child.kind(), "arguments" | "type_arguments"))
                });
            let Some(constructor_node) = constructor_node else {
                continue;
            };
            let constructor_name = self.base.get_node_text(&constructor_node);
            if symbol_map
                .get(&constructor_name)
                .is_some_and(|symbol| symbol.kind == SymbolKind::Import)
                || self.file_imports_binding(call_node, &constructor_name)
            {
                return Some(constructor_name);
            }
        }

        None
    }

    fn find_containing_scope_node<'a>(
        &self,
        node: tree_sitter::Node<'a>,
    ) -> Option<tree_sitter::Node<'a>> {
        let mut current = node.parent();
        while let Some(current_node) = current {
            if matches!(
                current_node.kind(),
                "function_declaration" | "method_definition" | "arrow_function"
            ) {
                return Some(current_node);
            }
            current = current_node.parent();
        }
        None
    }

    fn file_imports_binding(&self, node: tree_sitter::Node, binding_name: &str) -> bool {
        let mut current = Some(node);
        let mut root = node;
        while let Some(candidate) = current {
            root = candidate;
            current = candidate.parent();
        }

        let mut stack = vec![root];
        while let Some(candidate) = stack.pop() {
            let mut cursor = candidate.walk();
            for child in candidate.children(&mut cursor) {
                stack.push(child);
            }

            if !matches!(candidate.kind(), "import_statement" | "import_declaration") {
                continue;
            }

            if self.base.get_node_text(&candidate).contains(binding_name) {
                return true;
            }
        }

        false
    }

    /// Extract all identifiers (function calls, member access, etc.)
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(self, tree, symbols)
    }

    /// Infer types from variable assignments and function returns
    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        inference::infer_types(self, symbols)
    }

    /// Get pending relationships that need cross-file resolution
    pub fn get_pending_relationships(&self) -> Vec<PendingRelationship> {
        self.pending_relationships.clone()
    }

    pub fn get_structured_pending_relationships(&self) -> Vec<StructuredPendingRelationship> {
        self.structured_pending_relationships.clone()
    }

    /// Add a pending relationship (used during extraction)
    pub fn add_pending_relationship(&mut self, pending: PendingRelationship) {
        self.pending_relationships.push(pending);
    }

    pub fn add_structured_pending_relationship(&mut self, pending: StructuredPendingRelationship) {
        self.pending_relationships.push(pending.pending.clone());
        self.structured_pending_relationships.push(pending);
    }

    // ========================================================================
    // Public access to base for sub-modules (pub(super) scoped internal access)
    // ========================================================================

    /// Get mutable reference to base extractor (for sub-modules)
    pub(crate) fn base_mut(&mut self) -> &mut BaseExtractor {
        &mut self.base
    }

    /// Get immutable reference to base extractor (for sub-modules)
    pub(crate) fn base(&self) -> &BaseExtractor {
        &self.base
    }
}
