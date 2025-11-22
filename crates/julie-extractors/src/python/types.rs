/// Class and type extraction for Python
/// Handles class definitions, enums, protocols, and type detection
use super::super::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use super::PythonExtractor;
use super::{decorators, helpers};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract a class definition from a class_definition node
pub(super) fn extract_class(extractor: &mut PythonExtractor, node: Node) -> Symbol {
    // For Python, the class name is typically the second child (after "class" keyword)
    let name = if let Some(identifier_node) = node.children(&mut node.walk()).nth(1) {
        if identifier_node.kind() == "identifier" {
            extractor.base_mut().get_node_text(&identifier_node)
        } else {
            "Anonymous".to_string()
        }
    } else {
        "Anonymous".to_string()
    };

    // Extract base classes and metaclass arguments
    let superclasses_node = node.child_by_field_name("superclasses");
    let mut extends_info = String::new();
    let mut is_enum = false;
    let mut is_protocol = false;
    let all_args = if let Some(superclasses) = superclasses_node {
        let all_args = helpers::extract_argument_list(extractor, &superclasses);

        // Separate regular base classes from keyword arguments
        let bases: Vec<_> = all_args
            .iter()
            .filter(|arg| !arg.contains('='))
            .cloned()
            .collect();
        let keyword_args: Vec<_> = all_args
            .iter()
            .filter(|arg| arg.contains('='))
            .cloned()
            .collect();

        // Check if this is an Enum class
        is_enum = bases
            .iter()
            .any(|base| base == "Enum" || base.contains("Enum"));

        // Check if this is a Protocol class (should be treated as Interface)
        is_protocol = bases
            .iter()
            .any(|base| base == "Protocol" || base.contains("Protocol"));

        // Build extends information
        let mut extends_parts = Vec::new();
        if !bases.is_empty() {
            extends_parts.push(format!("extends {}", bases.join(", ")));
        }

        // Add metaclass info if present
        if let Some(metaclass_arg) = keyword_args
            .iter()
            .find(|arg| arg.starts_with("metaclass="))
        {
            extends_parts.push(metaclass_arg.clone());
        }

        if !extends_parts.is_empty() {
            extends_info = format!(" {}", extends_parts.join(" "));
        }

        all_args
    } else {
        Vec::new()
    };

    // Extract decorators
    let decorators_list = decorators::extract_decorators(extractor, &node);
    let decorator_info = if decorators_list.is_empty() {
        String::new()
    } else {
        format!("@{} ", decorators_list.join(" @"))
    };

    let signature = format!("{}class {}{}", decorator_info, name, extends_info);

    // Determine the symbol kind based on base classes
    let symbol_kind = if is_enum {
        SymbolKind::Enum
    } else if is_protocol {
        SymbolKind::Interface
    } else {
        SymbolKind::Class
    };

    // Extract docstring
    let doc_comment = extract_docstring(extractor, &node);

    let mut metadata = HashMap::new();
    metadata.insert("decorators".to_string(), serde_json::json!(decorators_list));
    metadata.insert("superclasses".to_string(), serde_json::json!(all_args));
    metadata.insert("isEnum".to_string(), serde_json::json!(is_enum));

    extractor.base_mut().create_symbol(
        &node,
        name,
        symbol_kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: None,
            metadata: Some(metadata),
            doc_comment,
        },
    )
}

/// Extract docstring from a function or class
pub(super) fn extract_docstring(extractor: &PythonExtractor, node: &Node) -> Option<String> {
    let body_node = node.child_by_field_name("body")?;
    let base = extractor.base();

    // Look for first string in function/class body (Python docstrings are inside expression_statement nodes)
    let mut cursor = body_node.walk();
    for child in body_node.children(&mut cursor) {
        // Check if this is an expression_statement containing a string (typical for docstrings)
        if child.kind() == "expression_statement" {
            let mut expr_cursor = child.walk();
            for expr_child in child.children(&mut expr_cursor) {
                if expr_child.kind() == "string" {
                    let mut docstring = base.get_node_text(&expr_child);

                    // Remove quotes (single, double, or triple quotes)
                    docstring = helpers::strip_string_delimiters(&docstring);
                    return Some(docstring.trim().to_string());
                }
            }
        }
        // Also handle direct string nodes (just in case)
        else if child.kind() == "string" {
            let mut docstring = base.get_node_text(&child);

            // Remove quotes (single, double, or triple quotes)
            docstring = helpers::strip_string_delimiters(&docstring);
            return Some(docstring.trim().to_string());
        }
    }

    None
}

/// Check if a node is inside an enum class
pub(super) fn is_inside_enum_class(extractor: &PythonExtractor, node: &Node) -> bool {
    // Walk up the parent tree to find a class definition
    let mut current = *node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "class_definition" {
            // Check if this class extends Enum
            if let Some(superclasses_node) = parent.child_by_field_name("superclasses") {
                let superclasses = helpers::extract_argument_list(extractor, &superclasses_node);
                // Check if any base class is "Enum"
                return superclasses
                    .iter()
                    .any(|base| base == "Enum" || base.contains("Enum"));
            }
            // If we found a class but it doesn't extend anything, it's not an enum
            return false;
        }
        current = parent;
    }
    false
}
