//! GDScript Extractor for Julie
//!
//! Extracts symbols from GDScript files (Godot's scripting language).
//! Supports:
//! - Classes (both explicit class_name and inner class definitions)
//! - Functions and methods
//! - Variables with @export and @onready annotations
//! - Constants
//! - Enums and enum members
//! - Signals
//! - Constructors (_init)

mod classes;
mod enums;
mod functions;
mod helpers;
mod identifiers;
mod signals;
mod types;
mod variables;

use crate::extractors::base::{BaseExtractor, Identifier, Relationship, Symbol};
use std::collections::{HashMap, HashSet};
use tree_sitter::{Node, Tree};

pub struct GDScriptExtractor {
    base: BaseExtractor,
    pending_inheritance: HashMap<String, String>, // className -> baseClassName
    processed_positions: HashSet<String>,         // Track processed node positions
    current_class_context: Option<String>,        // Current class ID for scope tracking
}

impl GDScriptExtractor {
    pub fn new(
        language: String,
        file_path: String,
        content: String,
        workspace_root: &std::path::Path,
    ) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content, workspace_root),
            pending_inheritance: HashMap::new(),
            processed_positions: HashSet::new(),
            current_class_context: None,
        }
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.pending_inheritance.clear();
        self.processed_positions.clear();
        self.current_class_context = None;

        let root_node = tree.root_node();
        // First pass: collect inheritance information
        classes::collect_inheritance_info(&mut self.base, root_node, &mut self.pending_inheritance);

        // Check for top-level extends statement (creates implicit class)
        let mut implicit_class_id: Option<String> = None;
        for i in 0..root_node.child_count() {
            if let Some(child) = root_node.child(i) {
                if child.kind() == "extends_statement" {
                    if let Some(type_node) = helpers::find_child_by_type(child, "type") {
                        let base_class_name = self.base.get_node_text(&type_node);

                        // Create implicit class based on file name
                        let file_name = self
                            .base
                            .file_path
                            .split('/')
                            .next_back()
                            .unwrap_or("ImplicitClass")
                            .replace(".gd", "");

                        let mut metadata = HashMap::new();
                        metadata.insert(
                            "baseClass".to_string(),
                            serde_json::Value::String(base_class_name.clone()),
                        );

                        let implicit_class = self.base.create_symbol(
                            &child,
                            file_name,
                            crate::extractors::base::SymbolKind::Class,
                            crate::extractors::base::SymbolOptions {
                                signature: Some(format!("extends {}", base_class_name)),
                                visibility: Some(crate::extractors::base::Visibility::Public),
                                parent_id: None,
                                metadata: Some(metadata),
                                doc_comment: None,
                            },
                        );

                        implicit_class_id = Some(implicit_class.id.clone());
                        symbols.push(implicit_class);
                        break;
                    }
                }
            }
        }

        // Second pass: extract symbols with implicit class context
        self.traverse_node(root_node, implicit_class_id.as_ref(), &mut symbols);

        symbols
    }

    pub fn extract_relationships(
        &mut self,
        _tree: &Tree,
        _symbols: &[Symbol],
    ) -> Vec<Relationship> {
        // For now, return empty relationships - this can be extended later
        Vec::new()
    }

    /// Extract all identifier usages (function calls, member access, etc.)
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(&mut self.base, tree, symbols)
    }

    /// Main tree traversal for symbol extraction
    fn traverse_node(&mut self, node: Node, parent_id: Option<&String>, symbols: &mut Vec<Symbol>) {
        // Create position-based key to prevent double processing
        let position_key = helpers::get_position_key(node);

        if self.processed_positions.contains(&position_key) {
            return;
        }
        self.processed_positions.insert(position_key);

        let mut extracted_symbol: Option<Symbol> = None;

        match node.kind() {
            "class_name_statement" => {
                if let Some(symbol) = classes::extract_class_name_statement(
                    &mut self.base,
                    &self.pending_inheritance,
                    node,
                    parent_id,
                ) {
                    // Set current class context for class_name classes
                    self.current_class_context = Some(symbol.id.clone());
                    extracted_symbol = Some(symbol);
                }
            }
            "class" => {
                if let Some(symbol) =
                    classes::extract_class_definition(&mut self.base, node, parent_id)
                {
                    // Set current class context for inner classes
                    self.current_class_context = Some(symbol.id.clone());
                    extracted_symbol = Some(symbol);
                }
            }
            "function_definition" => {
                // Check if we should use the current class context as parent
                let effective_parent_id =
                    self.determine_effective_parent_id(node, parent_id, symbols);
                if let Some(symbol) = functions::extract_function_definition(
                    &mut self.base,
                    node,
                    effective_parent_id.as_ref(),
                    symbols,
                ) {
                    extracted_symbol = Some(symbol);
                }
            }
            "func" => {
                // Skip if this func node is part of a function_definition
                if let Some(parent) = node.parent() {
                    if parent.kind() != "function_definition" {
                        let effective_parent_id =
                            self.determine_effective_parent_id(node, parent_id, symbols);
                        if let Some(symbol) = functions::extract_function_definition(
                            &mut self.base,
                            node,
                            effective_parent_id.as_ref(),
                            symbols,
                        ) {
                            extracted_symbol = Some(symbol);
                        }
                    }
                }
            }
            "constructor_definition" => {
                let effective_parent_id =
                    self.determine_effective_parent_id(node, parent_id, symbols);
                if let Some(symbol) = functions::extract_constructor_definition(
                    &mut self.base,
                    node,
                    effective_parent_id.as_ref(),
                ) {
                    extracted_symbol = Some(symbol);
                }
            }
            "var" => {
                // Skip if this var node is part of a variable_statement
                if let Some(parent) = node.parent() {
                    if parent.kind() != "variable_statement" {
                        if let Some(symbol) =
                            variables::extract_variable_statement(&mut self.base, node, parent_id)
                        {
                            extracted_symbol = Some(symbol);
                        }
                    }
                }
            }
            "variable_statement" => {
                if let Some(symbol) = variables::extract_variable_from_statement(
                    &mut self.base,
                    node,
                    parent_id,
                    symbols,
                ) {
                    extracted_symbol = Some(symbol);
                }
            }
            "const" => {
                if let Some(symbol) =
                    variables::extract_constant_statement(&mut self.base, node, parent_id)
                {
                    extracted_symbol = Some(symbol);
                }
            }
            "enum_definition" => {
                if let Some(symbol) =
                    enums::extract_enum_definition(&mut self.base, node, parent_id)
                {
                    extracted_symbol = Some(symbol);
                }
            }
            "identifier" => {
                // Check if this identifier is an enum member
                if let Some(symbol) =
                    enums::extract_enum_member(&mut self.base, node, parent_id, symbols)
                {
                    extracted_symbol = Some(symbol);
                }
            }
            "signal_statement" | "signal" => {
                if let Some(symbol) =
                    signals::extract_signal_statement(&mut self.base, node, parent_id)
                {
                    extracted_symbol = Some(symbol);
                }
            }
            _ => {}
        }

        if let Some(symbol) = extracted_symbol {
            let symbol_id = symbol.id.clone();
            symbols.push(symbol);

            // Traverse children with current symbol as parent
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    self.traverse_node(child, Some(&symbol_id), symbols);
                }
            }
        } else {
            // Traverse children with current parent
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    self.traverse_node(child, parent_id, symbols);
                }
            }
        }
    }

    /// Determine if a function should belong to the current class context
    fn determine_effective_parent_id(
        &self,
        node: Node,
        parent_id: Option<&String>,
        symbols: &[Symbol],
    ) -> Option<String> {
        // If we have a current class context, check if this function should belong to it
        if let Some(class_id) = &self.current_class_context {
            // Find the class symbol to get its context
            if let Some(class_symbol) = symbols.iter().find(|s| &s.id == class_id) {
                let class_start_col = class_symbol.start_column;
                let func_start_col = node.start_position().column as u32;

                // For class_name classes, functions at the same level or slightly indented belong to the class
                let is_class_name_class = class_symbol
                    .signature
                    .as_ref()
                    .map(|s| s.contains("class_name"))
                    .unwrap_or(false);

                // For inner classes, functions must be indented more than the class
                let is_inner_class = class_symbol
                    .signature
                    .as_ref()
                    .map(|s| s.contains("class ") && !s.contains("class_name"))
                    .unwrap_or(false);

                if is_class_name_class {
                    // For class_name classes, functions at same level or indented belong to the class
                    if func_start_col >= class_start_col {
                        return Some(class_id.clone());
                    }
                } else if is_inner_class {
                    // For inner classes, functions must be indented more than the class
                    if func_start_col > class_start_col {
                        return Some(class_id.clone());
                    }
                }
            }
        }

        // Otherwise, use the provided parent_id
        parent_id.cloned()
    }
}
