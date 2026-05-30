//! Identifier extraction for JavaScript
//!
//! Handles extraction of all identifier usages including function calls,
//! member access, and other references used for LSP-quality find_references.

use crate::base::{Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

impl super::JavaScriptExtractor {
    /// Extract all identifier usages (function calls, member access, etc.)
    /// Following the Rust extractor reference implementation pattern
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        // Create symbol map for fast lookup
        let symbol_map: HashMap<String, &Symbol> =
            symbols.iter().map(|s| (s.id.clone(), s)).collect();

        // Walk the tree and extract identifiers
        self.walk_tree_for_identifiers(tree.root_node(), &symbol_map);

        // Return the collected identifiers
        self.base.identifiers.clone()
    }

    /// Recursively walk tree extracting identifiers from each node
    fn walk_tree_for_identifiers(&mut self, node: Node, symbol_map: &HashMap<String, &Symbol>) {
        // Extract identifier from this node if applicable
        self.extract_identifier_from_node(node, symbol_map);

        // Recursively walk children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_identifiers(child, symbol_map);
        }
    }

    /// Extract identifier from a single node based on its kind
    fn extract_identifier_from_node(&mut self, node: Node, symbol_map: &HashMap<String, &Symbol>) {
        match node.kind() {
            // Function/method calls: foo(), bar.baz()
            "call_expression" => {
                // The function being called is in the "function" field
                if let Some(function_node) = node.child_by_field_name("function") {
                    match function_node.kind() {
                        "identifier" => {
                            // Simple function call: foo()
                            let name = self.base.get_node_text(&function_node);
                            let containing_symbol_id =
                                self.find_containing_symbol_id(node, symbol_map);

                            self.base.create_identifier(
                                &function_node,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                        }
                        "member_expression" => {
                            // Member call: object.method()
                            // Extract the rightmost identifier (the method name)
                            if let Some(property_node) =
                                function_node.child_by_field_name("property")
                            {
                                let name = self.base.get_node_text(&property_node);
                                let containing_symbol_id =
                                    self.find_containing_symbol_id(node, symbol_map);

                                self.base.create_identifier(
                                    &property_node,
                                    name,
                                    IdentifierKind::Call,
                                    containing_symbol_id,
                                );
                            }
                        }
                        _ => {
                            // Other cases like computed member expressions
                            // Skip for now
                        }
                    }
                }
                // Phase 3: capture string-literal call-arguments (config-free; the
                // carrier classification + gate happen in the src/ pipeline).
                self.record_call_arg_literals(&node, symbol_map);
            }

            "new_expression" => {
                if let Some((name_node, name)) = self.constructor_identifier(&node) {
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);
                    self.base.create_identifier(
                        &name_node,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    );
                }
            }

            // Member access: object.property
            "member_expression" => {
                // Only extract if it's NOT part of a call_expression
                // (we handle those in the call_expression case above)
                if let Some(parent) = node.parent() {
                    if parent.kind() == "call_expression" {
                        // Check if this member_expression is the function being called
                        if let Some(function_node) = parent.child_by_field_name("function") {
                            if function_node.id() == node.id() {
                                return; // Skip - handled by call_expression
                            }
                        }
                    }
                    if parent.kind() == "new_expression" {
                        if let Some(constructor_node) = parent.child_by_field_name("constructor") {
                            if constructor_node.id() == node.id() {
                                return;
                            }
                        }
                    }
                }

                // Extract the rightmost identifier (the property name)
                if let Some(property_node) = node.child_by_field_name("property") {
                    let name = self.base.get_node_text(&property_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &property_node,
                        name,
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                }
            }

            _ => {
                // Skip other node types for now
                // Future: type usage, constructor calls, etc.
            }
        }
    }

    /// Find the ID of the symbol that contains this node
    /// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
    fn find_containing_symbol_id(
        &self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) -> Option<String> {
        self.base
            .find_containing_symbol_from_map(&node, symbol_map)
            .map(|s| s.id.clone())
    }

    // ========================================================================
    // String-literal call-argument capture (Miller bridge Phase 3)
    // ========================================================================

    /// Capture string-literal arguments of a JS `call_expression` as `Literal`
    /// records. Config-free: `carrier` is the verbatim callee text; the URL/SQL
    /// classification and the carrier gate run later in the `src/` pipeline.
    /// Mirrors the TypeScript leg (JS shares the same `call_expression` grammar
    /// shape: `function` callee + `arguments` list, with tagged templates
    /// arriving as a `template_string` in the `arguments` field). `arg_position`
    /// is counted over the full (named) argument list.
    fn record_call_arg_literals(
        &mut self,
        call_node: &Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        let Some(function_node) = call_node.child_by_field_name("function") else {
            return;
        };
        let Some(args_node) = call_node.child_by_field_name("arguments") else {
            return;
        };
        let carrier = self.callee_text(function_node);
        let containing_symbol_id = self.find_containing_symbol_id(*call_node, symbol_map);

        let mut cursor = args_node.walk();
        for (pos, arg) in args_node.named_children(&mut cursor).enumerate() {
            if let Some(text) = self.base.decode_string_literal(&arg) {
                self.base.record_literal(
                    &arg,
                    text,
                    carrier.clone(),
                    pos as u32,
                    containing_symbol_id.clone(),
                );
            }
        }
    }

    /// Derive the verbatim callee text used as a literal's `carrier`.
    ///
    /// Plain `identifier` → its text (`fetch`). `member_expression` → the
    /// `object.property` join (`axios.get`) so dotted client APIs match config.
    fn callee_text(&self, function_node: Node) -> Option<String> {
        match function_node.kind() {
            "identifier" => Some(self.base.get_node_text(&function_node)),
            "member_expression" => {
                let object = function_node
                    .child_by_field_name("object")
                    .map(|n| self.base.get_node_text(&n));
                let property = function_node
                    .child_by_field_name("property")
                    .map(|n| self.base.get_node_text(&n));
                match (object, property) {
                    (Some(o), Some(p)) => Some(format!("{o}.{p}")),
                    (None, Some(p)) => Some(p),
                    _ => None,
                }
            }
            _ => {
                let text = self.base.get_node_text(&function_node);
                if text.is_empty() { None } else { Some(text) }
            }
        }
    }

    fn constructor_identifier<'tree>(&self, node: &Node<'tree>) -> Option<(Node<'tree>, String)> {
        let constructor = node
            .child_by_field_name("constructor")
            .or_else(|| node.child_by_field_name("callee"))
            .or_else(|| {
                let mut cursor = node.walk();
                node.named_children(&mut cursor)
                    .find(|child| child.kind() != "arguments")
            })?;
        self.terminal_identifier(constructor)
    }

    fn terminal_identifier<'tree>(&self, node: Node<'tree>) -> Option<(Node<'tree>, String)> {
        match node.kind() {
            "identifier" | "property_identifier" | "private_property_identifier" => {
                Some((node, self.base.get_node_text(&node)))
            }
            "member_expression" => node
                .child_by_field_name("property")
                .and_then(|property| self.terminal_identifier(property)),
            _ => None,
        }
    }
}
