// Dart Extractor - Helper Functions
//
// Dart-specific helper methods for detecting modifiers, attributes, and patterns

use tree_sitter::Node;

pub(super) use crate::base::find_child_by_type;

/// Get text content of a node - THREAD-SAFE PER-THREAD CACHE
/// Uses thread-local storage to avoid race conditions in parallel tests
use std::cell::RefCell;

thread_local! {
    static DART_CONTENT_CACHE: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn set_dart_content_cache(content: &str) {
    DART_CONTENT_CACHE.with(|cache| {
        *cache.borrow_mut() = content.as_bytes().to_vec();
    });
}

pub(super) fn get_node_text(node: &Node) -> String {
    DART_CONTENT_CACHE.with(|cache| {
        let cache = cache.borrow();
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();

        if start_byte >= cache.len() || end_byte > cache.len() {
            return String::new();
        }

        String::from_utf8_lossy(&cache[start_byte..end_byte]).to_string()
    })
}

/// Recursively traverse tree and call callback on each node
pub(super) fn traverse_tree<F>(node: Node, callback: &mut F)
where
    F: FnMut(Node),
{
    callback(node);

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        traverse_tree(child, callback);
    }
}

/// Check if a class is marked as abstract
pub(super) fn is_abstract_class(node: &Node) -> bool {
    get_node_text(node).contains("abstract")
}

/// Check if a function is async
pub(super) fn is_async_function(node: &Node, _content: &str) -> bool {
    // Check if the node text contains async (fallback)
    if get_node_text(node).contains("async") {
        return true;
    }

    // For function_signature nodes, check the sibling function_body for async keyword
    if node.kind() == "function_signature" {
        if let Some(function_body) = node.next_sibling() {
            if function_body.kind() == "function_body"
                && find_child_by_type(&function_body, "async").is_some()
            {
                return true;
            }
        }
    }

    false
}

/// Check if a method is static
pub(super) fn is_static_method(node: &Node) -> bool {
    // Check if the node text contains static
    if get_node_text(node).contains("static") {
        return true;
    }

    // Check if previous sibling is a static keyword (for parsing edge cases)
    let mut current = node.prev_sibling();
    while let Some(sibling) = current {
        if sibling.kind() == "static" || get_node_text(&sibling) == "static" {
            return true;
        }
        // Don't go too far back
        if sibling.kind() == ";" || sibling.kind() == "}" {
            break;
        }
        current = sibling.prev_sibling();
    }

    false
}

/// Check if a method is marked as @override
pub(super) fn is_override_method(node: &Node, content: &str) -> bool {
    // Check if the node text contains @override (fallback)
    let node_text = get_node_text(node);
    if node_text.contains("@override") {
        return true;
    }

    // Direct source text approach: Look for @override in the lines before this method
    let start_row = node.start_position().row;
    let source_lines: Vec<&str> = content.lines().collect();

    // Check up to 3 lines before the method for @override annotation
    let check_start = start_row.saturating_sub(3);
    for line_idx in check_start..start_row {
        if line_idx < source_lines.len() {
            let line = source_lines[line_idx].trim();
            if line == "@override" {
                return true;
            }
        }
    }

    // Also try tree traversal as backup
    check_node_for_override_annotation(node)
}

fn check_node_for_override_annotation(node: &Node) -> bool {
    // For method_signature nodes, check the parent node's siblings first
    let target_node = if node.kind() == "method_signature" {
        node.parent().unwrap_or(*node)
    } else {
        *node
    };

    // Check siblings of the current node
    let mut current = target_node.prev_sibling();
    while let Some(sibling) = current {
        let sibling_text = get_node_text(&sibling);

        // Check if this sibling is an annotation with @override
        if is_annotation_node(sibling.kind()) && sibling_text.contains("@override") {
            return true;
        }

        // Also check nested annotation nodes within siblings
        if find_override_annotation_in_subtree(&sibling) {
            return true;
        }

        // Stop if we hit a substantive non-annotation node
        if !sibling_text.trim().is_empty()
            && sibling.kind() != "annotation"
            && sibling.kind() != "marker_annotation"
            && !sibling_text.chars().all(|c| c.is_whitespace())
        {
            break;
        }
        current = sibling.prev_sibling();
    }

    false
}

