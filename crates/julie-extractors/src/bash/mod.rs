//! Bash Extractor - Complete Implementation of bash-extractor.ts
//!
//! Handles Bash/shell-specific constructs for DevOps tracing:
//! - Functions and their definitions
//! - Variables (local, environment, exported)
//! - External command calls (critical for cross-language tracing!)
//! - Script arguments and parameters
//! - Conditional logic and loops
//! - Source/include relationships
//! - Docker, kubectl, npm, and other DevOps tool calls
//!
//! Special focus on cross-language tracing since Bash scripts often orchestrate
//! other programs (Python, Node.js, Go binaries, Docker containers, etc.).

mod commands;
mod functions;
mod helpers;
mod relationships;
mod signatures;
mod types;
mod variables;

use crate::base::{
    BaseExtractor, Identifier, PendingRelationship, Relationship, StructuredPendingRelationship,
    Symbol, SymbolKind,
};
use tree_sitter::Tree;

pub struct BashExtractor {
    pub(super) base: BaseExtractor,
}

impl BashExtractor {
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

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();

        // Detect shebang line
        if let Some(first_line) = self.base.content.lines().next() {
            if first_line.starts_with("#!") {
                let interpreter = first_line.trim_start_matches("#!").trim();
                // Handle "#!/usr/bin/env python3" -> "python3"
                // Handle "#!/bin/bash" -> "bash"
                let name = if interpreter.contains("env ") {
                    interpreter
                        .rsplit_once(' ')
                        .map(|(_, cmd)| cmd)
                        .unwrap_or(interpreter)
                } else {
                    interpreter.rsplit('/').next().unwrap_or(interpreter)
                };
                let root = tree.root_node();
                let symbol = self.base.create_symbol(
                    &root,
                    name.to_string(),
                    SymbolKind::Variable,
                    crate::base::SymbolOptions {
                        signature: Some(first_line.to_string()),
                        ..Default::default()
                    },
                );
                symbols.push(symbol);
            }
        }

