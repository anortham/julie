// Dart Extractor - Import and Export Directives
//
// Extracts import/export statements from Dart source files.
// The harper-tree-sitter-dart grammar parses these as:
//   import_or_export
//     library_import -> import_specification -> configurable_uri -> uri -> string_literal
//     library_export -> configurable_uri -> uri -> string_literal
//
// Note: This grammar does NOT correctly parse `as`, `show`, `hide`, `library`,
// or `part` directives — they produce ERROR nodes. We extract what the parser
// gives us (basic import/export with URI).

use super::helpers::{find_child_by_type, get_node_text};
use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract an import or export directive from an `import_or_export` node.
///
/// Returns a symbol with:
///   - name: the URI string with quotes stripped (e.g., "dart:async", "package:flutter/material.dart")
///   - kind: Import or Export depending on the child node type
///   - signature: the full directive text (e.g., "import 'dart:async';")
pub(super) fn extract_import_or_export(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Determine whether this is an import or export by checking child node kinds
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "library_import" => return extract_library_import(base, &child, parent_id),
            "library_export" => return extract_library_export(base, &child, parent_id),
            _ => {}
        }
    }
    None
}

/// Extract from a `library_import` node.
fn extract_library_import(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let uri = extract_uri_from_subtree(node)?;
    let full_text = get_node_text(node);

    Some(base.create_symbol(
        node,
        uri,
        SymbolKind::Import,
        SymbolOptions {
            signature: Some(full_text),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([(
                "type".to_string(),
                serde_json::Value::String("import".to_string()),
            )])),
            doc_comment: None,
        },
    ))
}

/// Extract from a `library_export` node.
fn extract_library_export(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let uri = extract_uri_from_subtree(node)?;
    let full_text = get_node_text(node);

    Some(base.create_symbol(
        node,
        uri,
        SymbolKind::Export,
        SymbolOptions {
            signature: Some(full_text),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([(
                "type".to_string(),
                serde_json::Value::String("export".to_string()),
            )])),
            doc_comment: None,
        },
    ))
}

/// Walk the subtree to find the URI string, stripping quotes.
///
/// The tree structure is:
///   configurable_uri -> uri -> string_literal
/// The string_literal text includes quotes (e.g., `'dart:async'`), so we strip them.
fn extract_uri_from_subtree(node: &Node) -> Option<String> {
    let configurable_uri = find_child_by_type(node, "configurable_uri")
        .or_else(|| {
            // For import_specification, look one level deeper
            let mut cursor = node.walk();
            node.children(&mut cursor).find_map(|child| {
                find_child_by_type(&child, "configurable_uri")
            })
        })?;

    let uri_node = find_child_by_type(&configurable_uri, "uri")?;
    let string_literal = find_child_by_type(&uri_node, "string_literal")?;
    let raw_text = get_node_text(&string_literal);

    // Strip surrounding quotes (single or double)
    let stripped = raw_text
        .trim_start_matches('\'')
        .trim_start_matches('"')
        .trim_end_matches('\'')
        .trim_end_matches('"');

    if stripped.is_empty() {
        return None;
    }

    Some(stripped.to_string())
}
