//! ERROR node processing for tree-sitter parse failures.
//!
//! Handles extraction from tree-sitter ERROR nodes when the parser encounters
//! syntax it doesn't recognize. This is critical for handling diverse SQL dialects.

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions};
use crate::extractors::sql::helpers::CREATE_VIEW_RE;
use crate::extractors::sql::routines;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract multiple symbols from ERROR node
pub(super) fn extract_multiple_from_error_node(
    base: &mut BaseExtractor,
    node: Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) {
    let error_text = base.get_node_text(&node);

    // Extract stored procedures from DELIMITER syntax
    extract_procedures_from_error(&error_text, base, &node, symbols, parent_id);
    extract_functions_from_error(&error_text, base, &node, symbols, parent_id);
    extract_schemas_from_error(&error_text, base, &node, symbols, parent_id);
    extract_views_from_error(&error_text, base, &node, symbols, parent_id);
    extract_triggers_from_error(&error_text, base, &node, symbols, parent_id);
    extract_constraints_from_error(&error_text, base, &node, symbols, parent_id);
    extract_domains_from_error(&error_text, base, &node, symbols, parent_id);
    extract_types_from_error(&error_text, base, &node, symbols, parent_id);
    extract_aggregates_from_error(&error_text, base, &node, symbols, parent_id);
}

