//! Stored procedure and function extraction.
//!
//! Handles extraction of CREATE PROCEDURE and CREATE FUNCTION statements,
//! including parameter extraction and function signatures.

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions};
use crate::extractors::sql::helpers::{DECLARE_VAR_RE, VAR_DECL_RE};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract stored procedure or function from CREATE PROCEDURE/FUNCTION statement
pub(super) fn extract_stored_procedure(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Port extractStoredProcedure logic for regular nodes (not just ERROR)
    // Look for function/procedure name - it may be inside an object_reference
    let object_ref_node = base.find_child_by_type(&node, "object_reference");
    let name_node = if let Some(obj_ref) = object_ref_node {
        base.find_child_by_type(&obj_ref, "identifier")
    } else {
        base.find_child_by_type(&node, "identifier")
            .or_else(|| base.find_child_by_type(&node, "procedure_name"))
            .or_else(|| base.find_child_by_type(&node, "function_name"))
    }?;

    let name = base.get_node_text(&name_node);
    let is_function = node.kind().contains("function");

    let signature = extract_procedure_signature(base, &node);

    let mut metadata = HashMap::new();
    metadata.insert("isFunction".to_string(), Value::Bool(is_function));
    metadata.insert("isStoredProcedure".to_string(), Value::Bool(true));

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(crate::extractors::base::Visibility::Public),
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment: base.find_doc_comment(&node),
        metadata: Some(metadata),
    };

    let symbol_kind = if is_function {
        SymbolKind::Function
    } else {
        SymbolKind::Method
    };
    Some(base.create_symbol(&node, name, symbol_kind, options))
}

