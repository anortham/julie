//! SQL language extractor module.
//!
//! This module provides comprehensive SQL symbol extraction for cross-platform code intelligence.
//! It's organized into logical submodules for maintainability:
//!
//! - **helpers.rs**: Regex patterns and utility functions
//! - **schemas.rs**: Table, view, index, trigger extraction
//! - **routines.rs**: Stored procedures and functions
//! - **constraints.rs**: Column and table constraints
//! - **relationships.rs**: Foreign keys and joins
//! - **error_handling.rs**: ERROR node processing
//! - **views.rs**: View columns and SELECT alias extraction
//! - **identifiers.rs**: Identifier usage extraction
//!
//! This enables full-stack symbol tracing from frontend -> API -> database schema.

mod constraints;
mod error_handling;
mod helpers;
mod identifiers;
mod relationships;
mod routines;
mod schemas;
mod views;

use crate::base::{BaseExtractor, Identifier, Relationship, Symbol};
use std::collections::HashMap;
use tree_sitter::Tree;

/// SQL language extractor that handles SQL-specific constructs for cross-language tracing:
/// - Table definitions (CREATE TABLE)
/// - Column definitions and constraints
/// - Stored procedures and functions
/// - Views and triggers
/// - Indexes and foreign keys
/// - Query patterns and table references
pub struct SqlExtractor {
    base: BaseExtractor,
}

impl SqlExtractor {
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
        self.visit_node(tree.root_node(), &mut symbols, None);
        symbols
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        relationships::extract_relationships_internal(
            &mut self.base,
            tree.root_node(),
            symbols,
            &mut relationships,
        );
        relationships
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        use crate::sql::helpers::SQL_TYPE_RE;

        let mut types = HashMap::new();

        // SQL type inference based on symbol metadata and signatures
        for symbol in symbols {
            if let Some(ref signature) = symbol.signature {
                // Extract SQL data types from signatures like "CREATE TABLE users (id INT, name VARCHAR(100))"
                if let Some(type_match) = SQL_TYPE_RE.find(signature) {
                    types.insert(symbol.id.clone(), type_match.as_str().to_uppercase());
                }
            }

            // Use metadata for SQL-specific types
            if symbol
                .metadata
                .as_ref()
                .and_then(|m| m.get("isTable"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                types.insert(symbol.id.clone(), "TABLE".to_string());
            }
            if symbol
                .metadata
                .as_ref()
                .and_then(|m| m.get("isView"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                types.insert(symbol.id.clone(), "VIEW".to_string());
            }
            if symbol
                .metadata
                .as_ref()
                .and_then(|m| m.get("isStoredProcedure"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                types.insert(symbol.id.clone(), "PROCEDURE".to_string());
            }
        }

        types
    }

    /// Extract all identifier usages (function calls, member access, etc.)
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        let symbol_map: HashMap<String, &Symbol> =
            symbols.iter().map(|s| (s.id.clone(), s)).collect();

        self.walk_tree_for_identifiers(tree.root_node(), &symbol_map);
        self.base.identifiers.clone()
    }

    /// Main node visiting dispatch function
    fn visit_node(
        &mut self,
        node: tree_sitter::Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<&str>,
    ) {
        let mut symbol: Option<Symbol> = None;

        match node.kind() {
            "create_table" => {
                symbol = schemas::extract_table_definition(&mut self.base, node, parent_id);
            }
            "create_procedure" | "create_function" | "create_function_statement" => {
                symbol = routines::extract_stored_procedure(&mut self.base, node, parent_id);
            }
            "create_view" => {
                symbol = schemas::extract_view(&mut self.base, node, parent_id);
            }
            "create_index" => {
                symbol = schemas::extract_index(&mut self.base, node, parent_id);
            }
            "create_trigger" => {
                symbol = schemas::extract_trigger(&mut self.base, node, parent_id);
            }
            "cte" => {
                symbol = schemas::extract_cte(&mut self.base, node, parent_id);
            }
            "create_schema" => {
                symbol = schemas::extract_schema(&mut self.base, node, parent_id);
            }
            "create_sequence" => {
                symbol = schemas::extract_sequence(&mut self.base, node, parent_id);
            }
            "create_domain" => {
                symbol = schemas::extract_domain(&mut self.base, node, parent_id);
            }
            "create_type" => {
                symbol = schemas::extract_type(&mut self.base, node, parent_id);
            }
            "alter_table" => {
                constraints::extract_constraints_from_alter_table(
                    &mut self.base,
                    node,
                    symbols,
                    parent_id,
                );
            }
            "select" => {
                self.extract_select_aliases(node, symbols, parent_id);
            }
            "ERROR" => {
                // Remember symbol count before extraction
                let symbols_before = symbols.len();

                error_handling::extract_multiple_from_error_node(
                    &mut self.base,
                    node,
                    symbols,
                    parent_id,
                );

                // Check if any view symbols were added and extract their columns
                for i in symbols_before..symbols.len() {
                    let symbol_ref = &symbols[i].clone(); // Clone to avoid borrow issues
                    if symbol_ref
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("isView"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        self.extract_view_columns_from_error_node(node, symbols, &symbol_ref.id);
                    }
                }
            }
            _ => {}
        }

        if let Some(symbol) = symbol {
            symbols.push(symbol.clone());

            // Extract additional child symbols for specific node types
            match node.kind() {
                "create_table" => {
                    constraints::extract_table_columns(&mut self.base, node, symbols, &symbol.id);
                    constraints::extract_table_constraints(
                        &mut self.base,
                        node,
                        symbols,
                        &symbol.id,
                    );
                }
                "create_view" => {
                    self.extract_view_columns(node, symbols, &symbol.id);
                }
                "ERROR" => {
                    let metadata = &symbol.metadata;
                    if metadata
                        .as_ref()
                        .and_then(|m| m.get("isView"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        self.extract_view_columns_from_error_node(node, symbols, &symbol.id);
                    }
                    if metadata
                        .as_ref()
                        .and_then(|m| m.get("isStoredProcedure"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                        || metadata
                            .as_ref()
                            .and_then(|m| m.get("isFunction"))
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                    {
                        routines::extract_parameters_from_error_node(
                            &mut self.base,
                            node,
                            symbols,
                            &symbol.id,
                        );
                    }
                }
                "create_function" | "create_function_statement" => {
                    routines::extract_declare_variables(&mut self.base, node, symbols, &symbol.id);
                }
                _ => {}
            }

            // Continue with this symbol as parent
            let new_parent_id = Some(symbol.id.as_str());
            for child in node.children(&mut node.walk()) {
                self.visit_node(child, symbols, new_parent_id);
            }
        } else {
            // No symbol extracted, continue with current parent
            for child in node.children(&mut node.walk()) {
                self.visit_node(child, symbols, parent_id);
            }
        }
    }
}
