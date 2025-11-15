//! Column and table constraint extraction.
//!
//! Handles extraction of constraint definitions within tables:
//! - Column constraints (PRIMARY KEY, UNIQUE, NOT NULL, etc.)
//! - Table-level constraints (foreign keys, checks, etc.)
//! - ALTER TABLE ADD CONSTRAINT statements

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract column constraints (PRIMARY KEY, NOT NULL, UNIQUE, etc.)
pub(super) fn extract_column_constraints(base: &BaseExtractor, column_node: &Node) -> String {
    // Port exact column constraints extraction logic
    let mut constraints: Vec<String> = Vec::new();
    let mut has_primary = false;
    let mut has_key = false;

    base.traverse_tree(column_node, &mut |node| {
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
                // Check if followed by keyword_null (reference logic)
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
                // Find the default value (reference logic)
                if let Some(next_sibling) = node.next_sibling() {
                    let default_value = base.get_node_text(&next_sibling);
                    constraints.push(format!("DEFAULT {}", default_value));
                }
            }
            _ => {}
        }
    });

    // Add PRIMARY KEY if both keywords found (reference logic)
    if has_primary && has_key {
        constraints.push("PRIMARY KEY".to_string());
    }

    // Return formatted string standard format
    if constraints.is_empty() {
        String::new()
    } else {
        format!(" {}", constraints.join(" "))
    }
}

/// Extract table columns from CREATE TABLE statement
pub(super) fn extract_table_columns(
    base: &mut BaseExtractor,
    table_node: Node,
    symbols: &mut Vec<Symbol>,
    parent_table_id: &str,
) {
    // Use find_nodes_by_type to avoid borrowing conflicts
    let column_nodes = base.find_nodes_by_type(&table_node, "column_definition");

    for node in column_nodes {
        // Find column name from identifier or column_name nodes
        let column_name_node = base
            .find_child_by_type(&node, "identifier")
            .or_else(|| base.find_child_by_type(&node, "column_name"));

        if let Some(name_node) = column_name_node {
            let column_name = base.get_node_text(&name_node);

            // Find SQL data type nodes (port comprehensive type search)
            let data_type_node = base
                .find_child_by_type(&node, "data_type")
                .or_else(|| base.find_child_by_type(&node, "type_name"))
                .or_else(|| base.find_child_by_type(&node, "bigint"))
                .or_else(|| base.find_child_by_type(&node, "varchar"))
                .or_else(|| base.find_child_by_type(&node, "int"))
                .or_else(|| base.find_child_by_type(&node, "text"))
                .or_else(|| base.find_child_by_type(&node, "char"))
                .or_else(|| base.find_child_by_type(&node, "decimal"))
                .or_else(|| base.find_child_by_type(&node, "boolean"))
                .or_else(|| base.find_child_by_type(&node, "keyword_boolean"))
                .or_else(|| base.find_child_by_type(&node, "keyword_bigint"))
                .or_else(|| base.find_child_by_type(&node, "keyword_varchar"))
                .or_else(|| base.find_child_by_type(&node, "keyword_int"))
                .or_else(|| base.find_child_by_type(&node, "keyword_text"))
                .or_else(|| base.find_child_by_type(&node, "keyword_json"))
                .or_else(|| base.find_child_by_type(&node, "json"))
                .or_else(|| base.find_child_by_type(&node, "keyword_jsonb"))
                .or_else(|| base.find_child_by_type(&node, "jsonb"))
                .or_else(|| base.find_child_by_type(&node, "date"))
                .or_else(|| base.find_child_by_type(&node, "timestamp"));

            let data_type = if let Some(type_node) = data_type_node {
                base.get_node_text(&type_node)
            } else {
                "unknown".to_string()
            };

            // Extract column constraints and build signature standard format
            let constraints = extract_column_constraints(base, &node);
            let signature = format!("{}{}", data_type, constraints);

            let options = SymbolOptions {
                signature: Some(signature),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: Some(parent_table_id.to_string()),
                doc_comment: None,
                metadata: None,
            };

            // Columns are fields within the table (strategy)
            let column_symbol = base.create_symbol(&node, column_name, SymbolKind::Field, options);
            symbols.push(column_symbol);
        }
    }
}

/// Extract table constraints from CREATE TABLE statement
pub(super) fn extract_table_constraints(
    base: &mut BaseExtractor,
    table_node: Node,
    symbols: &mut Vec<Symbol>,
    parent_table_id: &str,
) {
    // Use find_nodes_by_type to avoid borrowing conflicts and node lifetime issues
    let constraint_nodes = base.find_nodes_by_type(&table_node, "constraint");

    for node in constraint_nodes {
        let mut constraint_type = "unknown";
        let mut constraint_name = format!("constraint_{}", node.start_position().row);

        // Determine constraint type based on child nodes (reference logic)
        let has_check = base.find_child_by_type(&node, "keyword_check").is_some();
        let has_primary = base.find_child_by_type(&node, "keyword_primary").is_some();
        let has_foreign = base.find_child_by_type(&node, "keyword_foreign").is_some();
        let has_unique = base.find_child_by_type(&node, "keyword_unique").is_some();
        let has_index = base.find_child_by_type(&node, "keyword_index").is_some();
        let named_constraint = base.find_child_by_type(&node, "identifier");

        if let Some(name_node) = named_constraint {
            constraint_name = base.get_node_text(&name_node);
        }

        // Determine constraint type (reference logic)
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

        // Create constraint symbol standard format
        let constraint_symbol = create_constraint_symbol(
            base,
            &node,
            constraint_type,
            parent_table_id,
            &constraint_name,
        );
        symbols.push(constraint_symbol);
    }
}

