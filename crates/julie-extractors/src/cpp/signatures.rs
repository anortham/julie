//! Signature-building helpers for C++ declarations
//! Constructs human-readable type signatures for variables and fields.

use crate::base::BaseExtractor;
use tree_sitter::Node;

use super::helpers;

/// Build signature for a direct variable declaration (no init_declarator)
/// Example: `extern int count` or `static const double PI`
pub(super) fn build_direct_variable_signature(
    base: &mut BaseExtractor,
    node: Node,
    name: &str,
) -> String {
    let mut signature = String::new();

    // Add storage class
    let storage_class = helpers::extract_storage_class(base, node);
    if !storage_class.is_empty() {
        signature.push_str(&storage_class.join(" "));
        signature.push(' ');
    }

    // Add type specifiers
    let type_specifiers = helpers::extract_type_specifiers(base, node);
    if !type_specifiers.is_empty() {
        signature.push_str(&type_specifiers.join(" "));
        signature.push(' ');
    }

    // Add type
    for child in node.children(&mut node.walk()) {
        if matches!(
            child.kind(),
            "primitive_type" | "type_identifier" | "qualified_identifier"
        ) {
            signature.push_str(&base.get_node_text(&child));
            signature.push(' ');
            break;
        }
    }

    signature.push_str(name);
    signature
}

/// Build signature for a variable with init_declarator
/// Example: `const int MAX_SIZE` or `static std::string name`
pub(super) fn build_variable_signature(
    base: &mut BaseExtractor,
    node: Node,
    name: &str,
) -> String {
    let mut signature = String::new();

    // Add storage class and type specifiers
    let storage_class = helpers::extract_storage_class(base, node);
    let type_specifiers = helpers::extract_type_specifiers(base, node);

    let mut parts = Vec::new();
    parts.extend(storage_class);
    parts.extend(type_specifiers);

    // Add type from node
    for child in node.children(&mut node.walk()) {
        if matches!(
            child.kind(),
            "primitive_type" | "type_identifier" | "qualified_identifier"
        ) {
            parts.push(base.get_node_text(&child));
            break;
        }
    }

    if !parts.is_empty() {
        signature.push_str(&parts.join(" "));
        signature.push(' ');
    }

    signature.push_str(name);
    signature
}

/// Build signature for a field (class/struct member variable)
/// Example: `static const size_t rows` or `double* data`
pub(super) fn build_field_signature(
    base: &mut BaseExtractor,
    node: Node,
    name: &str,
) -> String {
    let mut signature = String::new();

    // Add storage class and type specifiers
    let storage_class = helpers::extract_storage_class(base, node);
    let type_specifiers = helpers::extract_type_specifiers(base, node);

    let mut parts = Vec::new();
    parts.extend(storage_class);
    parts.extend(type_specifiers);

    // Add type from node
    for child in node.children(&mut node.walk()) {
        if matches!(
            child.kind(),
            "primitive_type" | "type_identifier" | "qualified_identifier"
        ) {
            parts.push(base.get_node_text(&child));
            break;
        }
    }

    if !parts.is_empty() {
        signature.push_str(&parts.join(" "));
        signature.push(' ');
    }

    signature.push_str(name);
    signature
}
