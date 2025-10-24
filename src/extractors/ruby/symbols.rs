use super::helpers::{extract_alias_name, extract_name_from_node, extract_singleton_method_name};
use super::signatures;
/// Symbol extraction for individual Ruby constructs
/// Handles extraction of modules, classes, methods, variables, constants, and aliases
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Node;

/// Extract a module symbol
pub(super) fn extract_module(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
    _current_visibility: Visibility,
) -> Symbol {
    // Try different field names that Ruby tree-sitter uses
    let name = extract_name_from_node(node, |n| base.get_node_text(n), "name")
        .or_else(|| extract_name_from_node(node, |n| base.get_node_text(n), "constant"))
        .or_else(|| {
            // Fallback: find first constant child
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "constant" {
                    return Some(base.get_node_text(&child));
                }
            }
            None
        })
        .unwrap_or_else(|| "UnknownModule".to_string());

    let signature = signatures::build_module_signature(&node, &name, |n| base.get_node_text(n));

    // Extract RDoc/YARD comment
    let doc_comment = base.find_doc_comment(&node);

    base.create_symbol(
        &node,
        name,
        SymbolKind::Module,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: None,
            doc_comment,
        },
    )
}

/// Extract a class symbol
pub(super) fn extract_class(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
    _current_visibility: Visibility,
) -> Symbol {
    // Try different field names that Ruby tree-sitter uses
    let name = extract_name_from_node(node, |n| base.get_node_text(n), "name")
        .or_else(|| extract_name_from_node(node, |n| base.get_node_text(n), "constant"))
        .or_else(|| {
            // Fallback: find first constant child
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "constant" {
                    return Some(base.get_node_text(&child));
                }
            }
            None
        })
        .unwrap_or_else(|| "UnknownClass".to_string());

    let signature = signatures::build_class_signature(&node, &name, |n| base.get_node_text(n));

    // Extract RDoc/YARD comment
    let doc_comment = base.find_doc_comment(&node);

    base.create_symbol(
        &node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: None,
            doc_comment,
        },
    )
}

/// Extract a singleton class symbol
pub(super) fn extract_singleton_class(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Symbol {
    // Find the target of the singleton class (self, identifier, etc.)
    let target_node = node
        .children(&mut node.walk())
        .find(|c| matches!(c.kind(), "self" | "identifier"));
    let target = target_node
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "self".to_string());
    let signature = format!("class << {}", target);

    // Extract RDoc/YARD comment
    let doc_comment = base.find_doc_comment(&node);

    base.create_symbol(
        &node,
        format!("<<{}", target),
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: None,
            doc_comment,
        },
    )
}

/// Extract a method symbol
pub(super) fn extract_method(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
    current_visibility: Visibility,
) -> Symbol {
    let name = extract_name_from_node(node, |n| base.get_node_text(n), "name")
        .or_else(|| extract_name_from_node(node, |n| base.get_node_text(n), "identifier"))
        .or_else(|| extract_name_from_node(node, |n| base.get_node_text(n), "operator"))
        .or_else(|| {
            // Fallback: find method name by traversing children
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "identifier" | "operator" => {
                        return Some(base.get_node_text(&child));
                    }
                    _ => continue,
                }
            }
            None
        })
        .unwrap_or_else(|| "unknownMethod".to_string());

    let signature = signatures::build_method_signature(&node, &name, |n| base.get_node_text(n));
    let kind = if name == "initialize" {
        SymbolKind::Constructor
    } else {
        SymbolKind::Method
    };

    // Extract RDoc/YARD comment
    let doc_comment = base.find_doc_comment(&node);

    base.create_symbol(
        &node,
        name,
        kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(current_visibility),
            parent_id,
            metadata: None,
            doc_comment,
        },
    )
}

/// Extract a singleton method symbol
pub(super) fn extract_singleton_method(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
    current_visibility: Visibility,
) -> Symbol {
    let name = extract_singleton_method_name(node, |n| base.get_node_text(n));
    let signature =
        signatures::build_singleton_method_signature(&node, &name, |n| base.get_node_text(n));

    // Extract RDoc/YARD comment
    let doc_comment = base.find_doc_comment(&node);

    base.create_symbol(
        &node,
        name,
        SymbolKind::Method,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(current_visibility),
            parent_id,
            metadata: None,
            doc_comment,
        },
    )
}

/// Extract a variable symbol
pub(super) fn extract_variable(base: &mut BaseExtractor, node: Node) -> Symbol {
    let name = base.get_node_text(&node);
    let signature = name.clone();

    // Extract RDoc/YARD comment
    let doc_comment = base.find_doc_comment(&node);

    base.create_symbol(
        &node,
        name,
        SymbolKind::Variable,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: None,
            metadata: None,
            doc_comment,
        },
    )
}

/// Extract a constant symbol
pub(super) fn extract_constant(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Symbol {
    let name = base.get_node_text(&node);
    let signature = name.clone();

    // Extract RDoc/YARD comment
    let doc_comment = base.find_doc_comment(&node);

    base.create_symbol(
        &node,
        name,
        SymbolKind::Constant,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: None,
            doc_comment,
        },
    )
}

/// Extract an alias symbol
pub(super) fn extract_alias(base: &mut BaseExtractor, node: Node) -> Symbol {
    let signature = base.get_node_text(&node);
    let alias_name = extract_alias_name(node, |n| base.get_node_text(n));

    // Extract RDoc/YARD comment
    let doc_comment = base.find_doc_comment(&node);

    base.create_symbol(
        &node,
        alias_name,
        SymbolKind::Method,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: None,
            metadata: None,
            doc_comment,
        },
    )
}