/// Create a constraint symbol
fn create_constraint_symbol(
    base: &mut BaseExtractor,
    node: &Node,
    constraint_type: &str,
    parent_table_id: &str,
    constraint_name: &str,
) -> Symbol {
    // Port createConstraintSymbol logic
    let signature = if constraint_type == "index" {
        format!("INDEX {}", constraint_name)
    } else {
        format!("CONSTRAINT {}", constraint_type.to_uppercase())
    };

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(crate::extractors::base::Visibility::Public),
        parent_id: Some(parent_table_id.to_string()),
        doc_comment: None,
        metadata: None,
    };

    // Constraints as Interface symbols (strategy)
    base.create_symbol(
        node,
        constraint_name.to_string(),
        SymbolKind::Interface,
        options,
    )
}

/// Extract constraints from ALTER TABLE statements
pub(super) fn extract_constraints_from_alter_table(
    base: &mut BaseExtractor,
    node: Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) {
    // Port extractConstraintsFromAlterTable logic
    let node_text = base.get_node_text(&node);

    // Extract ADD CONSTRAINT statements
    let constraint_regex = regex::Regex::new(
        r"ADD\s+CONSTRAINT\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+(CHECK|FOREIGN\s+KEY|UNIQUE|PRIMARY\s+KEY)",
    )
    .unwrap();
    if let Some(captures) = constraint_regex.captures(&node_text) {
        if let Some(constraint_name) = captures.get(1) {
            let name = constraint_name.as_str().to_string();
            let constraint_type = captures.get(2).map_or("", |m| m.as_str()).to_uppercase();

            // Skip if constraint type is empty
            if constraint_type.is_empty() {
                return;
            }

            let mut signature = format!("ALTER TABLE ADD CONSTRAINT {} {}", name, constraint_type);

            // Add more details based on constraint type
            if constraint_type == "CHECK" {
                let check_regex =
                    regex::Regex::new(r"CHECK\s*\(([^)]+(?:\([^)]*\)[^)]*)*)").unwrap();
                if let Some(check_captures) = check_regex.captures(&node_text) {
                    let check_condition = check_captures.get(1).map_or("", |m| m.as_str()).trim();
                    if !check_condition.is_empty() {
                        signature.push_str(&format!(" ({})", check_condition));
                    }
                }
            } else if constraint_type.contains("FOREIGN") {
                let fk_regex = regex::Regex::new(
                    r"FOREIGN\s+KEY\s*\(([^)]+)\)\s*REFERENCES\s+([a-zA-Z_][a-zA-Z0-9_]*)",
                )
                .unwrap();
                if let Some(fk_captures) = fk_regex.captures(&node_text) {
                    let fk_columns = fk_captures.get(1).map_or("", |m| m.as_str());
                    let fk_ref_table = fk_captures.get(2).map_or("", |m| m.as_str());

                    if !fk_columns.is_empty() && !fk_ref_table.is_empty() {
                        signature
                            .push_str(&format!(" ({}) REFERENCES {}", fk_columns, fk_ref_table));
                    }
                }

                // Add ON DELETE/UPDATE actions
                let on_delete_regex =
                    regex::Regex::new(r"ON\s+DELETE\s+(CASCADE|RESTRICT|SET\s+NULL|NO\s+ACTION)")
                        .unwrap();
                if let Some(on_delete_captures) = on_delete_regex.captures(&node_text) {
                    let on_delete_action = on_delete_captures
                        .get(1)
                        .map_or("", |m| m.as_str())
                        .to_uppercase();
                    if !on_delete_action.is_empty() {
                        signature.push_str(&format!(" ON DELETE {}", on_delete_action));
                    }
                }

                let on_update_regex =
                    regex::Regex::new(r"ON\s+UPDATE\s+(CASCADE|RESTRICT|SET\s+NULL|NO\s+ACTION)")
                        .unwrap();
                if let Some(on_update_captures) = on_update_regex.captures(&node_text) {
                    let on_update_action = on_update_captures
                        .get(1)
                        .map_or("", |m| m.as_str())
                        .to_uppercase();
                    if !on_update_action.is_empty() {
                        signature.push_str(&format!(" ON UPDATE {}", on_update_action));
                    }
                }
            }

            let mut metadata = HashMap::new();
            metadata.insert("isConstraint".to_string(), Value::Bool(true));
            metadata.insert(
                "constraintType".to_string(),
                Value::String(constraint_type.clone()),
            );

            let options = SymbolOptions {
                signature: Some(signature),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                doc_comment: None,
                metadata: Some(metadata),
            };

            let constraint_symbol = base.create_symbol(&node, name, SymbolKind::Property, options);
            symbols.push(constraint_symbol);
        }
    }
}