        self.walk_tree_for_symbols(tree.root_node(), &mut symbols, None);
        symbols
    }

    /// Main tree traversal for symbol extraction
    fn walk_tree_for_symbols(
        &mut self,
        node: tree_sitter::Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<String>,
    ) {
        if node.kind() == "declaration_command" {
            let declaration_symbols = self.extract_declarations(node, parent_id.as_deref());
            let mut current_parent_id = parent_id;

            if let Some(first_symbol) = declaration_symbols.first() {
                current_parent_id = Some(first_symbol.id.clone());
            }

            symbols.extend(declaration_symbols);

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.walk_tree_for_symbols(child, symbols, current_parent_id.clone());
            }
            return;
        }

        let symbol = self.extract_symbol_from_node(node, parent_id.as_deref());
        let mut current_parent_id = parent_id;

        if let Some(ref sym) = symbol {
            symbols.push(sym.clone());

            // If this is a function, extract its positional parameters
            if sym.kind == SymbolKind::Function {
                let parameters = self.extract_positional_parameters(node, &sym.id);
                symbols.extend(parameters);
            }

            current_parent_id = Some(sym.id.clone());
        }

        // Recursively process child nodes
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_symbols(child, symbols, current_parent_id.clone());
        }
    }

    /// Extract symbol from a node based on its type
    fn extract_symbol_from_node(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        match node.kind() {
            "function_definition" => self.extract_function(node, parent_id),
            "variable_assignment" => self.extract_variable(node, parent_id),
            "declaration_command" => self.extract_declaration(node, parent_id),
            "command" | "simple_command" => self.extract_command(node, parent_id),
            "for_statement" | "while_statement" | "if_statement" => None,
            _ => None,
        }
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.walk_tree_for_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    /// Walk tree extracting relationships
    fn walk_tree_for_relationships(
        &mut self,
        node: tree_sitter::Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        match node.kind() {
            "command" | "simple_command" => {
                self.extract_command_relationships(node, symbols, relationships);
            }
            _ => {}
        }

        // Recursively process child nodes
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_relationships(child, symbols, relationships);
        }
    }

    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        // Call the identifiers module implementation
        let symbol_map: std::collections::HashMap<String, &Symbol> =
            symbols.iter().map(|s| (s.id.clone(), s)).collect();
        self.walk_tree_for_identifiers(tree.root_node(), &symbol_map);
        self.base.identifiers.clone()
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> std::collections::HashMap<String, String> {
        // Delegate to types module
        types::BashExtractor::infer_types(self, symbols)
    }

    // Identifier extraction helper methods
    fn walk_tree_for_identifiers(
        &mut self,
        node: tree_sitter::Node,
        symbol_map: &std::collections::HashMap<String, &Symbol>,
    ) {
        self.extract_identifier_from_node(node, symbol_map);
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_identifiers(child, symbol_map);
        }
    }

    fn extract_identifier_from_node(
        &mut self,
        node: tree_sitter::Node,
        symbol_map: &std::collections::HashMap<String, &Symbol>,
    ) {
        match node.kind() {
            "command" => {
                if let Some(command_name_node) = self.find_command_name_node(node) {
                    let name = self.base.get_node_text(&command_name_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);
                    self.base.create_identifier(
                        &command_name_node,
                        name.clone(),
                        crate::base::IdentifierKind::Call,
                        containing_symbol_id,
                    );
                    // Miller bridge Phase 3b: capture string-literal command args.
                    self.record_command_arg_literals(node, &name, symbol_map);
                }
            }
            "subscript" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "variable_name" || child.kind() == "simple_expansion" {
                        let name = self.base.get_node_text(&child);
                        let clean_name = name.trim_start_matches('$').to_string();
                        let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);
                        self.base.create_identifier(
                            &child,
                            clean_name,
                            crate::base::IdentifierKind::MemberAccess,
                            containing_symbol_id,
                        );
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    /// Capture string-literal arguments of a `command` node (Miller bridge Phase 3b).
    ///
    /// Bash commands are a COMMAND grammar, not `call_expression`: the carrier is
    /// the command name itself (`curl`, `wget`, `psql`, `mysql`, `sqlite3`, …) and
    /// the args are the repeated `argument`-field children. This is config-free —
    /// `kind` is `Other` and the `src/` carrier gate reclassifies/drops; the
    /// `[literal_carriers]` table in `languages/bash.toml` decides which command
    /// names survive.
    ///
    /// Only string-bearing args are captured (`string`, `raw_string`,
    /// `ansi_c_string`, `translated_string` — all decode via
    /// `decode_string_literal`). A bare `word` arg such as the unquoted URL in
    /// `curl https://x` is NOT a string literal and is intentionally skipped;
    /// quoting (`curl "https://x"`) is required for capture, matching the
    /// string-literal contract used by every other language.
    ///
    /// `arg_position` counts over the full `argument` list, so the SQL in
    /// `psql -c "SELECT …"` (args `["-c", "SELECT …"]`) reports position 1.
    fn record_command_arg_literals(
        &mut self,
        command_node: tree_sitter::Node,
        carrier: &str,
        symbol_map: &std::collections::HashMap<String, &Symbol>,
    ) {
        let containing_symbol_id = self.find_containing_symbol_id(command_node, symbol_map);
        let args: Vec<tree_sitter::Node> = {
            let mut cursor = command_node.walk();
            command_node
                .children_by_field_name("argument", &mut cursor)
                .collect()
        };
        for (position, arg) in args.into_iter().enumerate() {
            if let Some(text) = self.base.decode_string_literal(&arg) {
                self.base.record_literal(
                    &arg,
                    text,
                    Some(carrier.to_string()),
                    position as u32,
                    containing_symbol_id.clone(),
                );
            }
        }
    }

    fn find_containing_symbol_id(
        &self,
        node: tree_sitter::Node,
        symbol_map: &std::collections::HashMap<String, &Symbol>,
    ) -> Option<String> {
        let file_symbols: Vec<Symbol> = symbol_map
            .values()
            .filter(|s| s.file_path == self.base.file_path)
            .map(|&s| s.clone())
            .collect();
        self.base
            .find_containing_symbol(&node, &file_symbols)
            .map(|s| s.id.clone())
    }

    // ========================================================================
    // Pending Relationship Management
    // ========================================================================

    pub(crate) fn add_structured_pending_relationship(
        &mut self,
        pending: StructuredPendingRelationship,
    ) {
        self.base.add_structured_pending_relationship(pending);
    }

    /// Get all pending relationships collected during extraction
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
}
