//! Helper functions for Kotlin symbol extraction
//!
//! This module provides utility functions for extracting modifiers, visibility,
//! type information, and other metadata from Kotlin code.

use crate::extractors::base::{SymbolKind, Visibility};
use tree_sitter::Node;

/// Extract modifiers from a Kotlin node (public, private, open, sealed, data, etc.)
pub(super) fn extract_modifiers(
    base: &super::super::base::BaseExtractor,
    node: &Node,
) -> Vec<String> {
    let mut modifiers = Vec::new();
    let modifiers_list = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "modifiers");

    if let Some(modifiers_list) = modifiers_list {
        for child in modifiers_list.children(&mut modifiers_list.walk()) {
            if matches!(
                child.kind(),
                "class_modifier"
                    | "function_modifier"
                    | "property_modifier"
                    | "visibility_modifier"
                    | "inheritance_modifier"
                    | "member_modifier"
                    | "annotation"
                    | "public"
                    | "private"
                    | "protected"
                    | "internal"
                    | "open"
                    | "final"
                    | "abstract"
                    | "sealed"
                    | "data"
                    | "inline"
                    | "suspend"
                    | "operator"
                    | "infix"
            ) {
                modifiers.push(base.get_node_text(&child));
            }
        }
    }

    modifiers
}

/// Extract type parameters from a Kotlin node (e.g., <T, U>)
pub(super) fn extract_type_parameters(
    base: &super::super::base::BaseExtractor,
    node: &Node,
) -> Option<String> {
    let type_params = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "type_parameters");
    type_params.map(|tp| base.get_node_text(&tp))
}

/// Extract super types/base classes from a Kotlin node
pub(super) fn extract_super_types(
    base: &super::super::base::BaseExtractor,
    node: &Node,
) -> Option<String> {
    let mut super_types = Vec::new();

    // Look for delegation_specifiers container first (wrapped case)
    let delegation_container = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "delegation_specifiers");
    if let Some(delegation_container) = delegation_container {
        for child in delegation_container.children(&mut delegation_container.walk()) {
            if child.kind() == "delegation_specifier" {
                // Check for explicit_delegation (delegation syntax like "Drawable by drawable")
                let explicit_delegation = child
                    .children(&mut child.walk())
                    .find(|n| n.kind() == "explicit_delegation");
                if let Some(explicit_delegation) = explicit_delegation {
                    // Get the full delegation text including "by" keyword
                    super_types.push(base.get_node_text(&explicit_delegation));
                } else {
                    // Fallback: simple inheritance without delegation
                    let type_node = child.children(&mut child.walk()).find(|n| {
                        matches!(
                            n.kind(),
                            "type" | "user_type" | "identifier" | "constructor_invocation"
                        )
                    });
                    if let Some(type_node) = type_node {
                        if type_node.kind() == "constructor_invocation" {
                            // For constructor invocations like Result<Nothing>(), include the full call
                            super_types.push(base.get_node_text(&type_node));
                        } else {
                            super_types.push(base.get_node_text(&type_node));
                        }
                    }
                }
            } else if matches!(child.kind(), "type" | "user_type" | "identifier") {
                super_types.push(base.get_node_text(&child));
            }
        }
    } else {
        // Look for individual delegation_specifier nodes (multiple at same level)
        let delegation_specifiers: Vec<Node> = node
            .children(&mut node.walk())
            .filter(|n| n.kind() == "delegation_specifier")
            .collect();
        for delegation in delegation_specifiers {
            let explicit_delegation = delegation
                .children(&mut delegation.walk())
                .find(|n| n.kind() == "explicit_delegation");
            if let Some(explicit_delegation) = explicit_delegation {
                super_types.push(base.get_node_text(&explicit_delegation));
            } else {
                // Check for constructor_invocation to include ()
                let constructor_invocation = delegation
                    .children(&mut delegation.walk())
                    .find(|n| n.kind() == "constructor_invocation");
                if let Some(constructor_invocation) = constructor_invocation {
                    super_types.push(base.get_node_text(&constructor_invocation));
                } else {
                    super_types.push(base.get_node_text(&delegation));
                }
            }
        }
    }

    if super_types.is_empty() {
        None
    } else {
        Some(super_types.join(", "))
    }
}

/// Extract function parameters from a Kotlin node
pub(super) fn extract_parameters(
    base: &super::super::base::BaseExtractor,
    node: &Node,
) -> Option<String> {
    let params = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "function_value_parameters");
    params.map(|p| base.get_node_text(&p))
}

/// Extract return type from a Kotlin function node
pub(super) fn extract_return_type(
    base: &super::super::base::BaseExtractor,
    node: &Node,
) -> Option<String> {
    let mut found_colon = false;
    for child in node.children(&mut node.walk()) {
        if child.kind() == ":" {
            found_colon = true;
            continue;
        }
        if found_colon
            && matches!(
                child.kind(),
                "type" | "user_type" | "identifier" | "function_type" | "nullable_type"
            )
        {
            return Some(base.get_node_text(&child));
        }
    }
    None
}

