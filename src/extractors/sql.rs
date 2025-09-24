use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, Relationship, RelationshipKind, SymbolOptions};
use tree_sitter::Tree;
use std::collections::HashMap;
use serde_json::Value;

/// SQL language extractor that handles SQL-specific constructs for cross-language tracing:
/// - Table definitions (CREATE TABLE)
/// - Column definitions and constraints
/// - Stored procedures and functions
/// - Views and triggers
/// - Indexes and foreign keys
/// - Query patterns and table references
///
/// This enables full-stack symbol tracing from frontend → API → database schema.
pub struct SqlExtractor {
    base: BaseExtractor,
}

impl SqlExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
        }
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.visit_node(tree.root_node(), &mut symbols, None);
        symbols
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.extract_relationships_internal(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();

        // SQL type inference based on symbol metadata and signatures
        for symbol in symbols {
            if let Some(ref signature) = symbol.signature {
                // Extract SQL data types from signatures like "CREATE TABLE users (id INT, name VARCHAR(100))"
                let sql_type_pattern = regex::Regex::new(r"\b(INT|INTEGER|VARCHAR|TEXT|DECIMAL|FLOAT|BOOLEAN|DATE|TIMESTAMP|CHAR|BIGINT|SMALLINT)\b").unwrap();
                if let Some(type_match) = sql_type_pattern.find(signature) {
                    types.insert(symbol.id.clone(), type_match.as_str().to_uppercase());
                }
            }

            // Use metadata for SQL-specific types
            if symbol.metadata.get("isTable").and_then(|v| v.as_bool()).unwrap_or(false) {
                types.insert(symbol.id.clone(), "TABLE".to_string());
            }
            if symbol.metadata.get("isView").and_then(|v| v.as_bool()).unwrap_or(false) {
                types.insert(symbol.id.clone(), "VIEW".to_string());
            }
            if symbol.metadata.get("isStoredProcedure").and_then(|v| v.as_bool()).unwrap_or(false) {
                types.insert(symbol.id.clone(), "PROCEDURE".to_string());
            }
        }

        types
    }

    fn visit_node(&mut self, node: tree_sitter::Node, symbols: &mut Vec<Symbol>, parent_id: Option<&str>) {
        let mut symbol: Option<Symbol> = None;

        match node.kind() {
            "create_table" => {
                symbol = self.extract_table_definition(node, parent_id);
            }
            "create_procedure" | "create_function" | "create_function_statement" => {
                symbol = self.extract_stored_procedure(node, parent_id);
            }
            "create_view" => {
                symbol = self.extract_view(node, parent_id);
            }
            "create_index" => {
                symbol = self.extract_index(node, parent_id);
            }
            "create_trigger" => {
                symbol = self.extract_trigger(node, parent_id);
            }
            "cte" => {
                symbol = self.extract_cte(node, parent_id);
            }
            "create_schema" => {
                symbol = self.extract_schema(node, parent_id);
            }
            "create_sequence" => {
                symbol = self.extract_sequence(node, parent_id);
            }
            "create_domain" => {
                symbol = self.extract_domain(node, parent_id);
            }
            "create_type" => {
                symbol = self.extract_type(node, parent_id);
            }
            "alter_table" => {
                self.extract_constraints_from_alter_table(node, symbols, parent_id);
            }
            "select" => {
                // Extract SELECT query aliases as fields
                self.extract_select_aliases(node, symbols, parent_id);
            }
            "ERROR" => {
                // Handle DELIMITER syntax issues - extract multiple symbols from ERROR nodes
                self.extract_multiple_from_error_node(node, symbols, parent_id);
            }
            _ => {}
        }

        if let Some(symbol) = symbol {
            symbols.push(symbol.clone());

            // Extract additional child symbols for specific node types
            match node.kind() {
                "create_table" => {
                    self.extract_table_columns(node, symbols, &symbol.id);
                    self.extract_table_constraints(node, symbols, &symbol.id);
                }
                "create_view" => {
                    self.extract_view_columns(node, symbols, &symbol.id);
                }
                "ERROR" => {
                    let metadata = &symbol.metadata;
                    if metadata.get("isView").and_then(|v| v.as_bool()).unwrap_or(false) {
                        self.extract_view_columns_from_error_node(node, symbols, &symbol.id);
                    }
                    if metadata.get("isStoredProcedure").and_then(|v| v.as_bool()).unwrap_or(false) ||
                       metadata.get("isFunction").and_then(|v| v.as_bool()).unwrap_or(false) {
                        self.extract_parameters_from_error_node(node, symbols, &symbol.id);
                    }
                }
                "create_function" | "create_function_statement" => {
                    self.extract_declare_variables(node, symbols, &symbol.id);
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

    fn extract_table_definition(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Port Miller's exact logic: Look for table name inside object_reference node
        let object_ref_node = self.base.find_child_by_type(&node, "object_reference");
        let table_name_node = if let Some(obj_ref) = object_ref_node {
            self.base.find_child_by_type(&obj_ref, "identifier")
        } else {
            self.base.find_child_by_type(&node, "identifier")
                .or_else(|| self.base.find_child_by_type(&node, "table_name"))
        };

        let table_name_node = table_name_node?;
        let table_name = self.base.get_node_text(&table_name_node);

        let signature = self.extract_table_signature(node);

        let mut metadata = HashMap::new();
        metadata.insert("isTable".to_string(), Value::Bool(true));

        use crate::extractors::base::SymbolOptions;

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: None, // TODO: Add doc comment extraction like Miller
            metadata: Some(metadata),
        };

        Some(self.base.create_symbol(&node, table_name, SymbolKind::Class, options))
    }

    fn extract_table_signature(&self, node: tree_sitter::Node) -> String {
        // Look for table name inside object_reference node (same as extractTableDefinition)
        let object_ref_node = self.base.find_child_by_type(&node, "object_reference");
        let name_node = if let Some(obj_ref) = object_ref_node {
            self.base.find_child_by_type(&obj_ref, "identifier")
        } else {
            self.base.find_child_by_type(&node, "identifier")
                .or_else(|| self.base.find_child_by_type(&node, "table_name"))
        };

        let table_name = if let Some(name_node) = name_node {
            self.base.get_node_text(&name_node)
        } else {
            "unknown".to_string()
        };

        // Count columns for a brief signature
        let mut column_count = 0;
        self.base.traverse_tree(&node, &mut |child_node| {
            if child_node.kind() == "column_definition" {
                column_count += 1;
            }
        });

        format!("CREATE TABLE {} ({} columns)", table_name, column_count)
    }

    fn extract_table_columns(&mut self, table_node: tree_sitter::Node, symbols: &mut Vec<Symbol>, parent_table_id: &str) {
        // Use find_nodes_by_type to avoid borrowing conflicts
        let column_nodes = self.base.find_nodes_by_type(&table_node, "column_definition");

        for node in column_nodes {
            // Find column name from identifier or column_name nodes
            let column_name_node = self.base.find_child_by_type(&node, "identifier")
                .or_else(|| self.base.find_child_by_type(&node, "column_name"));

            if let Some(name_node) = column_name_node {
                let column_name = self.base.get_node_text(&name_node);

                // Find SQL data type nodes (port Miller's comprehensive type search)
                let data_type_node = self.base.find_child_by_type(&node, "data_type")
                    .or_else(|| self.base.find_child_by_type(&node, "type_name"))
                    .or_else(|| self.base.find_child_by_type(&node, "bigint"))
                    .or_else(|| self.base.find_child_by_type(&node, "varchar"))
                    .or_else(|| self.base.find_child_by_type(&node, "int"))
                    .or_else(|| self.base.find_child_by_type(&node, "text"))
                    .or_else(|| self.base.find_child_by_type(&node, "char"))
                    .or_else(|| self.base.find_child_by_type(&node, "decimal"))
                    .or_else(|| self.base.find_child_by_type(&node, "boolean"))
                    .or_else(|| self.base.find_child_by_type(&node, "keyword_boolean"))
                    .or_else(|| self.base.find_child_by_type(&node, "keyword_bigint"))
                    .or_else(|| self.base.find_child_by_type(&node, "keyword_varchar"))
                    .or_else(|| self.base.find_child_by_type(&node, "keyword_int"))
                    .or_else(|| self.base.find_child_by_type(&node, "keyword_text"))
                    .or_else(|| self.base.find_child_by_type(&node, "keyword_json"))
                    .or_else(|| self.base.find_child_by_type(&node, "json"))
                    .or_else(|| self.base.find_child_by_type(&node, "keyword_jsonb"))
                    .or_else(|| self.base.find_child_by_type(&node, "jsonb"))
                    .or_else(|| self.base.find_child_by_type(&node, "date"))
                    .or_else(|| self.base.find_child_by_type(&node, "timestamp"));

                let data_type = if let Some(type_node) = data_type_node {
                    self.base.get_node_text(&type_node)
                } else {
                    "unknown".to_string()
                };

                // Extract column constraints and build signature like Miller
                let constraints = self.extract_column_constraints(&node);
                let signature = format!("{}{}", data_type, constraints);

                use crate::extractors::base::SymbolOptions;

                let options = SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(crate::extractors::base::Visibility::Public),
                    parent_id: Some(parent_table_id.to_string()),
                    doc_comment: None,
                    metadata: None,
                };

                // Columns are fields within the table (Miller's strategy)
                let column_symbol = self.base.create_symbol(&node, column_name, SymbolKind::Field, options);
                symbols.push(column_symbol);
            }
        }
    }

    fn extract_column_constraints(&self, column_node: &tree_sitter::Node) -> String {
        // Port Miller's exact column constraints extraction logic
        let mut constraints: Vec<String> = Vec::new();
        let mut has_primary = false;
        let mut has_key = false;

        self.base.traverse_tree(&column_node, &mut |node| {
            match node.kind() {
                "primary_key_constraint" | "primary_key" => {
                    constraints.push("PRIMARY KEY".to_string());
                }
                "keyword_primary" => {
                    has_primary = true;
                }
                "keyword_key" => {
                    has_key = true;
                }
                "foreign_key_constraint" | "foreign_key" => {
                    constraints.push("FOREIGN KEY".to_string());
                }
                "not_null_constraint" | "not_null" => {
                    constraints.push("NOT NULL".to_string());
                }
                "keyword_not" => {
                    // Check if followed by keyword_null (Miller's logic)
                    if let Some(next_sibling) = node.next_sibling() {
                        if next_sibling.kind() == "keyword_null" {
                            constraints.push("NOT NULL".to_string());
                        }
                    }
                }
                "keyword_unique" | "unique_constraint" | "unique" => {
                    constraints.push("UNIQUE".to_string());
                }
                "check_constraint" => {
                    constraints.push("CHECK".to_string());
                }
                "keyword_default" => {
                    // Find the default value (Miller's logic)
                    if let Some(next_sibling) = node.next_sibling() {
                        let default_value = self.base.get_node_text(&next_sibling);
                        constraints.push(format!("DEFAULT {}", default_value));
                    }
                }
                _ => {}
            }
        });

        // Add PRIMARY KEY if both keywords found (Miller's logic)
        if has_primary && has_key {
            constraints.push("PRIMARY KEY".to_string());
        }

        // Return formatted string like Miller
        if constraints.is_empty() {
            String::new()
        } else {
            format!(" {}", constraints.join(" "))
        }
    }

    fn extract_table_constraints(&mut self, table_node: tree_sitter::Node, symbols: &mut Vec<Symbol>, parent_table_id: &str) {
        // Use find_nodes_by_type to avoid borrowing conflicts and node lifetime issues
        let constraint_nodes = self.base.find_nodes_by_type(&table_node, "constraint");

        for node in constraint_nodes {
            let mut constraint_type = "unknown";
            let mut constraint_name = format!("constraint_{}", node.start_position().row);

            // Determine constraint type based on child nodes (Miller's logic)
            let has_check = self.base.find_child_by_type(&node, "keyword_check").is_some();
            let has_primary = self.base.find_child_by_type(&node, "keyword_primary").is_some();
            let has_foreign = self.base.find_child_by_type(&node, "keyword_foreign").is_some();
            let has_unique = self.base.find_child_by_type(&node, "keyword_unique").is_some();
            let has_index = self.base.find_child_by_type(&node, "keyword_index").is_some();
            let named_constraint = self.base.find_child_by_type(&node, "identifier");

            if let Some(name_node) = named_constraint {
                constraint_name = self.base.get_node_text(&name_node);
            }

            // Determine constraint type (Miller's logic)
            if has_check {
                constraint_type = "check";
            } else if has_primary {
                constraint_type = "primary_key";
            } else if has_foreign {
                constraint_type = "foreign_key";
            } else if has_unique {
                constraint_type = "unique";
            } else if has_index {
                constraint_type = "index";
            }

            // Create constraint symbol like Miller
            let constraint_symbol = self.create_constraint_symbol(&node, constraint_type, parent_table_id, &constraint_name);
            symbols.push(constraint_symbol);
        }
    }

    fn create_constraint_symbol(&mut self, node: &tree_sitter::Node, constraint_type: &str, parent_table_id: &str, constraint_name: &str) -> Symbol {
        // Port Miller's createConstraintSymbol logic
        let signature = if constraint_type == "index" {
            format!("INDEX {}", constraint_name)
        } else {
            format!("CONSTRAINT {}", constraint_type.to_uppercase())
        };

        use crate::extractors::base::SymbolOptions;

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: Some(parent_table_id.to_string()),
            doc_comment: None,
            metadata: None,
        };

        // Constraints as Interface symbols (Miller's strategy)
        self.base.create_symbol(node, constraint_name.to_string(), SymbolKind::Interface, options)
    }

    fn extract_stored_procedure(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Port Miller's extractStoredProcedure logic for regular nodes (not just ERROR)
        // Look for function/procedure name - it may be inside an object_reference
        let object_ref_node = self.base.find_child_by_type(&node, "object_reference");
        let name_node = if let Some(obj_ref) = object_ref_node {
            self.base.find_child_by_type(&obj_ref, "identifier")
        } else {
            self.base.find_child_by_type(&node, "identifier")
                .or_else(|| self.base.find_child_by_type(&node, "procedure_name"))
                .or_else(|| self.base.find_child_by_type(&node, "function_name"))
        }?;

        let name = self.base.get_node_text(&name_node);
        let is_function = node.kind().contains("function");

        let signature = self.extract_procedure_signature(&node);

        let mut metadata = HashMap::new();
        metadata.insert("isFunction".to_string(), Value::Bool(is_function));
        metadata.insert("isStoredProcedure".to_string(), Value::Bool(true));

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: self.base.find_doc_comment(&node),
            metadata: Some(metadata),
        };

        let symbol_kind = if is_function { SymbolKind::Function } else { SymbolKind::Method };
        Some(self.base.create_symbol(&node, name, symbol_kind, options))
    }

    fn extract_procedure_signature(&self, node: &tree_sitter::Node) -> String {
        // Extract function/procedure name from object_reference if present
        let object_ref_node = self.base.find_child_by_type(node, "object_reference");
        let name_node = if let Some(obj_ref) = object_ref_node {
            self.base.find_child_by_type(&obj_ref, "identifier")
        } else {
            self.base.find_child_by_type(node, "identifier")
                .or_else(|| self.base.find_child_by_type(node, "procedure_name"))
                .or_else(|| self.base.find_child_by_type(node, "function_name"))
        };
        let name = if let Some(name_node) = name_node {
            self.base.get_node_text(&name_node)
        } else {
            "unknown".to_string()
        };

        // Extract parameter list
        let mut params: Vec<String> = Vec::new();
        self.base.traverse_tree(node, &mut |child_node| {
            if child_node.kind() == "parameter_declaration" || child_node.kind() == "parameter" {
                let param_name_node = self.base.find_child_by_type(&child_node, "identifier")
                    .or_else(|| self.base.find_child_by_type(&child_node, "parameter_name"));
                let type_node = self.base.find_child_by_type(&child_node, "data_type")
                    .or_else(|| self.base.find_child_by_type(&child_node, "type_name"));

                if let Some(param_name_node) = param_name_node {
                    let param_name = self.base.get_node_text(&param_name_node);
                    let param_type = if let Some(type_node) = type_node {
                        self.base.get_node_text(&type_node)
                    } else {
                        String::new()
                    };
                    params.push(if !param_type.is_empty() {
                        format!("{}: {}", param_name, param_type)
                    } else {
                        param_name
                    });
                }
            }
        });

        let is_function = node.kind().contains("function");
        let keyword = if is_function { "FUNCTION" } else { "PROCEDURE" };

        // For functions, try to extract the RETURNS clause and LANGUAGE
        let mut return_clause = String::new();
        let mut language_clause = String::new();
        if is_function {
            // Look for decimal node for RETURNS DECIMAL(10,2) - search recursively
            let decimal_nodes = self.base.find_nodes_by_type(node, "decimal");
            if !decimal_nodes.is_empty() {
                let decimal_text = self.base.get_node_text(&decimal_nodes[0]);
                return_clause = format!(" RETURNS {}", decimal_text);
            } else {
                // Look for other return types as direct children
                let return_type_nodes = ["keyword_boolean", "keyword_bigint", "keyword_int", "keyword_varchar", "keyword_text", "keyword_jsonb"];
                for type_str in &return_type_nodes {
                    if let Some(type_node) = self.base.find_child_by_type(node, type_str) {
                        let type_text = self.base.get_node_text(&type_node).replace("keyword_", "").to_uppercase();
                        return_clause = format!(" RETURNS {}", type_text);
                        break;
                    }
                }
            }

            // Look for LANGUAGE clause (PostgreSQL functions)
            if let Some(language_node) = self.base.find_child_by_type(node, "function_language") {
                let language_text = self.base.get_node_text(&language_node);
                language_clause = format!(" {}", language_text);
            }
        }

        format!("{} {}({}){}{}", keyword, name, params.join(", "), return_clause, language_clause)
    }

    fn extract_view(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Port of Miller's view extraction from error nodes
        let node_text = self.base.get_node_text(&node);

        // Extract views from ERROR nodes
        let view_regex = regex::Regex::new(r"CREATE\s+VIEW\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+AS").unwrap();

        if let Some(captures) = view_regex.captures(&node_text) {
            if let Some(view_name) = captures.get(1) {
                let name = view_name.as_str().to_string();

                let mut metadata = HashMap::new();
                metadata.insert("isView".to_string(), serde_json::Value::Bool(true));
                metadata.insert("extractedFromError".to_string(), serde_json::Value::Bool(true));

                let options = SymbolOptions {
                    signature: Some(format!("CREATE VIEW {}", name)),
                    visibility: Some(crate::extractors::base::Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    doc_comment: None,
                    metadata: Some(metadata),
                };

                return Some(self.base.create_symbol(&node, name, SymbolKind::Interface, options));
            }
        }

        None
    }

    fn extract_index(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Port Miller's extractIndex logic
        let name_node = self.base.find_child_by_type(&node, "identifier")
            .or_else(|| self.base.find_child_by_type(&node, "index_name"))?;

        let name = self.base.get_node_text(&name_node);

        // Get the full index text for signature
        let node_text = self.base.get_node_text(&node);
        let is_unique = node_text.contains("UNIQUE");

        // Build a more comprehensive signature that includes key parts
        let mut signature = if is_unique {
            format!("CREATE UNIQUE INDEX {}", name)
        } else {
            format!("CREATE INDEX {}", name)
        };

        // Add table and column information if found
        let on_regex = regex::Regex::new(r"ON\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
        if let Some(on_captures) = on_regex.captures(&node_text) {
            signature.push_str(&format!(" ON {}", on_captures.get(1).unwrap().as_str()));
        }

        // Add USING clause if present (before columns)
        let using_regex = regex::Regex::new(r"USING\s+([A-Z]+)").unwrap();
        if let Some(using_captures) = using_regex.captures(&node_text) {
            signature.push_str(&format!(" USING {}", using_captures.get(1).unwrap().as_str()));
        }

        // Add column information if found
        let column_regex = regex::Regex::new(r"(?:ON\s+[a-zA-Z_][a-zA-Z0-9_]*(?:\s+USING\s+[A-Z]+)?\s*)?(\([^)]+\))").unwrap();
        if let Some(column_captures) = column_regex.captures(&node_text) {
            signature.push_str(&format!(" {}", column_captures.get(1).unwrap().as_str()));
        }

        // Add INCLUDE clause if present
        let include_regex = regex::Regex::new(r"INCLUDE\s*(\([^)]+\))").unwrap();
        if let Some(include_captures) = include_regex.captures(&node_text) {
            signature.push_str(&format!(" INCLUDE {}", include_captures.get(1).unwrap().as_str()));
        }

        // Add WHERE clause if present
        let where_regex = regex::Regex::new(r"WHERE\s+(.+?)(?:;|$)").unwrap();
        if let Some(where_captures) = where_regex.captures(&node_text) {
            signature.push_str(&format!(" WHERE {}", where_captures.get(1).unwrap().as_str().trim()));
        }

        let mut metadata = HashMap::new();
        metadata.insert("isIndex".to_string(), serde_json::Value::Bool(true));
        metadata.insert("isUnique".to_string(), serde_json::Value::Bool(is_unique));

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: None,
            metadata: Some(metadata),
        };

        Some(self.base.create_symbol(&node, name, SymbolKind::Property, options))
    }

    fn extract_trigger(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Port Miller's extractTrigger logic
        let name_node = self.base.find_child_by_type(&node, "identifier")
            .or_else(|| self.base.find_child_by_type(&node, "trigger_name"))?;

        let name = self.base.get_node_text(&name_node);

        let mut metadata = HashMap::new();
        metadata.insert("isTrigger".to_string(), Value::Bool(true));

        let options = SymbolOptions {
            signature: Some(format!("TRIGGER {}", name)),
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: self.base.find_doc_comment(&node),
            metadata: Some(metadata),
        };

        Some(self.base.create_symbol(&node, name, SymbolKind::Method, options))
    }

    fn extract_cte(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Port of Miller's extractCte method
        // Extract CTE name from identifier child
        let name_node = self.base.find_child_by_type(&node, "identifier")?;
        let name = self.base.get_node_text(&name_node);

        // Check if this is a recursive CTE by looking for RECURSIVE keyword in the parent context
        let mut signature = format!("WITH {} AS (...)", name);
        let parent_node = node.parent();
        if let Some(parent) = parent_node {
            let parent_text = self.base.get_node_text(&parent);
            if parent_text.contains("RECURSIVE") {
                signature = format!("WITH RECURSIVE {} AS (...)", name);
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert("isCte".to_string(), serde_json::Value::Bool(true));
        metadata.insert("isTemporaryView".to_string(), serde_json::Value::Bool(true));

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(crate::extractors::base::Visibility::Public), // CTEs are accessible within the query
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: self.base.find_doc_comment(&node),
            metadata: Some(metadata),
        };

        Some(self.base.create_symbol(&node, name, SymbolKind::Interface, options))
    }

    fn extract_schema(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Port of Miller's schema extraction from error nodes
        let node_text = self.base.get_node_text(&node);

        // Extract schemas from ERROR nodes
        let schema_regex = regex::Regex::new(r"CREATE\s+SCHEMA\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();

        if let Some(captures) = schema_regex.captures(&node_text) {
            if let Some(schema_name) = captures.get(1) {
                let name = schema_name.as_str().to_string();

                let mut metadata = HashMap::new();
                metadata.insert("isSchema".to_string(), serde_json::Value::Bool(true));
                metadata.insert("extractedFromError".to_string(), serde_json::Value::Bool(true));

                let options = SymbolOptions {
                    signature: Some(format!("CREATE SCHEMA {}", name)),
                    visibility: Some(crate::extractors::base::Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    doc_comment: None,
                    metadata: Some(metadata),
                };

                return Some(self.base.create_symbol(&node, name, SymbolKind::Namespace, options));
            }
        }

        None
    }

    fn extract_sequence(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Port Miller's extractSequence logic
        // Look for sequence name - it may be inside an object_reference
        let object_ref_node = self.base.find_child_by_type(&node, "object_reference");
        let name_node = if let Some(obj_ref) = object_ref_node {
            self.base.find_child_by_type(&obj_ref, "identifier")
        } else {
            self.base.find_child_by_type(&node, "identifier")
                .or_else(|| self.base.find_child_by_type(&node, "sequence_name"))
        }?;

        let name = self.base.get_node_text(&name_node);

        // Build sequence signature with options
        let node_text = self.base.get_node_text(&node);
        let mut signature = format!("CREATE SEQUENCE {}", name);

        // Add sequence options if present
        let mut options_vec = Vec::new();

        if let Some(start_match) = regex::Regex::new(r"START\s+WITH\s+(\d+)").unwrap().captures(&node_text) {
            options_vec.push(format!("START WITH {}", start_match.get(1).unwrap().as_str()));
        }

        if let Some(inc_match) = regex::Regex::new(r"INCREMENT\s+BY\s+(\d+)").unwrap().captures(&node_text) {
            options_vec.push(format!("INCREMENT BY {}", inc_match.get(1).unwrap().as_str()));
        }

        if let Some(min_match) = regex::Regex::new(r"MINVALUE\s+(\d+)").unwrap().captures(&node_text) {
            options_vec.push(format!("MINVALUE {}", min_match.get(1).unwrap().as_str()));
        }

        if let Some(max_match) = regex::Regex::new(r"MAXVALUE\s+(\d+)").unwrap().captures(&node_text) {
            options_vec.push(format!("MAXVALUE {}", max_match.get(1).unwrap().as_str()));
        }

        if let Some(cache_match) = regex::Regex::new(r"CACHE\s+(\d+)").unwrap().captures(&node_text) {
            options_vec.push(format!("CACHE {}", cache_match.get(1).unwrap().as_str()));
        }

        if !options_vec.is_empty() {
            signature.push_str(&format!(" ({})", options_vec.join(", ")));
        }

        let mut metadata = HashMap::new();
        metadata.insert("isSequence".to_string(), serde_json::Value::Bool(true));

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: None,
            metadata: Some(metadata),
        };

        Some(self.base.create_symbol(&node, name, SymbolKind::Variable, options))
    }

    fn extract_domain(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Port Miller's extractDomain logic
        // Look for domain name - it may be inside an object_reference
        let object_ref_node = self.base.find_child_by_type(&node, "object_reference");
        let name_node = if let Some(obj_ref) = object_ref_node {
            self.base.find_child_by_type(&obj_ref, "identifier")
        } else {
            self.base.find_child_by_type(&node, "identifier")
                .or_else(|| self.base.find_child_by_type(&node, "domain_name"))
        }?;

        let name = self.base.get_node_text(&name_node);

        // Build domain signature with base type and constraints
        let node_text = self.base.get_node_text(&node);
        let mut signature = format!("CREATE DOMAIN {}", name);

        // Extract the base type (AS datatype)
        if let Some(as_match) = regex::Regex::new(r"AS\s+([A-Za-z]+(?:\(\d+(?:,\s*\d+)?\))?)").unwrap().captures(&node_text) {
            signature.push_str(&format!(" AS {}", as_match.get(1).unwrap().as_str()));
        }

        // Add CHECK constraint if present
        if let Some(check_match) = regex::Regex::new(r"CHECK\s*\(([^)]+(?:\([^)]*\)[^)]*)*)\)").unwrap().captures(&node_text) {
            signature.push_str(&format!(" CHECK ({})", check_match.get(1).unwrap().as_str().trim()));
        }

        let mut metadata = HashMap::new();
        metadata.insert("isDomain".to_string(), serde_json::Value::Bool(true));

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: None,
            metadata: Some(metadata),
        };

        Some(self.base.create_symbol(&node, name, SymbolKind::Class, options))
    }

    fn extract_type(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Port Miller's extractType logic
        // Look for type name in object_reference
        let object_ref_node = self.base.find_child_by_type(&node, "object_reference");
        let name_node = if let Some(obj_ref) = object_ref_node {
            self.base.find_child_by_type(&obj_ref, "identifier")
        } else {
            self.base.find_child_by_type(&node, "identifier")
        }?;

        let name = self.base.get_node_text(&name_node);

        // Check if this is an ENUM type
        let node_text = self.base.get_node_text(&node);
        if node_text.contains("AS ENUM") {
            // Extract enum values from enum_elements
            let enum_elements_node = self.base.find_child_by_type(&node, "enum_elements");
            let enum_values = if let Some(elements) = enum_elements_node {
                self.base.get_node_text(&elements)
            } else {
                String::new()
            };

            let signature = format!("CREATE TYPE {} AS ENUM {}", name, enum_values);

            let mut metadata = HashMap::new();
            metadata.insert("isEnum".to_string(), serde_json::Value::Bool(true));
            metadata.insert("isType".to_string(), serde_json::Value::Bool(true));

            let options = SymbolOptions {
                signature: Some(signature),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                doc_comment: None,
                metadata: Some(metadata),
            };

            return Some(self.base.create_symbol(&node, name, SymbolKind::Class, options));
        }

        // Handle other types (non-enum)
        let signature = format!("CREATE TYPE {}", name);

        let mut metadata = HashMap::new();
        metadata.insert("isType".to_string(), serde_json::Value::Bool(true));

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: None,
            metadata: Some(metadata),
        };

        Some(self.base.create_symbol(&node, name, SymbolKind::Class, options))
    }

    fn extract_constraints_from_alter_table(&mut self, node: tree_sitter::Node, symbols: &mut Vec<Symbol>, parent_id: Option<&str>) {
        // Port Miller's extractConstraintsFromAlterTable logic
        let node_text = self.base.get_node_text(&node);

        // Extract ADD CONSTRAINT statements
        let constraint_regex = regex::Regex::new(r"ADD\s+CONSTRAINT\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+(CHECK|FOREIGN\s+KEY|UNIQUE|PRIMARY\s+KEY)").unwrap();
        if let Some(captures) = constraint_regex.captures(&node_text) {
            if let Some(constraint_name) = captures.get(1) {
                let name = constraint_name.as_str().to_string();
                let constraint_type = captures.get(2).unwrap().as_str().to_uppercase();

                let mut signature = format!("ALTER TABLE ADD CONSTRAINT {} {}", name, constraint_type);

                // Add more details based on constraint type
                if constraint_type == "CHECK" {
                    let check_regex = regex::Regex::new(r"CHECK\s*\(([^)]+(?:\([^)]*\)[^)]*)*)").unwrap();
                    if let Some(check_captures) = check_regex.captures(&node_text) {
                        signature.push_str(&format!(" ({})", check_captures.get(1).unwrap().as_str().trim()));
                    }
                } else if constraint_type.contains("FOREIGN") {
                    let fk_regex = regex::Regex::new(r"FOREIGN\s+KEY\s*\(([^)]+)\)\s*REFERENCES\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
                    if let Some(fk_captures) = fk_regex.captures(&node_text) {
                        signature.push_str(&format!(" ({}) REFERENCES {}", fk_captures.get(1).unwrap().as_str(), fk_captures.get(2).unwrap().as_str()));
                    }

                    // Add ON DELETE/UPDATE actions
                    let on_delete_regex = regex::Regex::new(r"ON\s+DELETE\s+(CASCADE|RESTRICT|SET\s+NULL|NO\s+ACTION)").unwrap();
                    if let Some(on_delete_captures) = on_delete_regex.captures(&node_text) {
                        signature.push_str(&format!(" ON DELETE {}", on_delete_captures.get(1).unwrap().as_str().to_uppercase()));
                    }

                    let on_update_regex = regex::Regex::new(r"ON\s+UPDATE\s+(CASCADE|RESTRICT|SET\s+NULL|NO\s+ACTION)").unwrap();
                    if let Some(on_update_captures) = on_update_regex.captures(&node_text) {
                        signature.push_str(&format!(" ON UPDATE {}", on_update_captures.get(1).unwrap().as_str().to_uppercase()));
                    }
                }

                let mut metadata = HashMap::new();
                metadata.insert("isConstraint".to_string(), Value::Bool(true));
                metadata.insert("constraintType".to_string(), Value::String(constraint_type.clone()));

                let options = SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(crate::extractors::base::Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    doc_comment: None,
                    metadata: Some(metadata),
                };

                let constraint_symbol = self.base.create_symbol(&node, name, SymbolKind::Property, options);
                symbols.push(constraint_symbol);
            }
        }
    }

    fn extract_select_aliases(&mut self, select_node: tree_sitter::Node, symbols: &mut Vec<Symbol>, parent_id: Option<&str>) {
        // Port Miller's extractSelectAliases logic using iterative approach to avoid borrow checker issues
        // Find all 'term' nodes first to avoid borrowing conflicts
        let term_nodes = self.base.find_nodes_by_type(&select_node, "term");

        for node in term_nodes {
            // Look for 'term' nodes that contain [expression, keyword_as, identifier] pattern
            let mut children = Vec::new();
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    children.push(child);
                }
            }

            if children.len() >= 3 {
                // Check if this term has the pattern [expression, keyword_as, identifier]
                for i in 0..(children.len() - 2) {
                    if children[i + 1].kind() == "keyword_as" && children[i + 2].kind() == "identifier" {
                        let expr_node = children[i];
                        let _as_node = children[i + 1];
                        let alias_node = children[i + 2];

                        let alias_name = self.base.get_node_text(&alias_node);
                        let expr_text = self.base.get_node_text(&expr_node);

                        // Determine expression type for better signatures
                        let expression = match expr_node.kind() {
                            "case" => "CASE expression".to_string(),
                            "window_function" => {
                                // Keep the OVER clause in the signature for window functions
                                if expr_text.contains("OVER (") {
                                    if let Some(over_index) = expr_text.find("OVER (") {
                                        if let Some(end_index) = expr_text[over_index..].find(')') {
                                            expr_text[0..over_index + end_index + 1].to_string()
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
                                            expr_text[0..over_index + end_index + 1].to_string()
                                        } else {
                                            expr_text.clone()
                                        }
                                    } else {
                                        expr_text.clone()
                                    }
                                } else if expr_text.contains("COUNT") || expr_text.contains("SUM") || expr_text.contains("AVG") {
                                    "aggregate function".to_string()
                                } else {
                                    if expr_text.len() > 30 {
                                        format!("{}...", &expr_text[0..30])
                                    } else {
                                        expr_text.clone()
                                    }
                                }
                            }
                        };

                        let signature = format!("{} AS {}", expression, alias_name);

                        let mut metadata = HashMap::new();
                        metadata.insert("isSelectAlias".to_string(), serde_json::Value::Bool(true));
                        metadata.insert("isComputedField".to_string(), serde_json::Value::Bool(true));

                        let options = SymbolOptions {
                            signature: Some(signature),
                            visibility: Some(crate::extractors::base::Visibility::Public),
                            parent_id: parent_id.map(|s| s.to_string()),
                            doc_comment: None,
                            metadata: Some(metadata),
                        };

                        let alias_symbol = self.base.create_symbol(&alias_node, alias_name, SymbolKind::Field, options);
                        symbols.push(alias_symbol);
                        break; // Found the alias in this term, move to next term
                    }
                }
            }
        }
    }

    fn extract_multiple_from_error_node(&mut self, node: tree_sitter::Node, symbols: &mut Vec<Symbol>, parent_id: Option<&str>) {
        // Port Miller's extractMultipleFromErrorNode logic
        let error_text = self.base.get_node_text(&node);

        // Extract stored procedures from DELIMITER syntax
        let procedure_regex = regex::Regex::new(r"CREATE\s+PROCEDURE\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
        if let Some(captures) = procedure_regex.captures(&error_text) {
            if let Some(procedure_name) = captures.get(1) {
                let name = procedure_name.as_str().to_string();

                let mut metadata = HashMap::new();
                metadata.insert("isStoredProcedure".to_string(), serde_json::Value::Bool(true));
                metadata.insert("extractedFromError".to_string(), serde_json::Value::Bool(true));

                let options = SymbolOptions {
                    signature: Some(format!("CREATE PROCEDURE {}(...)", name)),
                    visibility: Some(crate::extractors::base::Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    doc_comment: None,
                    metadata: Some(metadata),
                };

                let procedure_symbol = self.base.create_symbol(&node, name.clone(), SymbolKind::Function, options);
                symbols.push(procedure_symbol.clone());
                // Extract parameters for this procedure
                self.extract_parameters_from_error_node(node, symbols, &procedure_symbol.id);
            }
        }

        // Extract functions with RETURNS clause
        let function_regex = regex::Regex::new(r"CREATE\s+(?:OR\s+REPLACE\s+)?FUNCTION\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\([^)]*\)\s*RETURNS?\s+([A-Z0-9(),\s]+)").unwrap();
        if let Some(captures) = function_regex.captures(&error_text) {
            if let Some(function_name) = captures.get(1) {
                let name = function_name.as_str().to_string();
                let return_type = captures.get(2).map(|m| m.as_str().trim().to_string()).unwrap_or_default();

                let mut metadata = HashMap::new();
                metadata.insert("isFunction".to_string(), serde_json::Value::Bool(true));
                metadata.insert("extractedFromError".to_string(), serde_json::Value::Bool(true));
                metadata.insert("returnType".to_string(), serde_json::Value::String(return_type.clone()));

                let options = SymbolOptions {
                    signature: Some(format!("CREATE FUNCTION {}(...) RETURNS {}", name, return_type)),
                    visibility: Some(crate::extractors::base::Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    doc_comment: None,
                    metadata: Some(metadata),
                };

                let function_symbol = self.base.create_symbol(&node, name.clone(), SymbolKind::Function, options);
                symbols.push(function_symbol.clone());
                // Extract DECLARE variables from function body
                self.extract_declare_variables(node, symbols, &function_symbol.id);
            }
        } else {
            // Fallback: Extract any CREATE FUNCTION
            let simple_function_regex = regex::Regex::new(r"CREATE\s+(?:OR\s+REPLACE\s+)?FUNCTION\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
            if let Some(captures) = simple_function_regex.captures(&error_text) {
                if let Some(function_name) = captures.get(1) {
                    let name = function_name.as_str().to_string();

                    let mut metadata = HashMap::new();
                    metadata.insert("isFunction".to_string(), serde_json::Value::Bool(true));
                    metadata.insert("extractedFromError".to_string(), serde_json::Value::Bool(true));

                    let options = SymbolOptions {
                        signature: Some(format!("CREATE FUNCTION {}(...)", name)),
                        visibility: Some(crate::extractors::base::Visibility::Public),
                        parent_id: parent_id.map(|s| s.to_string()),
                        doc_comment: None,
                        metadata: Some(metadata),
                    };

                    let function_symbol = self.base.create_symbol(&node, name.clone(), SymbolKind::Function, options);
                    symbols.push(function_symbol.clone());
                    // Extract DECLARE variables from function body
                    self.extract_declare_variables(node, symbols, &function_symbol.id);
                }
            }
        }

        // Extract schemas from ERROR nodes
        let schema_regex = regex::Regex::new(r"CREATE\s+SCHEMA\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
        if let Some(captures) = schema_regex.captures(&error_text) {
            if let Some(schema_name) = captures.get(1) {
                let name = schema_name.as_str().to_string();

                let mut metadata = HashMap::new();
                metadata.insert("isSchema".to_string(), serde_json::Value::Bool(true));
                metadata.insert("extractedFromError".to_string(), serde_json::Value::Bool(true));

                let options = SymbolOptions {
                    signature: Some(format!("CREATE SCHEMA {}", name)),
                    visibility: Some(crate::extractors::base::Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    doc_comment: None,
                    metadata: Some(metadata),
                };

                let schema_symbol = self.base.create_symbol(&node, name, SymbolKind::Namespace, options);
                symbols.push(schema_symbol);
            }
        }

        // Extract views from ERROR nodes
        let view_regex = regex::Regex::new(r"CREATE\s+VIEW\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+AS").unwrap();
        if let Some(captures) = view_regex.captures(&error_text) {
            if let Some(view_name) = captures.get(1) {
                let name = view_name.as_str().to_string();

                let mut metadata = HashMap::new();
                metadata.insert("isView".to_string(), serde_json::Value::Bool(true));
                metadata.insert("extractedFromError".to_string(), serde_json::Value::Bool(true));

                let options = SymbolOptions {
                    signature: Some(format!("CREATE VIEW {}", name)),
                    visibility: Some(crate::extractors::base::Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    doc_comment: None,
                    metadata: Some(metadata),
                };

                let view_symbol = self.base.create_symbol(&node, name.clone(), SymbolKind::Interface, options);
                symbols.push(view_symbol.clone());
                // Extract view columns from this ERROR node
                self.extract_view_columns_from_error_node(node, symbols, &view_symbol.id);
            }
        }

        // Extract triggers from ERROR nodes
        let trigger_regex = regex::Regex::new(r"CREATE\s+TRIGGER\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
        if let Some(captures) = trigger_regex.captures(&error_text) {
            if let Some(trigger_name) = captures.get(1) {
                let name = trigger_name.as_str().to_string();

                // Try to extract trigger details (BEFORE/AFTER, event, table)
                let details_regex = regex::Regex::new(r"CREATE\s+TRIGGER\s+[a-zA-Z_][a-zA-Z0-9_]*\s+(BEFORE|AFTER)\s+(INSERT|UPDATE|DELETE)\s+ON\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();

                let mut signature = format!("CREATE TRIGGER {}", name);
                if let Some(details_captures) = details_regex.captures(&error_text) {
                    let timing = details_captures.get(1).unwrap().as_str();
                    let event = details_captures.get(2).unwrap().as_str();
                    let table = details_captures.get(3).unwrap().as_str();
                    signature = format!("CREATE TRIGGER {} {} {} ON {}", name, timing, event, table);
                }

                let mut metadata = HashMap::new();
                metadata.insert("isTrigger".to_string(), serde_json::Value::Bool(true));
                metadata.insert("extractedFromError".to_string(), serde_json::Value::Bool(true));

                let options = SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(crate::extractors::base::Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    doc_comment: None,
                    metadata: Some(metadata),
                };

                let trigger_symbol = self.base.create_symbol(&node, name, SymbolKind::Method, options);
                symbols.push(trigger_symbol);
            }
        }

        // Extract constraints from ALTER TABLE statements
        let constraint_regex = regex::Regex::new(r"ALTER\s+TABLE\s+[a-zA-Z_][a-zA-Z0-9_]*\s+ADD\s+CONSTRAINT\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+(CHECK|FOREIGN\s+KEY|UNIQUE|PRIMARY\s+KEY)").unwrap();
        if let Some(captures) = constraint_regex.captures(&error_text) {
            if let Some(constraint_name) = captures.get(1) {
                let name = constraint_name.as_str().to_string();
                let constraint_type = captures.get(2).unwrap().as_str().to_uppercase();

                let mut signature = format!("ALTER TABLE ADD CONSTRAINT {} {}", name, constraint_type);

                // Add more details based on constraint type
                if constraint_type == "CHECK" {
                    let check_regex = regex::Regex::new(r"CHECK\s*\(([^)]+(?:\([^)]*\)[^)]*)*)\)").unwrap();
                    if let Some(check_captures) = check_regex.captures(&error_text) {
                        signature.push_str(&format!(" ({})", check_captures.get(1).unwrap().as_str().trim()));
                    }
                } else if constraint_type.contains("FOREIGN") {
                    let fk_regex = regex::Regex::new(r"FOREIGN\s+KEY\s*\(([^)]+)\)\s*REFERENCES\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
                    if let Some(fk_captures) = fk_regex.captures(&error_text) {
                        signature.push_str(&format!(" ({}) REFERENCES {}", fk_captures.get(1).unwrap().as_str(), fk_captures.get(2).unwrap().as_str()));
                    }

                    // Add ON DELETE/UPDATE actions
                    let on_delete_regex = regex::Regex::new(r"ON\s+DELETE\s+(CASCADE|RESTRICT|SET\s+NULL|NO\s+ACTION)").unwrap();
                    if let Some(on_delete_captures) = on_delete_regex.captures(&error_text) {
                        signature.push_str(&format!(" ON DELETE {}", on_delete_captures.get(1).unwrap().as_str().to_uppercase()));
                    }

                    let on_update_regex = regex::Regex::new(r"ON\s+UPDATE\s+(CASCADE|RESTRICT|SET\s+NULL|NO\s+ACTION)").unwrap();
                    if let Some(on_update_captures) = on_update_regex.captures(&error_text) {
                        signature.push_str(&format!(" ON UPDATE {}", on_update_captures.get(1).unwrap().as_str().to_uppercase()));
                    }
                }

                let mut metadata = HashMap::new();
                metadata.insert("isConstraint".to_string(), serde_json::Value::Bool(true));
                metadata.insert("constraintType".to_string(), serde_json::Value::String(constraint_type.clone()));
                metadata.insert("extractedFromError".to_string(), serde_json::Value::Bool(true));

                let options = SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(crate::extractors::base::Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    doc_comment: None,
                    metadata: Some(metadata),
                };

                let constraint_symbol = self.base.create_symbol(&node, name, SymbolKind::Property, options);
                symbols.push(constraint_symbol);
            }
        }

        // Extract domains
        let domain_regex = regex::Regex::new(r"CREATE\s+DOMAIN\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+AS\s+([A-Za-z]+(?:\(\d+(?:,\s*\d+)?\))?)").unwrap();
        if let Some(captures) = domain_regex.captures(&error_text) {
            if let Some(domain_name) = captures.get(1) {
                let name = domain_name.as_str().to_string();
                let base_type = captures.get(2).unwrap().as_str().to_string();

                let mut signature = format!("CREATE DOMAIN {} AS {}", name, base_type);

                // Add CHECK constraint if present
                let check_regex = regex::Regex::new(r"CHECK\s*\(([^)]+(?:\([^)]*\)[^)]*)*)\)").unwrap();
                if let Some(check_captures) = check_regex.captures(&error_text) {
                    signature.push_str(&format!(" CHECK ({})", check_captures.get(1).unwrap().as_str().trim()));
                }

                let mut metadata = HashMap::new();
                metadata.insert("isDomain".to_string(), serde_json::Value::Bool(true));
                metadata.insert("extractedFromError".to_string(), serde_json::Value::Bool(true));
                metadata.insert("baseType".to_string(), serde_json::Value::String(base_type));

                let options = SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(crate::extractors::base::Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    doc_comment: None,
                    metadata: Some(metadata),
                };

                let domain_symbol = self.base.create_symbol(&node, name, SymbolKind::Class, options);
                symbols.push(domain_symbol);
            }
        }

        // Extract enum/custom types
        let enum_regex = regex::Regex::new(r"CREATE\s+TYPE\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+AS\s+ENUM\s*\(([\s\S]*?)\)").unwrap();
        if let Some(captures) = enum_regex.captures(&error_text) {
            if let Some(enum_name) = captures.get(1) {
                let name = enum_name.as_str().to_string();
                let enum_values = captures.get(2).unwrap().as_str();

                let signature = format!("CREATE TYPE {} AS ENUM ({})", name, enum_values.trim());

                let mut metadata = HashMap::new();
                metadata.insert("isEnum".to_string(), serde_json::Value::Bool(true));
                metadata.insert("extractedFromError".to_string(), serde_json::Value::Bool(true));

                let options = SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(crate::extractors::base::Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    doc_comment: None,
                    metadata: Some(metadata),
                };

                let enum_symbol = self.base.create_symbol(&node, name, SymbolKind::Class, options);
                symbols.push(enum_symbol);
            }
        }

        // Extract aggregate functions
        let aggregate_regex = regex::Regex::new(r"CREATE\s+AGGREGATE\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\(([^)]*)\)").unwrap();
        if let Some(captures) = aggregate_regex.captures(&error_text) {
            if let Some(aggregate_name) = captures.get(1) {
                let name = aggregate_name.as_str().to_string();
                let parameters = captures.get(2).unwrap().as_str();

                let signature = format!("CREATE AGGREGATE {}({})", name, parameters);

                let mut metadata = HashMap::new();
                metadata.insert("isAggregate".to_string(), serde_json::Value::Bool(true));
                metadata.insert("extractedFromError".to_string(), serde_json::Value::Bool(true));

                let options = SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(crate::extractors::base::Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    doc_comment: None,
                    metadata: Some(metadata),
                };

                let aggregate_symbol = self.base.create_symbol(&node, name, SymbolKind::Function, options);
                symbols.push(aggregate_symbol);
            }
        }
    }

    fn extract_view_columns(&mut self, view_node: tree_sitter::Node, symbols: &mut Vec<Symbol>, parent_view_id: &str) {
        // Port Miller's extractViewColumns logic
        // Look for the SELECT statement inside the view and extract its aliases
        let nodes = self.base.find_nodes_by_type(&view_node, "select_statement");
        for select_node in nodes {
            self.extract_select_aliases(select_node, symbols, Some(parent_view_id));
        }

        // Also check for just "select" nodes
        let select_nodes = self.base.find_nodes_by_type(&view_node, "select");
        for select_node in select_nodes {
            self.extract_select_aliases(select_node, symbols, Some(parent_view_id));
        }
    }

    fn extract_view_columns_from_error_node(&mut self, node: tree_sitter::Node, symbols: &mut Vec<Symbol>, parent_view_id: &str) {
        // TODO: Implement view columns from ERROR node extraction
        // This is a stub that will be implemented as we port Miller's logic
    }

    fn extract_parameters_from_error_node(&mut self, node: tree_sitter::Node, symbols: &mut Vec<Symbol>, parent_id: &str) {
        // Port Miller's extractParametersFromErrorNode logic
        let error_text = self.base.get_node_text(&node);

        // Extract parameters from procedure/function definitions
        // Look for patterns like "IN p_user_id BIGINT", "OUT p_total_events INT"
        let param_regex = regex::Regex::new(r"(IN|OUT|INOUT)?\s*([a-zA-Z_][a-zA-Z0-9_]*)\s+(BIGINT|INT|VARCHAR|DECIMAL|DATE|BOOLEAN|TEXT|JSONB)").unwrap();

        for captures in param_regex.captures_iter(&error_text) {
            let direction = captures.get(1).map(|m| m.as_str()).unwrap_or("IN"); // Default to IN if not specified
            let param_name = captures.get(2).unwrap().as_str();
            let param_type = captures.get(3).unwrap().as_str();

            // Don't extract procedure/function names as parameters
            if !error_text.contains(&format!("PROCEDURE {}", param_name)) &&
               !error_text.contains(&format!("FUNCTION {}", param_name)) {

                let signature = format!("{} {} {}", direction, param_name, param_type);

                let mut metadata = HashMap::new();
                metadata.insert("isParameter".to_string(), serde_json::Value::Bool(true));
                metadata.insert("extractedFromError".to_string(), serde_json::Value::Bool(true));

                let options = SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(crate::extractors::base::Visibility::Public),
                    parent_id: Some(parent_id.to_string()),
                    doc_comment: None,
                    metadata: Some(metadata),
                };

                let param_symbol = self.base.create_symbol(&node, param_name.to_string(), SymbolKind::Variable, options);
                symbols.push(param_symbol);
            }
        }
    }

    fn extract_declare_variables(&mut self, function_node: tree_sitter::Node, symbols: &mut Vec<Symbol>, parent_id: &str) {
        // Port Miller's extractDeclareVariables logic
        let function_text = self.base.get_node_text(&function_node);

        // Look for DECLARE statements within function bodies
        // Replaced closure with iterative approach to avoid borrow checker issues
        let mut nodes_to_process = vec![function_node];
        while let Some(node) = nodes_to_process.pop() {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                nodes_to_process.push(child);
            }
            // PostgreSQL style: function_declaration nodes like "v_current_prefs JSONB;"
            if node.kind() == "function_declaration" {
                // Parse the declaration text to extract variable name and type
                let declaration_raw = self.base.get_node_text(&node);
                let declaration_text = declaration_raw.trim();
                // Match patterns like "v_current_prefs JSONB;" or "v_score DECIMAL(10,2) DEFAULT 0.0;"
                let var_regex = regex::Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\s+([A-Z0-9(),\s]+)").unwrap();

                if let Some(captures) = var_regex.captures(declaration_text) {
                    let variable_name = captures.get(1).unwrap().as_str();
                    let variable_type = captures.get(2).unwrap().as_str().split_whitespace().next().unwrap_or("unknown"); // Get first word as type

                    let mut metadata = HashMap::new();
                    metadata.insert("isLocalVariable".to_string(), serde_json::Value::Bool(true));
                    metadata.insert("isDeclaredVariable".to_string(), serde_json::Value::Bool(true));

                    let options = SymbolOptions {
                        signature: Some(format!("DECLARE {} {}", variable_name, variable_type)),
                        visibility: Some(crate::extractors::base::Visibility::Private),
                        parent_id: Some(parent_id.to_string()),
                        doc_comment: None,
                        metadata: Some(metadata),
                    };

                    let variable_symbol = self.base.create_symbol(&node, variable_name.to_string(), SymbolKind::Variable, options);
                    symbols.push(variable_symbol);
                }
            }
            // MySQL style: keyword_declare followed by identifier and type
            else if node.kind() == "keyword_declare" {
                // For MySQL DECLARE statements, look for the pattern in the surrounding text
                if let Some(parent) = node.parent() {
                    let parent_text = self.base.get_node_text(&parent);

                    // Look for DECLARE patterns in the parent text
                    let declare_regex = regex::Regex::new(r"DECLARE\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+(DECIMAL\([^)]+\)|INT|BIGINT|VARCHAR\([^)]+\)|TEXT|BOOLEAN)").unwrap();

                    for captures in declare_regex.captures_iter(&parent_text) {
                        let variable_name = captures.get(1).unwrap().as_str();
                        let variable_type = captures.get(2).unwrap().as_str();

                        let mut metadata = HashMap::new();
                        metadata.insert("isLocalVariable".to_string(), serde_json::Value::Bool(true));
                        metadata.insert("isDeclaredVariable".to_string(), serde_json::Value::Bool(true));

                        let options = SymbolOptions {
                            signature: Some(format!("DECLARE {} {}", variable_name, variable_type)),
                            visibility: Some(crate::extractors::base::Visibility::Private),
                            parent_id: Some(parent_id.to_string()),
                            doc_comment: None,
                            metadata: Some(metadata),
                        };

                        let variable_symbol = self.base.create_symbol(&node, variable_name.to_string(), SymbolKind::Variable, options);
                        symbols.push(variable_symbol);
                    }
                }
            }
        }

        // Also extract DECLARE variables directly from function text using regex
        let declare_regex = regex::Regex::new(r"DECLARE\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+(DECIMAL\([^)]+\)|JSONB|INT|BIGINT|VARCHAR\([^)]+\)|TEXT|BOOLEAN)").unwrap();
        for captures in declare_regex.captures_iter(&function_text) {
            let variable_name = captures.get(1).unwrap().as_str();
            let variable_type = captures.get(2).unwrap().as_str();

            // Only add if not already added from tree traversal
            if !symbols.iter().any(|s| s.name == variable_name && s.parent_id.as_ref().map(|p| p.as_str()) == Some(parent_id)) {
                let mut metadata = HashMap::new();
                metadata.insert("isLocalVariable".to_string(), serde_json::Value::Bool(true));
                metadata.insert("isDeclaredVariable".to_string(), serde_json::Value::Bool(true));

                let options = SymbolOptions {
                    signature: Some(format!("DECLARE {} {}", variable_name, variable_type)),
                    visibility: Some(crate::extractors::base::Visibility::Private),
                    parent_id: Some(parent_id.to_string()),
                    doc_comment: None,
                    metadata: Some(metadata),
                };

                let variable_symbol = self.base.create_symbol(&function_node, variable_name.to_string(), SymbolKind::Variable, options);
                symbols.push(variable_symbol);
            }
        }
    }

    fn extract_relationships_internal(&mut self, node: tree_sitter::Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        // Port Miller's relationship extraction logic
        match node.kind() {
            "constraint" => {
                // Check if this is a foreign key constraint
                let has_foreign = self.base.find_child_by_type(&node, "keyword_foreign");
                if has_foreign.is_some() {
                    self.extract_foreign_key_relationship(node, symbols, relationships);
                }
            }
            "foreign_key_constraint" | "references_clause" => {
                self.extract_foreign_key_relationship(node, symbols, relationships);
            }
            "select_statement" | "from_clause" => {
                self.extract_table_references(node, symbols, relationships);
            }
            "join" | "join_clause" => {
                self.extract_join_relationships(node, symbols, relationships);
            }
            _ => {}
        }

        // Recursively visit children
        for child in node.children(&mut node.walk()) {
            self.extract_relationships_internal(child, symbols, relationships);
        }
    }

    fn extract_foreign_key_relationship(&mut self, node: tree_sitter::Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        // Port Miller's extractForeignKeyRelationship logic
        // Extract foreign key relationships between tables
        // Look for object_reference after keyword_references
        let references_keyword = self.base.find_child_by_type(&node, "keyword_references");
        if references_keyword.is_none() {
            return;
        }

        let object_ref_node = self.base.find_child_by_type(&node, "object_reference");
        let referenced_table_node = if let Some(obj_ref) = object_ref_node {
            self.base.find_child_by_type(&obj_ref, "identifier")
        } else {
            self.base.find_child_by_type(&node, "table_name")
                .or_else(|| self.base.find_child_by_type(&node, "identifier"))
        };

        let referenced_table_node = match referenced_table_node {
            Some(node) => node,
            None => return,
        };

        let referenced_table = self.base.get_node_text(&referenced_table_node);

        // Find the source table (parent of this foreign key)
        let mut current_node = node.parent();
        while let Some(current) = current_node {
            if current.kind() == "create_table" {
                break;
            }
            current_node = current.parent();
        }

        let current_node = match current_node {
            Some(node) => node,
            None => return,
        };

        // Look for table name in object_reference (same pattern as extractTableDefinition)
        let source_object_ref_node = self.base.find_child_by_type(&current_node, "object_reference");
        let source_table_node = if let Some(obj_ref) = source_object_ref_node {
            self.base.find_child_by_type(&obj_ref, "identifier")
        } else {
            self.base.find_child_by_type(&current_node, "identifier")
                .or_else(|| self.base.find_child_by_type(&current_node, "table_name"))
        };

        let source_table_node = match source_table_node {
            Some(node) => node,
            None => return,
        };

        let source_table = self.base.get_node_text(&source_table_node);

        // Find corresponding symbols
        let source_symbol = symbols.iter().find(|s| s.name == source_table && s.kind == SymbolKind::Class);
        let target_symbol = symbols.iter().find(|s| s.name == referenced_table && s.kind == SymbolKind::Class);

        // Create relationship if we have at least the source symbol
        // Target symbol might not exist if referencing external table
        if let Some(source_symbol) = source_symbol {
            let mut metadata = HashMap::new();
            metadata.insert("targetTable".to_string(), Value::String(referenced_table.clone()));
            metadata.insert("sourceTable".to_string(), Value::String(source_table));
            metadata.insert("relationshipType".to_string(), Value::String("foreign_key".to_string()));
            metadata.insert("isExternal".to_string(), Value::Bool(target_symbol.is_none()));

            relationships.push(Relationship {
                from_symbol_id: source_symbol.id.clone(),
                to_symbol_id: target_symbol.map(|s| s.id.clone()).unwrap_or_else(|| format!("external_{}", referenced_table)),
                kind: RelationshipKind::References, // Foreign key reference
                file_path: self.base.file_path.clone(),
                line_number: node.start_position().row as u32,
                confidence: if target_symbol.is_some() { 1.0 } else { 0.8 }, // Lower confidence for external references
                metadata: Some(metadata),
            });
        }
    }

    fn extract_table_references(&mut self, node: tree_sitter::Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        // Port Miller's extractTableReferences logic
        // Extract table references in SELECT statements for query analysis
        self.base.traverse_tree(&node, &mut |child_node| {
            if child_node.kind() == "table_name" ||
               (child_node.kind() == "identifier" &&
                child_node.parent().map_or(false, |p| p.kind() == "from_clause")) {

                let table_name = self.base.get_node_text(&child_node);
                let _table_symbol = symbols.iter().find(|s| s.name == table_name && s.kind == SymbolKind::Class);

                // This represents a query dependency - the query uses this table
                // We could create a relationship to track which queries use which tables
                // For now, we're just identifying the table usage
            }
        });
    }

    fn extract_join_relationships(&mut self, node: tree_sitter::Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        // Port Miller's extractJoinRelationships logic
        // Extract JOIN relationships from SQL queries
        self.base.traverse_tree(&node, &mut |child_node| {
            if child_node.kind() == "table_name" ||
               (child_node.kind() == "identifier" &&
                child_node.parent().map_or(false, |p| p.kind() == "object_reference")) {

                let table_name = self.base.get_node_text(&child_node);
                let table_symbol = symbols.iter().find(|s| s.name == table_name && s.kind == SymbolKind::Class);

                if let Some(table_symbol) = table_symbol {
                    // Create a join relationship
                    let mut metadata = HashMap::new();
                    metadata.insert("joinType".to_string(), Value::String("join".to_string()));
                    metadata.insert("tableName".to_string(), Value::String(table_name.clone()));

                    relationships.push(Relationship {
                        from_symbol_id: table_symbol.id.clone(),
                        to_symbol_id: table_symbol.id.clone(), // Self-reference for joins
                        kind: RelationshipKind::Joins,
                        file_path: self.base.file_path.clone(),
                        line_number: node.start_position().row as u32,
                        confidence: 0.9,
                        metadata: Some(metadata),
                    });
                }
            }
        });
    }
}