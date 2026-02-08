/// Helper utilities for Ruby symbol extraction
/// Includes node name extraction, type inference, and context checking
use crate::base::{SymbolKind, Visibility};
use tree_sitter::Node;

/// Extract a name from a node by field name
pub(super) fn extract_name_from_node(
    node: Node,
    base_get_text: impl Fn(&Node) -> String,
    field_name: &str,
) -> Option<String> {
    node.child_by_field_name(field_name)
        .map(|name_node| base_get_text(&name_node))
}

/// Build a namespace-aware qualified name by walking up parent modules/classes
pub(super) fn build_qualified_name(
    node: Node,
    name: &str,
    base_get_text: impl Fn(&Node) -> String,
) -> String {
    let mut namespace_parts = Vec::new();
    let mut current = node;

    // Walk up the tree to find parent modules/classes
    while let Some(parent) = current.parent() {
        if matches!(parent.kind(), "module" | "class") {
            // Extract the name of the parent module/class
            if let Some(parent_name) = extract_name_from_node(parent, &base_get_text, "name")
                .or_else(|| extract_name_from_node(parent, &base_get_text, "constant"))
                .or_else(|| {
                    // Fallback: find first constant child
                    let mut cursor = parent.walk();
                    for child in parent.children(&mut cursor) {
                        if child.kind() == "constant" {
                            return Some(base_get_text(&child));
                        }
                    }
                    None
                })
            {
                namespace_parts.push(parent_name);
            }
        }
        current = parent;
    }

    // Reverse to get the correct order (outermost first)
    namespace_parts.reverse();

    // If we have namespace parts, join them with ::
    if namespace_parts.is_empty() {
        name.to_string()
    } else {
        format!("{}::{}", namespace_parts.join("::"), name)
    }
}

/// Infer symbol kind from assignment node (constant vs variable)
pub(super) fn infer_symbol_kind_from_assignment(
    left_node: &Node,
    base_get_text: impl Fn(&Node) -> String,
) -> SymbolKind {
    match left_node.kind() {
        "constant" => SymbolKind::Constant,
        "class_variable" | "instance_variable" | "global_variable" => SymbolKind::Variable,
        _ => {
            let text = base_get_text(left_node);
            if text.chars().all(|c| c.is_uppercase() || c == '_') {
                SymbolKind::Constant
            } else {
                SymbolKind::Variable
            }
        }
    }
}

/// Check if a node is part of an assignment
pub(super) fn is_part_of_assignment(node: &Node) -> bool {
    let mut current = *node;
    while let Some(parent) = current.parent() {
        if matches!(parent.kind(), "assignment" | "operator_assignment") {
            return true;
        }
        current = parent;
    }
    false
}

/// Check if a node is the left-hand side target of an assignment.
/// Used to skip duplicate symbol creation for constants already handled by the assignment.
pub(super) fn is_assignment_target(node: &Node) -> bool {
    node.parent().is_some_and(|p| {
        matches!(p.kind(), "assignment" | "operator_assignment")
            && p.child_by_field_name("left")
                .is_some_and(|left| left.id() == node.id())
    })
}

/// Extract method name from a call node
pub(super) fn extract_method_name_from_call(
    node: Node,
    base_get_text: impl Fn(&Node) -> String,
) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return Some(base_get_text(&child));
        }
    }
    None
}

/// Extract method name from a singleton method node
pub(super) fn extract_singleton_method_name(
    node: Node,
    base_get_text: impl Fn(&Node) -> String,
) -> Option<String> {
    // Ruby singleton method structure: def target.method_name
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" && child.prev_sibling().is_some_and(|s| s.kind() == ".") {
            return Some(base_get_text(&child));
        }
    }
    None
}

/// Extract target of a singleton method (e.g., 'self' or object name)
pub(super) fn extract_singleton_method_target(
    node: Node,
    base_get_text: impl Fn(&Node) -> String,
) -> String {
    // Find the target before the dot
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if (child.kind() == "identifier" || child.kind() == "self")
            && child.next_sibling().is_some_and(|s| s.kind() == ".")
        {
            return base_get_text(&child);
        }
    }
    "self".to_string()
}

/// Extract alias name from an alias node
pub(super) fn extract_alias_name(node: Node, base_get_text: impl Fn(&Node) -> String) -> Option<String> {
    // alias new_name old_name - extract the new_name
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();

    if children.len() >= 2 {
        Some(base_get_text(&children[1]))
    } else {
        None
    }
}

/// Find all include/extend/prepend/using calls within a node
pub(super) fn find_includes_and_extends(
    node: &Node,
    extract_method_name: impl Fn(Node) -> Option<String> + Copy,
    base_get_text: impl Fn(&Node) -> String + Copy,
) -> Vec<String> {
    let mut includes = Vec::new();
    find_includes_and_extends_recursive(*node, &mut includes, extract_method_name, base_get_text);
    includes
}

fn find_includes_and_extends_recursive(
    node: Node,
    includes: &mut Vec<String>,
    extract_method_name: impl Fn(Node) -> Option<String> + Copy,
    base_get_text: impl Fn(&Node) -> String + Copy,
) {
    // Check if this node itself is a call node for include/extend/prepend
    if node.kind() == "call" {
        if let Some(method_name) = extract_method_name(node) {
            if matches!(
                method_name.as_str(),
                "include" | "extend" | "prepend" | "using"
            ) {
                includes.push(base_get_text(&node));
            }
        }
    }

    // Recursively search children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        find_includes_and_extends_recursive(child, includes, extract_method_name, base_get_text);
    }
}

/// Parse visibility from identifier string.
/// Includes `module_function` which makes subsequent methods public as module-level functions.
pub(super) fn parse_visibility(text: &str) -> Option<Visibility> {
    match text {
        "private" => Some(Visibility::Private),
        "protected" => Some(Visibility::Protected),
        "public" | "module_function" => Some(Visibility::Public),
        _ => None,
    }
}