fn find_override_annotation_in_subtree(node: &Node) -> bool {
    // Check current node
    let node_text = get_node_text(node);
    if is_annotation_node(node.kind()) && node_text.contains("@override") {
        return true;
    }

    // Check children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if find_override_annotation_in_subtree(&child) {
            return true;
        }
    }

    false
}

/// Extract annotation names from sibling nodes preceding a declaration.
///
/// Walks previous siblings looking for `annotation` kind nodes (e.g. `@isTest`,
/// `@override`, `@pragma('vm:prefer-inline')`). Returns the annotation text
/// stripped of the leading `@`.
pub(super) fn extract_annotations(node: &Node) -> Vec<String> {
    let mut annotations = Vec::new();

    let target_node = if annotation_parent_should_be_used(node.kind()) {
        node.parent().unwrap_or(*node)
    } else {
        *node
    };

    let mut current = target_node.prev_sibling();
    while let Some(sibling) = current {
        let sibling_text = get_node_text(&sibling);

        if is_annotation_node(sibling.kind()) {
            // Strip leading @ and trim whitespace
            let name = sibling_text
                .trim()
                .strip_prefix('@')
                .unwrap_or(&sibling_text)
                .to_string();
            if !name.is_empty() {
                annotations.push(name);
            }
        } else if !sibling_text.trim().is_empty()
            && !sibling_text.chars().all(|c| c.is_whitespace())
        {
            // Stop at substantive non-annotation node
            break;
        }

        current = sibling.prev_sibling();
    }

    // Also check subtree for annotation nodes (for nested structures)
    collect_annotations_from_subtree(&target_node, &mut annotations);

    annotations
}

fn annotation_parent_should_be_used(kind: &str) -> bool {
    matches!(
        kind,
        "function_signature"
            | "method_signature"
            | "constructor_signature"
            | "factory_constructor_signature"
            | "constant_constructor_signature"
            | "getter_signature"
            | "setter_signature"
    )
}

fn collect_annotations_from_subtree(node: &Node, annotations: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            kind if is_annotation_node(kind) => {
                let text = get_node_text(&child);
                let name = text.trim().strip_prefix('@').unwrap_or(&text).to_string();
                if !name.is_empty() && !annotations.contains(&name) {
                    annotations.push(name);
                }
            }
            "metadata" | "metadata_star" => collect_annotations_from_subtree(&child, annotations),
            _ => {}
        }
    }
}

fn is_annotation_node(kind: &str) -> bool {
    matches!(kind, "annotation" | "marker_annotation")
}

/// Check if a constructor is a factory constructor
pub(super) fn is_factory_constructor(node: &Node) -> bool {
    get_node_text(node).contains("factory")
}

/// Check if a constructor is const
pub(super) fn is_const_constructor(node: &Node) -> bool {
    get_node_text(node).contains("const")
}

/// Check if a variable is final
pub(super) fn is_final_variable(node: &Node) -> bool {
    get_node_text(node).contains("final")
}

/// Check if a variable is const
pub(super) fn is_const_variable(node: &Node) -> bool {
    get_node_text(node).contains("const")
}

/// Check if a class is a Flutter widget
pub(super) fn is_flutter_widget(class_node: &Node) -> bool {
    if let Some(extends_clause) = find_child_by_type(class_node, "superclass") {
        let superclass_name = get_node_text(&extends_clause);
        let flutter_widgets = [
            "StatelessWidget",
            "StatefulWidget",
            "Widget",
            "PreferredSizeWidget",
            "RenderObjectWidget",
            "SingleChildRenderObjectWidget",
            "MultiChildRenderObjectWidget",
        ];

        flutter_widgets
            .iter()
            .any(|widget| superclass_name.contains(widget))
    } else {
        false
    }
}

/// Check if a method is a Flutter lifecycle method
pub(super) fn is_flutter_lifecycle_method(method_name: &str) -> bool {
    let lifecycle_methods = [
        "initState",
        "dispose",
        "build",
        "didChangeDependencies",
        "didUpdateWidget",
        "deactivate",
        "setState",
    ];
    lifecycle_methods.contains(&method_name)
}
