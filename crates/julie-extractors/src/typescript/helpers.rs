//! Helper functions for TypeScript extractor
//!
//! This module provides utility functions for tree traversal, node inspection,
//! and common extraction patterns used across other modules.

use crate::base::Visibility;
use tree_sitter::Node;

/// Check if a node has a modifier child of the given kind
///
/// Useful for checking for 'async', 'static', 'abstract', etc.
pub(super) fn has_modifier(node: Node, modifier_kind: &str) -> bool {
    node.children(&mut node.walk())
        .any(|child| child.kind() == modifier_kind)
}

/// Extract decorator names from child `decorator` nodes.
///
/// Returns decorator names like `@Component`, `@Injectable`, etc.
/// Works for class declarations (decorators are children) and
/// property definitions (decorators are children).
pub(super) fn extract_decorator_names(node: Node, content: &str) -> Vec<String> {
    let mut decorators = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "decorator" {
            if let Some(name) = extract_single_decorator_name(child, content) {
                decorators.push(name);
            }
        }
    }
    decorators
}

/// Extract decorator names from preceding sibling nodes.
///
/// In tree-sitter TypeScript, method decorators are siblings of the
/// method_definition inside class_body, not children of it. This function
/// walks backwards through preceding siblings to collect decorators.
pub(super) fn extract_preceding_decorator_names(node: Node, content: &str) -> Vec<String> {
    let mut decorators = Vec::new();
    let mut sibling = node.prev_sibling();
    while let Some(sib) = sibling {
        if sib.kind() == "decorator" {
            if let Some(name) = extract_single_decorator_name(sib, content) {
                decorators.insert(0, name); // prepend to maintain order
            }
        } else if sib.kind() != "comment" {
            // Stop at non-decorator, non-comment siblings
            break;
        }
        sibling = sib.prev_sibling();
    }
    decorators
}

/// Extract the name from a single decorator node.
///
/// Handles both `@Foo` (identifier decorator) and `@Foo(args)` (call expression decorator).
fn extract_single_decorator_name(decorator_node: Node, content: &str) -> Option<String> {
    let mut cursor = decorator_node.walk();
    for child in decorator_node.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                let name = content[child.byte_range()].to_string();
                return Some(format!("@{}", name));
            }
            "call_expression" => {
                // The function being called is the first child (identifier or member_expression)
                if let Some(func_node) = child.child_by_field_name("function") {
                    let name = content[func_node.byte_range()].to_string();
                    return Some(format!("@{}", name));
                }
            }
            _ => {}
        }
    }
    None
}

/// Build a decorator prefix string from decorator names (e.g., "@Component @Injectable ").
///
/// Returns empty string if no decorators.
pub(super) fn decorator_prefix(decorators: &[String]) -> String {
    if decorators.is_empty() {
        String::new()
    } else {
        format!("{} ", decorators.join(" "))
    }
}

/// Extract visibility from an `accessibility_modifier` child node.
///
/// In TypeScript tree-sitter, access modifiers produce `accessibility_modifier`
/// child nodes containing `public`, `private`, or `protected`.
pub(super) fn extract_ts_visibility(node: Node) -> Option<Visibility> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "accessibility_modifier" {
            let mut inner_cursor = child.walk();
            for inner_child in child.children(&mut inner_cursor) {
                match inner_child.kind() {
                    "public" => return Some(Visibility::Public),
                    "private" => return Some(Visibility::Private),
                    "protected" => return Some(Visibility::Protected),
                    _ => {}
                }
            }
        }
    }
    None
}

/// Check if a node has a `readonly` modifier child.
pub(super) fn has_readonly(node: Node) -> bool {
    has_modifier(node, "readonly")
}
