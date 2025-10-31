//! Schema object extraction (tables, views, indexes, triggers, domains, types).
//!
//! Handles extraction of CREATE statements for schema-level database objects:
//! - Tables (with column extraction delegated to constraints module)
//! - Views
//! - Indexes
//! - Triggers
//! - Schemas
//! - Domains
//! - Types (including ENUMs)

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions};
use crate::extractors::sql::helpers::{CREATE_VIEW_RE, INCLUDE_CLAUSE_RE, INDEX_COLUMN_RE};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract table definition from CREATE TABLE statement
pub(super) fn extract_table_definition(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Port exact logic: Look for table name inside object_reference node
    let object_ref_node = base.find_child_by_type(&node, "object_reference");
    let table_name_node = if let Some(obj_ref) = object_ref_node {
        base.find_child_by_type(&obj_ref, "identifier")
    } else {
        base.find_child_by_type(&node, "identifier")
            .or_else(|| base.find_child_by_type(&node, "table_name"))
    };

    let table_name_node = table_name_node?;
    let table_name = base.get_node_text(&table_name_node);

    let signature = extract_table_signature(base, node);

    let mut metadata = HashMap::new();
    metadata.insert("isTable".to_string(), Value::Bool(true));

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(crate::extractors::base::Visibility::Public),
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment: base.find_doc_comment(&node),
        metadata: Some(metadata),
    };

    Some(base.create_symbol(&node, table_name, SymbolKind::Class, options))
}

/// Extract table signature showing column count
pub(super) fn extract_table_signature(base: &mut BaseExtractor, node: Node) -> String {
    // Look for table name inside object_reference node (same as extractTableDefinition)
    let object_ref_node = base.find_child_by_type(&node, "object_reference");
    let name_node = if let Some(obj_ref) = object_ref_node {
        base.find_child_by_type(&obj_ref, "identifier")
    } else {
        base.find_child_by_type(&node, "identifier")
            .or_else(|| base.find_child_by_type(&node, "table_name"))
    };

    let table_name = if let Some(name_node) = name_node {
        base.get_node_text(&name_node)
    } else {
        "unknown".to_string()
    };

    // Count columns for a brief signature
    let mut column_count = 0;
    base.traverse_tree(&node, &mut |child_node| {
        if child_node.kind() == "column_definition" {
            column_count += 1;
        }
    });

    format!("CREATE TABLE {} ({} columns)", table_name, column_count)
}

/// Extract view from CREATE VIEW statement
pub(super) fn extract_view(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Implementation of view extraction from error nodes
    let node_text = base.get_node_text(&node);

    // Extract views from ERROR nodes
    if let Some(captures) = CREATE_VIEW_RE.captures(&node_text) {
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
                doc_comment: base.find_doc_comment(&node),
                metadata: Some(metadata),
            };

            return Some(base.create_symbol(&node, name, SymbolKind::Interface, options));
        }
    }

    None
}

/// Extract index from CREATE INDEX statement
pub(super) fn extract_index(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Port extractIndex logic
    let name_node = base
        .find_child_by_type(&node, "identifier")
        .or_else(|| base.find_child_by_type(&node, "index_name"))?;

    let name = base.get_node_text(&name_node);

    // Get the full index text for signature
    let node_text = base.get_node_text(&node);
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
        let table_name = on_captures.get(1).map_or("", |m| m.as_str());
        if !table_name.is_empty() {
            signature.push_str(&format!(" ON {}", table_name));
        }
    }

    // Add USING clause if present (before columns)
    let using_regex = regex::Regex::new(r"USING\s+([A-Z]+)").unwrap();
    if let Some(using_captures) = using_regex.captures(&node_text) {
        let using_method = using_captures.get(1).map_or("", |m| m.as_str());
        if !using_method.is_empty() {
            signature.push_str(&format!(" USING {}", using_method));
        }
    }

    // Add column information if found
    if let Some(column_captures) = INDEX_COLUMN_RE.captures(&node_text) {
        let columns = column_captures.get(1).map_or("", |m| m.as_str());
        if !columns.is_empty() {
            signature.push_str(&format!(" {}", columns));
        }
    }

    // Add INCLUDE clause if present
    if let Some(include_captures) = INCLUDE_CLAUSE_RE.captures(&node_text) {
        let include_clause = include_captures.get(1).map_or("", |m| m.as_str());
        if !include_clause.is_empty() {
            signature.push_str(&format!(" INCLUDE {}", include_clause));
        }
    }

    // Add WHERE clause if present
    let where_regex = regex::Regex::new(r"WHERE\s+(.+?)(?:;|$)").unwrap();
    if let Some(where_captures) = where_regex.captures(&node_text) {
        let where_condition = where_captures.get(1).map_or("", |m| m.as_str()).trim();
        if !where_condition.is_empty() {
            signature.push_str(&format!(" WHERE {}", where_condition));
        }
    }

    let mut metadata = HashMap::new();
    metadata.insert("isIndex".to_string(), serde_json::Value::Bool(true));
    metadata.insert("isUnique".to_string(), serde_json::Value::Bool(is_unique));

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(crate::extractors::base::Visibility::Public),
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment: base.find_doc_comment(&node),
        metadata: Some(metadata),
    };

    Some(base.create_symbol(&node, name, SymbolKind::Property, options))
}