/// Extract procedures from ERROR node text
fn extract_procedures_from_error(
    error_text: &str,
    base: &mut BaseExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) {
    let procedure_regex =
        regex::Regex::new(r"CREATE\s+PROCEDURE\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
    if let Some(captures) = procedure_regex.captures(error_text) {
        if let Some(procedure_name) = captures.get(1) {
            let name = procedure_name.as_str().to_string();

            let mut metadata = HashMap::new();
            metadata.insert(
                "isStoredProcedure".to_string(),
                serde_json::Value::Bool(true),
            );
            metadata.insert(
                "extractedFromError".to_string(),
                serde_json::Value::Bool(true),
            );

            let options = SymbolOptions {
                signature: Some(format!("CREATE PROCEDURE {}(...)", name)),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                doc_comment: None,
                metadata: Some(metadata),
            };

            let procedure_symbol =
                base.create_symbol(node, name.clone(), SymbolKind::Function, options);
            symbols.push(procedure_symbol.clone());
            routines::extract_parameters_from_error_node(
                base,
                *node,
                symbols,
                &procedure_symbol.id,
            );
        }
    }
}

/// Extract functions from ERROR node text
fn extract_functions_from_error(
    error_text: &str,
    base: &mut BaseExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) {
    let function_regex = regex::Regex::new(r"CREATE\s+(?:OR\s+REPLACE\s+)?FUNCTION\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\([^)]*\)\s*RETURNS?\s+([A-Z0-9(),\s]+)").unwrap();
    if let Some(captures) = function_regex.captures(error_text) {
        if let Some(function_name) = captures.get(1) {
            let name = function_name.as_str().to_string();
            let return_type = captures
                .get(2)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();

            let mut metadata = HashMap::new();
            metadata.insert("isFunction".to_string(), serde_json::Value::Bool(true));
            metadata.insert(
                "extractedFromError".to_string(),
                serde_json::Value::Bool(true),
            );
            metadata.insert(
                "returnType".to_string(),
                serde_json::Value::String(return_type.clone()),
            );

            let options = SymbolOptions {
                signature: Some(format!(
                    "CREATE FUNCTION {}(...) RETURNS {}",
                    name, return_type
                )),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                doc_comment: None,
                metadata: Some(metadata),
            };

            let function_symbol =
                base.create_symbol(node, name.clone(), SymbolKind::Function, options);
            symbols.push(function_symbol.clone());
            routines::extract_declare_variables(base, *node, symbols, &function_symbol.id);
            return;
        }
    }

    // Fallback: Extract any CREATE FUNCTION
    let simple_function_regex =
        regex::Regex::new(r"CREATE\s+(?:OR\s+REPLACE\s+)?FUNCTION\s+([a-zA-Z_][a-zA-Z0-9_]*)")
            .unwrap();
    if let Some(captures) = simple_function_regex.captures(error_text) {
        if let Some(function_name) = captures.get(1) {
            let name = function_name.as_str().to_string();

            let mut metadata = HashMap::new();
            metadata.insert("isFunction".to_string(), serde_json::Value::Bool(true));
            metadata.insert(
                "extractedFromError".to_string(),
                serde_json::Value::Bool(true),
            );

            let options = SymbolOptions {
                signature: Some(format!("CREATE FUNCTION {}(...)", name)),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                doc_comment: None,
                metadata: Some(metadata),
            };

            let function_symbol =
                base.create_symbol(node, name.clone(), SymbolKind::Function, options);
            symbols.push(function_symbol.clone());
            routines::extract_declare_variables(base, *node, symbols, &function_symbol.id);
        }
    }
}

/// Extract schemas from ERROR node text
fn extract_schemas_from_error(
    error_text: &str,
    base: &mut BaseExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) {
    let schema_regex = regex::Regex::new(r"CREATE\s+SCHEMA\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
    if let Some(captures) = schema_regex.captures(error_text) {
        if let Some(schema_name) = captures.get(1) {
            let name = schema_name.as_str().to_string();

            let mut metadata = HashMap::new();
            metadata.insert("isSchema".to_string(), serde_json::Value::Bool(true));
            metadata.insert(
                "extractedFromError".to_string(),
                serde_json::Value::Bool(true),
            );

            let options = SymbolOptions {
                signature: Some(format!("CREATE SCHEMA {}", name)),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                doc_comment: None,
                metadata: Some(metadata),
            };

            let schema_symbol = base.create_symbol(node, name, SymbolKind::Namespace, options);
            symbols.push(schema_symbol);
        }
    }
}

/// Extract views from ERROR node text
fn extract_views_from_error(
    error_text: &str,
    base: &mut BaseExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) {
    if let Some(captures) = CREATE_VIEW_RE.captures(error_text) {
        if let Some(view_name) = captures.get(1) {
            let name = view_name.as_str().to_string();

            let mut metadata = HashMap::new();
            metadata.insert("isView".to_string(), serde_json::Value::Bool(true));
            metadata.insert(
                "extractedFromError".to_string(),
                serde_json::Value::Bool(true),
            );

            let options = SymbolOptions {
                signature: Some(format!("CREATE VIEW {}", name)),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                doc_comment: None,
                metadata: Some(metadata),
            };

            let view_symbol =
                base.create_symbol(node, name.clone(), SymbolKind::Interface, options);
            symbols.push(view_symbol.clone());
        }
    }
}

/// Extract triggers from ERROR node text
fn extract_triggers_from_error(
    error_text: &str,
    base: &mut BaseExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) {
    let trigger_regex = regex::Regex::new(r"CREATE\s+TRIGGER\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
    if let Some(captures) = trigger_regex.captures(error_text) {
        if let Some(trigger_name) = captures.get(1) {
            let name = trigger_name.as_str().to_string();

            let details_regex = regex::Regex::new(r"CREATE\s+TRIGGER\s+[a-zA-Z_][a-zA-Z0-9_]*\s+(BEFORE|AFTER)\s+(INSERT|UPDATE|DELETE)\s+ON\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();

            let mut signature = format!("CREATE TRIGGER {}", name);
            if let Some(details_captures) = details_regex.captures(error_text) {
                let timing = details_captures.get(1).map_or("", |m| m.as_str());
                let event = details_captures.get(2).map_or("", |m| m.as_str());
                let table = details_captures.get(3).map_or("", |m| m.as_str());

                if !timing.is_empty() && !event.is_empty() && !table.is_empty() {
                    signature =
                        format!("CREATE TRIGGER {} {} {} ON {}", name, timing, event, table);
                }
            }

            let mut metadata = HashMap::new();
            metadata.insert("isTrigger".to_string(), serde_json::Value::Bool(true));
            metadata.insert(
                "extractedFromError".to_string(),
                serde_json::Value::Bool(true),
            );

            let options = SymbolOptions {
                signature: Some(signature),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                doc_comment: None,
                metadata: Some(metadata),
            };

            let trigger_symbol = base.create_symbol(node, name, SymbolKind::Method, options);
            symbols.push(trigger_symbol);
        }
    }
}

/// Extract constraints from ERROR node text (ALTER TABLE ADD CONSTRAINT)
fn extract_constraints_from_error(
    error_text: &str,
    base: &mut BaseExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) {
    let constraint_regex = regex::Regex::new(r"ALTER\s+TABLE\s+[a-zA-Z_][a-zA-Z0-9_]*\s+ADD\s+CONSTRAINT\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+(CHECK|FOREIGN\s+KEY|UNIQUE|PRIMARY\s+KEY)").unwrap();
    if let Some(captures) = constraint_regex.captures(error_text) {
        if let Some(constraint_name) = captures.get(1) {
            let name = constraint_name.as_str().to_string();
            let constraint_type = captures.get(2).map_or("", |m| m.as_str()).to_uppercase();

            // Skip if constraint type is empty
            if constraint_type.is_empty() {
                return;
            }

            let mut signature = format!("ALTER TABLE ADD CONSTRAINT {} {}", name, constraint_type);

            if constraint_type == "CHECK" {
                let check_regex =
                    regex::Regex::new(r"CHECK\s*\(([^)]+(?:\([^)]*\)[^)]*)*)").unwrap();
                if let Some(check_captures) = check_regex.captures(error_text) {
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
                if let Some(fk_captures) = fk_regex.captures(error_text) {
                    let fk_columns = fk_captures.get(1).map_or("", |m| m.as_str());
                    let fk_ref_table = fk_captures.get(2).map_or("", |m| m.as_str());

                    if !fk_columns.is_empty() && !fk_ref_table.is_empty() {
                        signature
                            .push_str(&format!(" ({}) REFERENCES {}", fk_columns, fk_ref_table));
                    }
                }

                let on_delete_regex =
                    regex::Regex::new(r"ON\s+DELETE\s+(CASCADE|RESTRICT|SET\s+NULL|NO\s+ACTION)")
                        .unwrap();
                if let Some(on_delete_captures) = on_delete_regex.captures(error_text) {
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
                if let Some(on_update_captures) = on_update_regex.captures(error_text) {
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
            metadata.insert("isConstraint".to_string(), serde_json::Value::Bool(true));
            metadata.insert(
                "constraintType".to_string(),
                serde_json::Value::String(constraint_type.clone()),
            );
            metadata.insert(
                "extractedFromError".to_string(),
                serde_json::Value::Bool(true),
            );

            let options = SymbolOptions {
                signature: Some(signature),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                doc_comment: None,
                metadata: Some(metadata),
            };

            let constraint_symbol = base.create_symbol(node, name, SymbolKind::Property, options);
            symbols.push(constraint_symbol);
        }
    }
}

/// Extract domains from ERROR node text
fn extract_domains_from_error(
    error_text: &str,
    base: &mut BaseExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) {
    let domain_regex = regex::Regex::new(
        r"CREATE\s+DOMAIN\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+AS\s+([A-Za-z]+(?:\(\d+(?:,\s*\d+)?\))?)",
    )
    .unwrap();
    if let Some(captures) = domain_regex.captures(error_text) {
        if let Some(domain_name) = captures.get(1) {
            let name = domain_name.as_str().to_string();
            let base_type = captures.get(2).map_or("", |m| m.as_str()).to_string();

            // Skip if base type is empty
            if base_type.is_empty() {
                return;
            }

            let mut signature = format!("CREATE DOMAIN {} AS {}", name, base_type);

            let check_regex = regex::Regex::new(r"CHECK\s*\(([^)]+(?:\([^)]*\)[^)]*)*)\)").unwrap();
            if let Some(check_captures) = check_regex.captures(error_text) {
                let check_condition = check_captures.get(1).map_or("", |m| m.as_str()).trim();
                if !check_condition.is_empty() {
                    signature.push_str(&format!(" CHECK ({})", check_condition));
                }
            }

            let mut metadata = HashMap::new();
            metadata.insert("isDomain".to_string(), serde_json::Value::Bool(true));
            metadata.insert(
                "extractedFromError".to_string(),
                serde_json::Value::Bool(true),
            );
            metadata.insert("baseType".to_string(), serde_json::Value::String(base_type));

            let options = SymbolOptions {
                signature: Some(signature),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                doc_comment: None,
                metadata: Some(metadata),
            };

            let domain_symbol = base.create_symbol(node, name, SymbolKind::Class, options);
            symbols.push(domain_symbol);
        }
    }
}

/// Extract enum/custom types from ERROR node text
fn extract_types_from_error(
    error_text: &str,
    base: &mut BaseExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) {
    let enum_regex =
        regex::Regex::new(r"CREATE\s+TYPE\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+AS\s+ENUM\s*\(([\s\S]*?)\)")
            .unwrap();
    if let Some(captures) = enum_regex.captures(error_text) {
        if let Some(enum_name) = captures.get(1) {
            let name = enum_name.as_str().to_string();
            let enum_values = captures.get(2).map_or("", |m| m.as_str());

            // Skip if enum values are empty
            if enum_values.is_empty() {
                return;
            }

            let signature = format!("CREATE TYPE {} AS ENUM ({})", name, enum_values.trim());

            let mut metadata = HashMap::new();
            metadata.insert("isEnum".to_string(), serde_json::Value::Bool(true));
            metadata.insert(
                "extractedFromError".to_string(),
                serde_json::Value::Bool(true),
            );

            let options = SymbolOptions {
                signature: Some(signature),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                doc_comment: None,
                metadata: Some(metadata),
            };

            let enum_symbol = base.create_symbol(node, name, SymbolKind::Class, options);
            symbols.push(enum_symbol);
        }
    }
}

/// Extract aggregate functions from ERROR node text
fn extract_aggregates_from_error(
    error_text: &str,
    base: &mut BaseExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) {
    let aggregate_regex =
        regex::Regex::new(r"CREATE\s+AGGREGATE\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\(([^)]*)\)").unwrap();
    if let Some(captures) = aggregate_regex.captures(error_text) {
        if let Some(aggregate_name) = captures.get(1) {
            let name = aggregate_name.as_str().to_string();
            let parameters = captures.get(2).map_or("", |m| m.as_str());

            let signature = format!("CREATE AGGREGATE {}({})", name, parameters);

            let mut metadata = HashMap::new();
            metadata.insert("isAggregate".to_string(), serde_json::Value::Bool(true));
            metadata.insert(
                "extractedFromError".to_string(),
                serde_json::Value::Bool(true),
            );

            let options = SymbolOptions {
                signature: Some(signature),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                doc_comment: None,
                metadata: Some(metadata),
            };

            let aggregate_symbol = base.create_symbol(node, name, SymbolKind::Function, options);
            symbols.push(aggregate_symbol);
        }
    }
}