/// Extract property type from a Kotlin property node
pub(super) fn extract_property_type(
    base: &super::super::base::BaseExtractor,
    node: &Node,
) -> Option<String> {
    // Look for type in variable_declaration (interface properties)
    let var_decl = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "variable_declaration");
    if let Some(var_decl) = var_decl {
        let user_type = var_decl.children(&mut var_decl.walk()).find(|n| {
            matches!(
                n.kind(),
                "user_type" | "type" | "nullable_type" | "type_reference"
            )
        });
        if let Some(user_type) = user_type {
            return Some(base.get_node_text(&user_type));
        }
    }

    // Look for direct type node (regular properties)
    let property_type = node.children(&mut node.walk()).find(|n| {
        matches!(
            n.kind(),
            "type" | "user_type" | "nullable_type" | "type_reference"
        )
    });
    property_type.map(|n| base.get_node_text(&n))
}

/// Extract property initializer (the value after `=`)
pub(super) fn extract_property_initializer(
    base: &super::super::base::BaseExtractor,
    node: &Node,
) -> Option<String> {
    let children: Vec<Node> = node.children(&mut node.walk()).collect();
    if let Some(assignment_index) = children.iter().position(|n| base.get_node_text(n) == "=") {
        if assignment_index + 1 < children.len() {
            let initializer_node = &children[assignment_index + 1];
            return Some(base.get_node_text(initializer_node).trim().to_string());
        }
    }

    // Also check for property_initializer node type
    let initializer_node = node
        .children(&mut node.walk())
        .find(|n| matches!(n.kind(), "property_initializer" | "expression" | "literal"));
    initializer_node.map(|n| base.get_node_text(&n).trim().to_string())
}

/// Extract property delegation (e.g., `by lazy { ... }`)
pub(super) fn extract_property_delegation(
    base: &super::super::base::BaseExtractor,
    node: &Node,
) -> Option<String> {
    let children: Vec<Node> = node.children(&mut node.walk()).collect();
    if let Some(by_index) = children.iter().position(|n| base.get_node_text(n) == "by") {
        if by_index + 1 < children.len() {
            let delegate_node = &children[by_index + 1];
            return Some(format!("by {}", base.get_node_text(delegate_node)));
        }
    }

    // Also check for property_delegate node type
    let delegate_node = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "property_delegate");
    delegate_node.map(|n| base.get_node_text(&n))
}

/// Extract where clause from a function declaration
pub(super) fn extract_where_clause(
    base: &super::super::base::BaseExtractor,
    node: &Node,
) -> Option<String> {
    let type_constraints = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "type_constraints");

    if let Some(type_constraints) = type_constraints {
        let mut constraints = Vec::new();

        for child in type_constraints.children(&mut type_constraints.walk()) {
            if child.kind() == "type_constraint" {
                constraints.push(base.get_node_text(&child));
            }
        }

        if !constraints.is_empty() {
            return Some(format!("where {}", constraints.join(", ")));
        }
    }

    None
}

/// Extract receiver type for extension functions (e.g., `String.functionName`)
pub(super) fn extract_receiver_type(
    base: &super::super::base::BaseExtractor,
    node: &Node,
) -> Option<String> {
    let children: Vec<_> = node.children(&mut node.walk()).collect();

    // Find the pattern: user_type followed by "."
    for i in 0..children.len().saturating_sub(1) {
        if children[i].kind() == "user_type"
            && i + 1 < children.len()
            && base.get_node_text(&children[i + 1]) == "."
        {
            return Some(base.get_node_text(&children[i]));
        }
    }

    None
}

/// Extract primary constructor signature
pub(super) fn extract_primary_constructor_signature(
    base: &super::super::base::BaseExtractor,
    node: &Node,
) -> Option<String> {
    let primary_constructor = node
        .children(&mut node.walk())
        .find(|n| n.kind() == "primary_constructor");
    let primary_constructor = primary_constructor?;

    Some(base.get_node_text(&primary_constructor))
}

/// Determine the kind of class (Class, Enum, Data class, Sealed class)
pub(super) fn determine_class_kind(
    base: &super::super::base::BaseExtractor,
    modifiers: &[String],
    node: &Node,
) -> SymbolKind {
    // Check if this is an enum declaration by node type
    if node.kind() == "enum_declaration" {
        return SymbolKind::Enum;
    }

    // Check for enum class by looking for 'enum' keyword in the node
    let has_enum_keyword = node
        .children(&mut node.walk())
        .any(|n| base.get_node_text(&n) == "enum");
    if has_enum_keyword {
        return SymbolKind::Enum;
    }

    // Check modifiers
    if modifiers.contains(&"enum".to_string()) || modifiers.contains(&"enum class".to_string()) {
        return SymbolKind::Enum;
    }
    if modifiers.contains(&"data".to_string()) {
        return SymbolKind::Class;
    }
    if modifiers.contains(&"sealed".to_string()) {
        return SymbolKind::Class;
    }
    SymbolKind::Class
}

/// Determine visibility from modifiers (Public, Private, Protected)
pub(super) fn determine_visibility(modifiers: &[String]) -> Visibility {
    if modifiers.contains(&"private".to_string()) {
        Visibility::Private
    } else if modifiers.contains(&"protected".to_string()) {
        Visibility::Protected
    } else {
        Visibility::Public // Kotlin defaults to public
    }
}
