//! PowerShell variable extraction and management
//! Handles variable assignment, scopes, and references

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Node;

use super::documentation::{
    get_variable_documentation, is_automatic_variable, is_environment_variable,
};
use super::helpers::find_variable_name_node;

/// Extract variable symbols from variable assignment and references
pub(super) fn extract_variable(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = find_variable_name_node(node)?;
    let mut name = base.get_node_text(&name_node);

    // Remove $ prefix and scope qualifiers
    name = name
        .replace("$", "")
        .replace("Global:", "")
        .replace("Script:", "")
        .replace("Local:", "")
        .replace("Using:", "");

    // Determine scope and visibility
    let full_text = base.get_node_text(&name_node);
    let is_global = full_text.contains("Global:");
    let is_script = full_text.contains("Script:");
    let is_environment = full_text.contains("env:") || is_environment_variable(&name);
    let is_automatic = is_automatic_variable(&name);

    let signature = extract_variable_signature(base, node);
    let visibility = if is_global {
        Visibility::Public
    } else {
        Visibility::Private
    };
    let doc_comment =
        get_variable_documentation(is_environment, is_automatic, is_global, is_script);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Variable,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: None,
            doc_comment: if doc_comment.is_empty() {
                None
            } else {
                Some(doc_comment)
            },
        },
    ))
}

/// Extract variable reference symbols (automatic and environment variables only)
pub(super) fn extract_variable_reference(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let mut name = base.get_node_text(&node);

    // Remove $ prefix and scope qualifiers
    name = name
        .replace("$", "")
        .replace("Global:", "")
        .replace("Script:", "")
        .replace("Local:", "")
        .replace("Using:", "")
        .replace("env:", "");

    // Only extract automatic variables, environment variables, and special variables
    // to avoid creating symbols for every variable reference
    let is_automatic = is_automatic_variable(&name);
    let is_environment =
        is_environment_variable(&name) || base.get_node_text(&node).contains("env:");

    if !is_automatic && !is_environment {
        return None; // Skip regular variable references
    }

    // Determine scope and visibility
    let full_text = base.get_node_text(&node);
    let is_global = is_automatic || full_text.contains("Global:");

    let visibility = if is_global {
        Visibility::Public
    } else {
        Visibility::Private
    };
    let doc_comment = get_variable_documentation(is_environment, is_automatic, is_global, false);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Variable,
        SymbolOptions {
            signature: Some(full_text), // Use the full variable reference as signature
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: None,
            doc_comment: if doc_comment.is_empty() {
                None
            } else {
                Some(doc_comment)
            },
        },
    ))
}

/// Extract variable assignment signature
fn extract_variable_signature(base: &BaseExtractor, node: Node) -> String {
    let full_text = base.get_node_text(&node);
    let equal_index = full_text.find('=');

    if let Some(pos) = equal_index {
        if pos < full_text.len() - 1 {
            return full_text.trim().to_string();
        }
    }

    find_variable_name_node(node)
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "unknown".to_string())
}