/// Extract trigger from CREATE TRIGGER statement
pub(super) fn extract_trigger(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Port extractTrigger logic
    let name_node = base
        .find_child_by_type(&node, "identifier")
        .or_else(|| base.find_child_by_type(&node, "trigger_name"))?;

    let name = base.get_node_text(&name_node);

    let mut metadata = HashMap::new();
    metadata.insert("isTrigger".to_string(), Value::Bool(true));

    let options = SymbolOptions {
        signature: Some(format!("TRIGGER {}", name)),
        visibility: Some(crate::extractors::base::Visibility::Public),
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment: base.find_doc_comment(&node),
        metadata: Some(metadata),
    };

    Some(base.create_symbol(&node, name, SymbolKind::Method, options))
}

/// Extract schema from CREATE SCHEMA statement
pub(super) fn extract_schema(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Implementation of schema extraction from error nodes
    let node_text = base.get_node_text(&node);

    // Extract schemas from ERROR nodes
    let schema_regex = regex::Regex::new(r"CREATE\s+SCHEMA\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();

    if let Some(captures) = schema_regex.captures(&node_text) {
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
                doc_comment: base.find_doc_comment(&node),
                metadata: Some(metadata),
            };

            return Some(base.create_symbol(&node, name, SymbolKind::Namespace, options));
        }
    }

    None
}

/// Extract domain from CREATE DOMAIN statement
pub(super) fn extract_domain(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Port extractDomain logic
    // Look for domain name - it may be inside an object_reference
    let object_ref_node = base.find_child_by_type(&node, "object_reference");
    let name_node = if let Some(obj_ref) = object_ref_node {
        base.find_child_by_type(&obj_ref, "identifier")
    } else {
        base.find_child_by_type(&node, "identifier")
            .or_else(|| base.find_child_by_type(&node, "domain_name"))
    }?;

    let name = base.get_node_text(&name_node);

    // Build domain signature with base type and constraints
    let node_text = base.get_node_text(&node);
    let mut signature = format!("CREATE DOMAIN {}", name);

    // Extract the base type (AS datatype)
    if let Some(as_match) = regex::Regex::new(r"AS\s+([A-Za-z]+(?:\(\d+(?:,\s*\d+)?\))?)")
        .unwrap()
        .captures(&node_text)
    {
        signature.push_str(&format!(" AS {}", as_match.get(1).unwrap().as_str()));
    }

    // Add CHECK constraint if present
    if let Some(check_match) = regex::Regex::new(r"CHECK\s*\(([^)]+(?:\([^)]*\)[^)]*)*)\)")
        .unwrap()
        .captures(&node_text)
    {
        signature.push_str(&format!(
            " CHECK ({})",
            check_match.get(1).unwrap().as_str().trim()
        ));
    }

    let mut metadata = HashMap::new();
    metadata.insert("isDomain".to_string(), serde_json::Value::Bool(true));

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(crate::extractors::base::Visibility::Public),
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment: base.find_doc_comment(&node),
        metadata: Some(metadata),
    };

    Some(base.create_symbol(&node, name, SymbolKind::Class, options))
}