/// Extract procedure/function signature with parameters
pub(super) fn extract_procedure_signature(base: &BaseExtractor, node: &Node) -> String {
    // Extract function/procedure name from object_reference if present
    let object_ref_node = base.find_child_by_type(node, "object_reference");
    let name_node = if let Some(obj_ref) = object_ref_node {
        base.find_child_by_type(&obj_ref, "identifier")
    } else {
        base.find_child_by_type(node, "identifier")
            .or_else(|| base.find_child_by_type(node, "procedure_name"))
            .or_else(|| base.find_child_by_type(node, "function_name"))
    };
    let name = if let Some(name_node) = name_node {
        base.get_node_text(&name_node)
    } else {
        "unknown".to_string()
    };

    // Extract parameter list
    let mut params: Vec<String> = Vec::new();
    base.traverse_tree(node, &mut |child_node| {
        if child_node.kind() == "parameter_declaration" || child_node.kind() == "parameter" {
            let param_name_node = base
                .find_child_by_type(child_node, "identifier")
                .or_else(|| base.find_child_by_type(child_node, "parameter_name"));
            let type_node = base
                .find_child_by_type(child_node, "data_type")
                .or_else(|| base.find_child_by_type(child_node, "type_name"));

            if let Some(param_name_node) = param_name_node {
                let param_name = base.get_node_text(&param_name_node);
                let param_type = if let Some(type_node) = type_node {
                    base.get_node_text(&type_node)
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
        let decimal_nodes = base.find_nodes_by_type(node, "decimal");
        if !decimal_nodes.is_empty() {
            let decimal_text = base.get_node_text(&decimal_nodes[0]);
            return_clause = format!(" RETURNS {}", decimal_text);
        } else {
            // Look for other return types as direct children
            let return_type_nodes = [
                "keyword_boolean",
                "keyword_bigint",
                "keyword_int",
                "keyword_varchar",
                "keyword_text",
                "keyword_jsonb",
            ];
            for type_str in &return_type_nodes {
                if let Some(type_node) = base.find_child_by_type(node, type_str) {
                    let type_text = base
                        .get_node_text(&type_node)
                        .replace("keyword_", "")
                        .to_uppercase();
                    return_clause = format!(" RETURNS {}", type_text);
                    break;
                }
            }
        }

        // Look for LANGUAGE clause (PostgreSQL functions)
        if let Some(language_node) = base.find_child_by_type(node, "function_language") {
            let language_text = base.get_node_text(&language_node);
            language_clause = format!(" {}", language_text);
        }
    }

    format!(
        "{} {}({}){}{}",
        keyword,
        name,
        params.join(", "),
        return_clause,
        language_clause
    )
}

/// Extract declared variables from function/procedure body
pub(super) fn extract_declare_variables(
    base: &mut BaseExtractor,
    function_node: Node,
    symbols: &mut Vec<Symbol>,
    parent_id: &str,
) {
    // Port extractDeclareVariables logic
    let function_text = base.get_node_text(&function_node);

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
            let declaration_raw = base.get_node_text(&node);
            let declaration_text = declaration_raw.trim();
            // Match patterns like "v_current_prefs JSONB;" or "v_score DECIMAL(10,2) DEFAULT 0.0;"
            if let Some(captures) = VAR_DECL_RE.captures(declaration_text) {
                let variable_name = captures.get(1).map_or("", |m| m.as_str());
                let variable_type_full = captures.get(2).map_or("", |m| m.as_str());
                let variable_type = variable_type_full
                    .split_whitespace()
                    .next()
                    .unwrap_or("unknown"); // Get first word as type

                // Skip if variable name is empty
                if variable_name.is_empty() {
                    continue;
                }

                let mut metadata = HashMap::new();
                metadata.insert("isLocalVariable".to_string(), serde_json::Value::Bool(true));
                metadata.insert(
                    "isDeclaredVariable".to_string(),
                    serde_json::Value::Bool(true),
                );

                let options = SymbolOptions {
                    signature: Some(format!("DECLARE {} {}", variable_name, variable_type)),
                    visibility: Some(crate::extractors::base::Visibility::Private),
                    parent_id: Some(parent_id.to_string()),
                    doc_comment: None,
                    metadata: Some(metadata),
                };

                let variable_symbol = base.create_symbol(
                    &node,
                    variable_name.to_string(),
                    SymbolKind::Variable,
                    options,
                );
                symbols.push(variable_symbol);
            }
        }
        // MySQL style: keyword_declare followed by identifier and type
        else if node.kind() == "keyword_declare" {
            // For MySQL DECLARE statements, look for the pattern in the surrounding text
            if let Some(parent) = node.parent() {
                let parent_text = base.get_node_text(&parent);

                // Look for DECLARE patterns in the parent text
                for captures in DECLARE_VAR_RE.captures_iter(&parent_text) {
                    let variable_name = captures.get(1).map_or("", |m| m.as_str());
                    let variable_type = captures.get(2).map_or("", |m| m.as_str());

                    // Skip if variable name or type is empty
                    if variable_name.is_empty() || variable_type.is_empty() {
                        continue;
                    }

                    let mut metadata = HashMap::new();
                    metadata.insert("isLocalVariable".to_string(), serde_json::Value::Bool(true));
                    metadata.insert(
                        "isDeclaredVariable".to_string(),
                        serde_json::Value::Bool(true),
                    );

                    let options = SymbolOptions {
                        signature: Some(format!("DECLARE {} {}", variable_name, variable_type)),
                        visibility: Some(crate::extractors::base::Visibility::Private),
                        parent_id: Some(parent_id.to_string()),
                        doc_comment: None,
                        metadata: Some(metadata),
                    };

                    let variable_symbol = base.create_symbol(
                        &node,
                        variable_name.to_string(),
                        SymbolKind::Variable,
                        options,
                    );
                    symbols.push(variable_symbol);
                }
            }
        }
    }

    // Also extract DECLARE variables directly from function text using regex
    let declare_regex = regex::Regex::new(r"DECLARE\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+(DECIMAL\([^)]+\)|JSONB|INT|BIGINT|VARCHAR\([^)]+\)|TEXT|BOOLEAN)").unwrap();
    for captures in declare_regex.captures_iter(&function_text) {
        let variable_name = captures.get(1).map_or("", |m| m.as_str());
        let variable_type = captures.get(2).map_or("", |m| m.as_str());

        // Skip if variable name or type is empty
        if variable_name.is_empty() || variable_type.is_empty() {
            continue;
        }

        // Only add if not already added from tree traversal
        if !symbols
            .iter()
            .any(|s| s.name == variable_name && s.parent_id.as_deref() == Some(parent_id))
        {
            let mut metadata = HashMap::new();
            metadata.insert("isLocalVariable".to_string(), serde_json::Value::Bool(true));
            metadata.insert(
                "isDeclaredVariable".to_string(),
                serde_json::Value::Bool(true),
            );

            let options = SymbolOptions {
                signature: Some(format!("DECLARE {} {}", variable_name, variable_type)),
                visibility: Some(crate::extractors::base::Visibility::Private),
                parent_id: Some(parent_id.to_string()),
                doc_comment: None,
                metadata: Some(metadata),
            };

            let variable_symbol = base.create_symbol(
                &function_node,
                variable_name.to_string(),
                SymbolKind::Variable,
                options,
            );
            symbols.push(variable_symbol);
        }
    }
}

/// Extract parameters from ERROR nodes (for procedures/functions with parse errors)
pub(super) fn extract_parameters_from_error_node(
    base: &mut BaseExtractor,
    node: Node,
    symbols: &mut Vec<Symbol>,
    parent_id: &str,
) {
    // Port extractParametersFromErrorNode logic
    let error_text = base.get_node_text(&node);

    // Extract parameters from procedure/function definitions
    // Look for patterns like "IN p_user_id BIGINT", "OUT p_total_events INT"
    let param_regex = regex::Regex::new(r"(IN|OUT|INOUT)?\s*([a-zA-Z_][a-zA-Z0-9_]*)\s+(BIGINT|INT|VARCHAR|DECIMAL|DATE|BOOLEAN|TEXT|JSONB)").unwrap();

    for captures in param_regex.captures_iter(&error_text) {
        let direction = captures.get(1).map(|m| m.as_str()).unwrap_or("IN"); // Default to IN if not specified
        let param_name = captures.get(2).map_or("", |m| m.as_str());
        let param_type = captures.get(3).map_or("", |m| m.as_str());

        // Skip if param name or type is empty
        if param_name.is_empty() || param_type.is_empty() {
            continue;
        }

        // Don't extract procedure/function names as parameters
        if !error_text.contains(&format!("PROCEDURE {}", param_name))
            && !error_text.contains(&format!("FUNCTION {}", param_name))
        {
            let signature = format!("{} {} {}", direction, param_name, param_type);

            let mut metadata = HashMap::new();
            metadata.insert("isParameter".to_string(), serde_json::Value::Bool(true));
            metadata.insert(
                "extractedFromError".to_string(),
                serde_json::Value::Bool(true),
            );

            let options = SymbolOptions {
                signature: Some(signature),
                visibility: Some(crate::extractors::base::Visibility::Public),
                parent_id: Some(parent_id.to_string()),
                doc_comment: None,
                metadata: Some(metadata),
            };

            let param_symbol =
                base.create_symbol(&node, param_name.to_string(), SymbolKind::Variable, options);
            symbols.push(param_symbol);
        }
    }
}
