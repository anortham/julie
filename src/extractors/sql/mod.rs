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
//!
//! This enables full-stack symbol tracing from frontend → API → database schema.

mod constraints;
mod error_handling;
mod helpers;
mod relationships;
mod routines;
mod schemas;

use crate::extractors::base::{
    BaseExtractor, Identifier, IdentifierKind, Relationship, Symbol, SymbolKind, SymbolOptions,
};
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
        use crate::extractors::sql::helpers::SQL_TYPE_RE;

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

    /// Extract SELECT query aliases as fields
    fn extract_select_aliases(
        &mut self,
        select_node: tree_sitter::Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<&str>,
    ) {
        // Port extractSelectAliases logic using iterative approach to avoid borrow checker issues
        let term_nodes = self.base.find_nodes_by_type(&select_node, "term");

        for node in term_nodes {
            let mut children = Vec::new();
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    children.push(child);
                }
            }

            if children.len() >= 3 {
                for i in 0..(children.len() - 2) {
                    if children[i + 1].kind() == "keyword_as"
                        && children[i + 2].kind() == "identifier"
                    {
                        let expr_node = children[i];
                        let alias_name = self.base.get_node_text(&children[i + 2]);
                        let expr_text = self.base.get_node_text(&expr_node);

                        // Determine expression type for better signatures - CRITICAL: Window function handling
                        let expression = match expr_node.kind() {
                            "case" => "CASE expression".to_string(),
                            "window_function" => {
                                // Keep the OVER clause in the signature for window functions
                                if expr_text.contains("OVER (") {
                                    if let Some(over_index) = expr_text.find("OVER (") {
                                        if let Some(end_index) = expr_text[over_index..].find(')') {
                                            // Use safe UTF-8 aware substring extraction
                                            let total_len = over_index + end_index + 1;
                                            if expr_text.is_char_boundary(total_len) {
                                                expr_text[0..total_len].to_string()
                                            } else {
                                                expr_text.clone()
                                            }
                                        } else {
                                            expr_text.clone() // Keep full text if no closing paren
                                        }
                                    } else {
                                        expr_text.clone()
                                    }
                                } else {
                                    expr_text.clone() // Keep full text for window_function type
                                }
                            }
                            _ => {
                                if expr_text.contains("OVER (") {
                                    // Handle expressions with OVER clauses that aren't detected as window_function
                                    if let Some(over_index) = expr_text.find("OVER (") {
                                        if let Some(end_index) = expr_text[over_index..].find(')') {
                                            // Use safe UTF-8 aware substring extraction
                                            let total_len = over_index + end_index + 1;
                                            if expr_text.is_char_boundary(total_len) {
                                                expr_text[0..total_len].to_string()
                                            } else {
                                                expr_text.clone()
                                            }
                                        } else {
                                            expr_text.clone()
                                        }
                                    } else {
                                        expr_text.clone()
                                    }
                                } else if expr_text.contains("COUNT")
                                    || expr_text.contains("SUM")
                                    || expr_text.contains("AVG")
                                    || expr_text.contains("MAX")
                                    || expr_text.contains("MIN")
                                {
                                    format!(
                                        "{}()",
                                        expr_text.split('(').next().unwrap_or(&expr_text)
                                    )
                                } else {
                                    expr_text.clone()
                                }
                            }
                        };

                        let signature = format!("{} AS {}", expression, alias_name);

                        let mut metadata = HashMap::new();
                        metadata.insert("isSelectAlias".to_string(), serde_json::Value::Bool(true));
                        metadata
                            .insert("isComputedField".to_string(), serde_json::Value::Bool(true));

                        let options = SymbolOptions {
                            signature: Some(signature),
                            visibility: Some(crate::extractors::base::Visibility::Public),
                            parent_id: parent_id.map(|s| s.to_string()),
                            doc_comment: None,
                            metadata: Some(metadata),
                        };

                        let alias_symbol =
                            self.base
                                .create_symbol(&node, alias_name, SymbolKind::Field, options);
                        symbols.push(alias_symbol);
                        break;
                    }
                }
            }
        }
    }

    /// Extract view columns from CREATE VIEW statement
    fn extract_view_columns(
        &mut self,
        view_node: tree_sitter::Node,
        symbols: &mut Vec<Symbol>,
        parent_view_id: &str,
    ) {
        // Port extractViewColumns logic
        let nodes = self.base.find_nodes_by_type(&view_node, "select_statement");
        for select_node in nodes {
            self.extract_select_aliases(select_node, symbols, Some(parent_view_id));
        }

        let select_nodes = self.base.find_nodes_by_type(&view_node, "select");
        for select_node in select_nodes {
            self.extract_select_aliases(select_node, symbols, Some(parent_view_id));
        }
    }

    /// Extract view columns from ERROR node
    fn extract_view_columns_from_error_node(
        &mut self,
        node: tree_sitter::Node,
        symbols: &mut Vec<Symbol>,
        parent_view_id: &str,
    ) {
        // Port extractViewColumnsFromErrorNode logic
        let error_text = self.base.get_node_text(&node);

        // Only process if this ERROR node contains a CREATE VIEW statement
        if !error_text.contains("CREATE VIEW") {
            return;
        }

        let select_index = match error_text.find("SELECT") {
            Some(idx) => idx,
            None => return,
        };

        // Find the FROM clause to limit our search to the SELECT list only
        let from_regex = regex::Regex::new(r"\bFROM\s+[a-zA-Z_][a-zA-Z0-9_]*\s+[a-zA-Z_]").unwrap();
        let from_index = from_regex
            .find(&error_text[select_index..])
            .map(|from_match| select_index + from_match.start());

        let select_section = if let Some(from_idx) = from_index {
            if from_idx > select_index
                && error_text.is_char_boundary(select_index)
                && error_text.is_char_boundary(from_idx)
            {
                &error_text[select_index..from_idx]
            } else if error_text.is_char_boundary(select_index) {
                &error_text[select_index..]
            } else {
                &error_text
            }
        } else if error_text.is_char_boundary(select_index) {
            &error_text[select_index..]
        } else {
            &error_text
        };

        // Extract SELECT aliases using regex patterns
        let alias_regex = regex::Regex::new(
            r"(?:^|,|\s)\s*(.+?)\s+(?:[Aa][Ss]\s+)?([a-zA-Z_][a-zA-Z0-9_]*)\s*(?:,|$)",
        )
        .unwrap();

        for captures in alias_regex.captures_iter(select_section) {
            // Safe: if regex matched, capture groups should exist, but handle gracefully
            let full_expression = captures.get(1).map_or("", |m| m.as_str()).trim();
            let alias_name = captures.get(2).map_or("", |m| m.as_str());

            // Skip if capture groups were empty
            if full_expression.is_empty() || alias_name.is_empty() {
                continue;
            }

            // Skip if this looks like a table alias or common SQL keywords
            if [
                "u",
                "ae",
                "users",
                "analytics_events",
                "id",
                "username",
                "email",
            ]
            .contains(&alias_name)
            {
                continue;
            }

            // Skip if the expression looks like a simple column reference
            if !full_expression.contains('(')
                && !full_expression.contains("COUNT")
                && !full_expression.contains("MIN")
                && !full_expression.contains("MAX")
                && !full_expression.contains("AVG")
                && !full_expression.contains("SUM")
                && !full_expression.contains("EXTRACT")
                && !full_expression.contains("CASE")
                && full_expression.split('.').count() <= 2
            {
                continue;
            }

            let signature = format!("{} AS {}", full_expression, alias_name);

            let mut metadata = HashMap::new();
            metadata.insert("isSelectAlias".to_string(), serde_json::Value::Bool(true));
            metadata.insert("isComputedField".to_string(), serde_json::Value::Bool(true));
            metadata.insert(
                "extractedFromError".to_string(),
                serde_json::Value::Bool(true),
            );

            let options = SymbolOptions {
                signature: Some(signature),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: Some(parent_view_id.to_string()),
                doc_comment: None,
                metadata: Some(metadata),
            };

            let alias_symbol =
                self.base
                    .create_symbol(&node, alias_name.to_string(), SymbolKind::Field, options);
            symbols.push(alias_symbol);
        }
    }

    /// Recursively walk tree extracting identifiers from each node
    fn walk_tree_for_identifiers(
        &mut self,
        node: tree_sitter::Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        self.extract_identifier_from_node(node, symbol_map);

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_identifiers(child, symbol_map);
        }
    }

    /// Extract identifier from a single node based on its kind
    fn extract_identifier_from_node(
        &mut self,
        node: tree_sitter::Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        match node.kind() {
            "invocation" => {
                let name_node = if let Some(obj_ref) =
                    self.base.find_child_by_type(&node, "object_reference")
                {
                    self.base.find_child_by_type(&obj_ref, "identifier")
                } else {
                    self.base.find_child_by_type(&node, "identifier")
                };

                if let Some(name_node) = name_node {
                    let name = self.base.get_node_text(&name_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &name_node,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    );
                }
            }

            "identifier" => {
                if let Some(next_sibling) = node.next_sibling() {
                    if next_sibling.kind() == "function_arguments" {
                        let name = self.base.get_node_text(&node);
                        let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                        self.base.create_identifier(
                            &node,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                        return;
                    }
                }

                if let Some(parent) = node.parent() {
                    match parent.kind() {
                        "select_expression" | "where_clause" | "having_clause" => {
                            let name = self.base.get_node_text(&node);
                            let containing_symbol_id =
                                self.find_containing_symbol_id(node, symbol_map);

                            self.base.create_identifier(
                                &node,
                                name,
                                IdentifierKind::MemberAccess,
                                containing_symbol_id,
                            );
                        }
                        _ => {}
                    }
                }
            }

            "field" => {
                if let Some(parent) = node.parent() {
                    if parent.kind() == "table_reference" || parent.kind() == "qualified_name" {
                        return;
                    }
                }

                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = self.base.get_node_text(&name_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &name_node,
                        name,
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                } else {
                    let name = self.base.get_node_text(&node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &node,
                        name,
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                }
            }

            "qualified_name" => {
                let mut rightmost_identifier = None;
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        rightmost_identifier = Some(child);
                    }
                }

                if let Some(name_node) = rightmost_identifier {
                    let name = self.base.get_node_text(&name_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &name_node,
                        name,
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                }
            }

            _ => {}
        }
    }

    /// Find the ID of the symbol that contains this node
    fn find_containing_symbol_id(
        &self,
        node: tree_sitter::Node,
        symbol_map: &HashMap<String, &Symbol>,
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
}