/// Extract type from CREATE TYPE statement (including ENUMs)
pub(super) fn extract_type(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Port extractType logic
    // Look for type name in object_reference
    let object_ref_node = base.find_child_by_type(&node, "object_reference");
    let name_node = if let Some(obj_ref) = object_ref_node {
        base.find_child_by_type(&obj_ref, "identifier")
    } else {
        base.find_child_by_type(&node, "identifier")
    }?;

    let name = base.get_node_text(&name_node);

    // Check if this is an ENUM type
    let node_text = base.get_node_text(&node);
    if node_text.contains("AS ENUM") {
        // Extract enum values from enum_elements
        let enum_elements_node = base.find_child_by_type(&node, "enum_elements");
        let enum_values = if let Some(elements) = enum_elements_node {
            base.get_node_text(&elements)
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
            doc_comment: base.find_doc_comment(&node),
            metadata: Some(metadata),
        };

        return Some(base.create_symbol(&node, name, SymbolKind::Class, options));
    }

    // Handle other types (non-enum)
    let signature = format!("CREATE TYPE {}", name);

    let mut metadata = HashMap::new();
    metadata.insert("isType".to_string(), serde_json::Value::Bool(true));

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(crate::extractors::base::Visibility::Public),
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment: base.find_doc_comment(&node),
        metadata: Some(metadata),
    };

    Some(base.create_symbol(&node, name, SymbolKind::Class, options))
}

/// Extract CTE (Common Table Expression) from WITH clause
pub(super) fn extract_cte(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Implementation of extractCte method
    // Extract CTE name from identifier child
    let name_node = base.find_child_by_type(&node, "identifier")?;
    let name = base.get_node_text(&name_node);

    // Check if this is a recursive CTE by looking for RECURSIVE keyword in the parent context
    let mut signature = format!("WITH {} AS (...)", name);
    let parent_node = node.parent();
    if let Some(parent) = parent_node {
        let parent_text = base.get_node_text(&parent);
        if parent_text.contains("RECURSIVE") {
            signature = format!("WITH RECURSIVE {} AS (...)", name);
        }
    }

    let mut metadata = HashMap::new();
    metadata.insert("isCte".to_string(), serde_json::Value::Bool(true));
    metadata.insert("isTemporaryView".to_string(), serde_json::Value::Bool(true));

    let options = SymbolOptions {
        signature: Some(signature),
        visibility: Some(crate::extractors::base::Visibility::Public),
        parent_id: parent_id.map(|s| s.to_string()),
        doc_comment: base.find_doc_comment(&node),
        metadata: Some(metadata),
    };

    Some(base.create_symbol(&node, name, SymbolKind::Interface, options))
}

/// Extract sequence from CREATE SEQUENCE statement
pub(super) fn extract_sequence(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Port extractSequence logic
    // Look for sequence name - it may be inside an object_reference
    let object_ref_node = base.find_child_by_type(&node, "object_reference");
    let name_node = if let Some(obj_ref) = object_ref_node {
        base.find_child_by_type(&obj_ref, "identifier")
    } else {
        base.find_child_by_type(&node, "identifier")
            .or_else(|| base.find_child_by_type(&node, "sequence_name"))
    }?;

    let name = base.get_node_text(&name_node);

    // Build sequence signature with options
    let node_text = base.get_node_text(&node);
    let mut signature = format!("CREATE SEQUENCE {}", name);

    // Add sequence options if present
    let mut options_vec = Vec::new();

    if let Some(start_match) = regex::Regex::new(r"START\s+WITH\s+(\d+)")
        .unwrap()
        .captures(&node_text)
    {
        options_vec.push(format!(
            "START WITH {}",
            start_match.get(1).unwrap().as_str()
        ));
    }

    if let Some(inc_match) = regex::Regex::new(r"INCREMENT\s+BY\s+(\d+)")
        .unwrap()
        .captures(&node_text)
    {
        options_vec.push(format!(
            "INCREMENT BY {}",
            inc_match.get(1).unwrap().as_str()
        ));
    }

    if let Some(min_match) = regex::Regex::new(r"MINVALUE\s+(\d+)")
        .unwrap()
        .captures(&node_text)
    {
        options_vec.push(format!("MINVALUE {}", min_match.get(1).unwrap().as_str()));
    }

    if let Some(max_match) = regex::Regex::new(r"MAXVALUE\s+(\d+)")
        .unwrap()
        .captures(&node_text)
    {
        options_vec.push(format!("MAXVALUE {}", max_match.get(1).unwrap().as_str()));
    }

    if let Some(cache_match) = regex::Regex::new(r"CACHE\s+(\d+)")
        .unwrap()
        .captures(&node_text)
    {
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
        doc_comment: base.find_doc_comment(&node),
        metadata: Some(metadata),
    };

    Some(base.create_symbol(&node, name, SymbolKind::Variable, options))
}
