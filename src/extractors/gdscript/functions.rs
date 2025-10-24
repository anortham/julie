//! Function and constructor extraction for GDScript

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Node;

const LIFECYCLE_PREFIXES: &[&str] = &[
    "_ready",
    "_enter_tree",
    "_exit_tree",
    "_process",
    "_physics_process",
    "_input",
    "_unhandled_input",
    "_unhandled_key_input",
    "_notification",
    "_draw",
    "_on_",
    "_handle_",
];

/// Extract constructor (_init) definition
pub(super) fn extract_constructor_definition(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
) -> Option<Symbol> {
    let signature = base.get_node_text(&node);

    // Extract doc comment
    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        "_init".to_string(),
        SymbolKind::Constructor,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.cloned(),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract function or method definition
pub(super) fn extract_function_definition(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
    symbols: &[Symbol],
) -> Option<Symbol> {
    let (name_node, _func_node, parent_node) = if node.kind() == "function_definition" {
        // Processing function_definition node - find child nodes
        let children = node.children(&mut node.walk()).collect::<Vec<_>>();
        let func_node = children.iter().find(|c| c.kind() == "func").cloned();
        let name_node = children.iter().find(|c| c.kind() == "name").cloned();
        (name_node, func_node, Some(node))
    } else if node.kind() == "func" {
        // Processing func node - look for sibling name node
        let parent_node = node.parent()?;
        let mut name_node = None;

        // Find func index and look for name after it
        for i in 0..parent_node.child_count() {
            if let Some(child) = parent_node.child(i) {
                if child.id() == node.id() {
                    // Found func node, look for name after it
                    for j in (i + 1)..parent_node.child_count() {
                        if let Some(sibling) = parent_node.child(j) {
                            if sibling.kind() == "name" {
                                name_node = Some(sibling);
                                break;
                            }
                        }
                    }
                    break;
                }
            }
        }
        (name_node, Some(node), Some(parent_node))
    } else {
        return None;
    };

    let name_node = name_node?;
    let parent_node = parent_node?;
    let name = base.get_node_text(&name_node);
    let signature = base.get_node_text(&parent_node);

    // Determine visibility based on naming convention
    let visibility = if name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Public
    };

    // Determine symbol kind based on context and name
    let kind = determine_function_kind(base, &name, parent_id, symbols);

    // Extract doc comment
    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        name,
        kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.cloned(),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Determine if a function is a method or standalone function based on context
fn determine_function_kind(
    _base: &mut BaseExtractor,
    name: &str,
    parent_id: Option<&String>,
    symbols: &[Symbol],
) -> SymbolKind {
    if name == "_init" {
        return SymbolKind::Constructor;
    }

    let Some(parent_id) = parent_id else {
        return SymbolKind::Function;
    };

    // Find the parent symbol to determine context
    let Some(parent_symbol) = symbols.iter().find(|s| &s.id == parent_id) else {
        return SymbolKind::Function;
    };

    if parent_symbol.kind != SymbolKind::Class {
        return SymbolKind::Function;
    }

    let is_implicit_class = parent_symbol
        .signature
        .as_ref()
        .map(|s| s.contains("extends") && !s.contains("class_name") && !s.contains("class "))
        .unwrap_or(false);

    let is_explicit_class = parent_symbol
        .signature
        .as_ref()
        .map(|s| s.contains("class_name"))
        .unwrap_or(false);

    let is_inner_class = parent_symbol
        .signature
        .as_ref()
        .map(|s| s.contains("class ") && !s.contains("class_name"))
        .unwrap_or(false);

    if is_implicit_class {
        // In implicit classes, only lifecycle callbacks and setget functions are methods
        let is_lifecycle_callback = name.starts_with('_')
            && LIFECYCLE_PREFIXES
                .iter()
                .any(|prefix| name.starts_with(prefix));

        // Check if this function is associated with a property (setget)
        let is_setget_function = is_setget_function_in_symbols(name, symbols);

        if is_lifecycle_callback || is_setget_function {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        }
    } else if is_explicit_class || is_inner_class {
        // In explicit classes and inner classes, all functions are methods
        SymbolKind::Method
    } else {
        SymbolKind::Method
    }
}

/// Check if a function name appears in any setget property signature
fn is_setget_function_in_symbols(function_name: &str, symbols: &[Symbol]) -> bool {
    symbols.iter().any(|s| {
        s.kind == SymbolKind::Field
            && s.signature.as_ref().is_some_and(|sig| {
                sig.contains("setget")
                    && (sig.contains(&format!("setget {}", function_name))
                        || sig.contains(&format!(", {}", function_name))
                        || sig.contains(&format!("{}, ", function_name)))
            })
    })
}
