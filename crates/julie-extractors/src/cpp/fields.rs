//! Field and multi-variable declaration extraction for C++
//! Handles class/struct member fields and multi-declarator statements like `int x, y, z;`

use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions};
use tree_sitter::Node;

use super::helpers;
use super::signatures;
use super::visibility;

/// Extract field declaration (class member variable)
/// Returns Vec<Symbol> because a single declaration can define multiple fields
/// Examples:
///   size_t rows, cols;  -> extracts both "rows" and "cols"
///   double* data;       -> extracts "data" (pointer_declarator)
pub(super) fn extract_field(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Vec<Symbol> {
    // C++ field declarations can have multiple declarators on same line
    // Also need to handle pointer_declarator for pointer fields
    let declarators: Vec<Node> = node
        .children(&mut node.walk())
        .filter(|c| {
            matches!(
                c.kind(),
                "field_declarator" | "init_declarator" | "pointer_declarator"
            )
        })
        .collect();

    if declarators.is_empty() {
        return extract_fields_without_declarators(base, node, parent_id);
    }

    // Get storage class and type specifiers once (shared by all declarators)
    let storage_class = helpers::extract_storage_class(base, node);
    let type_specifiers = helpers::extract_type_specifiers(base, node);
    let is_constant = helpers::is_constant_declaration(&storage_class, &type_specifiers);
    let is_static_member = helpers::is_static_member_variable(node, &storage_class);

    // Handle ALL declarators (size_t rows, cols; extracts both rows and cols)
    let mut symbols = Vec::new();
    let doc_comment = base.find_doc_comment(&node);
    for declarator in declarators {
        // Extract field name from declarator (handles pointer_declarator, field_declarator, etc.)
        let name_node = match extract_field_name_from_declarator(declarator) {
            Some(n) => n,
            None => continue, // Skip if we can't find the name
        };

        let name = base.get_node_text(&name_node);

        let kind = if is_constant || is_static_member {
            SymbolKind::Constant
        } else {
            SymbolKind::Field
        };

        // Build signature
        let signature = signatures::build_field_signature(base, node, &name);
        let vis = visibility::extract_visibility_from_node(base, node);

        symbols.push(base.create_symbol(
            &node,
            name,
            kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(vis),
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment: doc_comment.clone(),
            },
        ));
    }

    symbols
}

/// Handle field declarations that have no declarator children
/// (direct field_identifiers or method declarations)
fn extract_fields_without_declarators(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Vec<Symbol> {
    // Check for function_declarator (method declarations)
    if node
        .children(&mut node.walk())
        .any(|c| c.kind() == "function_declarator")
    {
        // This is a method declaration inside a class - not a field
        // Don't handle it here - let it be processed as a function
        return vec![];
    }

    // Check for direct field_identifiers
    let field_identifiers: Vec<Node> = node
        .children(&mut node.walk())
        .filter(|c| c.kind() == "field_identifier")
        .collect();

    if field_identifiers.is_empty() {
        return vec![];
    }

    // Get storage class and type specifiers once (shared by all fields)
    let storage_class = helpers::extract_storage_class(base, node);
    let type_specifiers = helpers::extract_type_specifiers(base, node);
    let is_constant = helpers::is_constant_declaration(&storage_class, &type_specifiers);
    let is_static_member = helpers::is_static_member_variable(node, &storage_class);

    // Create a symbol for EACH field_identifier (handles: size_t rows, cols;)
    let mut symbols = Vec::new();
    let doc_comment = base.find_doc_comment(&node);
    for field_node in field_identifiers {
        let name = base.get_node_text(&field_node);

        let kind = if is_constant || is_static_member {
            SymbolKind::Constant
        } else {
            SymbolKind::Field
        };

        // Build signature
        let signature = signatures::build_field_signature(base, node, &name);
        let vis = visibility::extract_visibility_from_node(base, node);

        symbols.push(base.create_symbol(
            &node,
            name,
            kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(vis),
                parent_id: parent_id.map(String::from),
                metadata: None,
                doc_comment: doc_comment.clone(),
            },
        ));
    }

    symbols
}

/// Extract field name from various declarator types
/// Handles: field_declarator, pointer_declarator, init_declarator, etc.
fn extract_field_name_from_declarator(declarator: Node) -> Option<Node> {
    // For pointer_declarator: need to recursively find field_identifier
    if declarator.kind() == "pointer_declarator" {
        return declarator.children(&mut declarator.walk()).find_map(|c| {
            if c.kind() == "field_identifier" || c.kind() == "identifier" {
                Some(c)
            } else if matches!(c.kind(), "pointer_declarator" | "field_declarator") {
                // Recursively search nested declarators (e.g., double**)
                extract_field_name_from_declarator(c)
            } else {
                None
            }
        });
    }

    // For field_declarator and init_declarator: direct children
    declarator
        .children(&mut declarator.walk())
        .find(|c| c.kind() == "field_identifier" || c.kind() == "identifier")
}

/// Extract additional variables from multi-variable declarations
/// For declarations like `int x = 1, y = 2, z = 3;` the first variable (x)
/// is extracted by extract_declaration(). This function extracts the remaining
/// variables (y, z) so they all appear as separate symbols.
pub(super) fn extract_multi_declarations(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Vec<Symbol> {
    let declarators: Vec<Node> = node
        .children(&mut node.walk())
        .filter(|c| c.kind() == "init_declarator")
        .collect();

    // Only relevant when there are 2+ declarators
    if declarators.len() < 2 {
        return vec![];
    }

    // Get shared properties from the declaration
    let storage_class = helpers::extract_storage_class(base, node);
    let type_specifiers = helpers::extract_type_specifiers(base, node);
    let is_constant = helpers::is_constant_declaration(&storage_class, &type_specifiers);
    let kind = if is_constant {
        SymbolKind::Constant
    } else {
        SymbolKind::Variable
    };

    let vis = visibility::extract_cpp_visibility(base, node);
    let doc_comment = base.find_doc_comment(&node);

    // Skip first declarator (already handled by extract_declaration)
    declarators
        .iter()
        .skip(1)
        .filter_map(|declarator| {
            let name_node = helpers::extract_declarator_name(*declarator)?;
            let name = base.get_node_text(&name_node);
            let signature = signatures::build_variable_signature(base, node, &name);

            Some(base.create_symbol(
                &node,
                name,
                kind.clone(),
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(vis.clone()),
                    parent_id: parent_id.map(String::from),
                    metadata: None,
                    doc_comment: doc_comment.clone(),
                },
            ))
        })
        .collect()
}
