use super::helpers::{
    extract_method_name_from_call, extract_singleton_method_target, find_includes_and_extends,
};
/// Signature building for Ruby symbols
/// Handles construction of method signatures, class signatures, and module signatures
use tree_sitter::Node;

/// Build module signature with includes/extends
pub(super) fn build_module_signature(
    node: &Node,
    name: &str,
    base_get_text: impl Fn(&Node) -> String + Copy,
) -> String {
    let qualified_name = super::helpers::build_qualified_name(*node, name, base_get_text);
    let mut signature = format!("module {}", qualified_name);

    // Look for include/extend statements
    let includes = find_includes_and_extends(
        node,
        |n| extract_method_name_from_call(n, base_get_text),
        base_get_text,
    );
    if !includes.is_empty() {
        signature.push_str(&format!("\n  {}", includes.join("\n  ")));
    }

    signature
}

/// Build class signature with superclass and includes/extends
pub(super) fn build_class_signature(
    node: &Node,
    name: &str,
    base_get_text: impl Fn(&Node) -> String + Copy,
) -> String {
    let qualified_name = super::helpers::build_qualified_name(*node, name, base_get_text);
    let mut signature = format!("class {}", qualified_name);

    // Check for inheritance
    if let Some(superclass) = node.child_by_field_name("superclass") {
        let superclass_name = base_get_text(&superclass)
            .replace('<', "")
            .trim()
            .to_string();
        signature.push_str(&format!(" < {}", superclass_name));
    }

    // Look for include/extend statements
    let includes = find_includes_and_extends(
        node,
        |n| extract_method_name_from_call(n, base_get_text),
        base_get_text,
    );
    if !includes.is_empty() {
        signature.push_str(&format!("\n  {}", includes.join("\n  ")));
    }

    signature
}

/// Build method signature with parameters and return statements
pub(super) fn build_method_signature(
    node: &Node,
    name: &str,
    base_get_text: impl Fn(&Node) -> String + Copy,
) -> String {
    let mut signature = format!("def {}", name);

    // Try different field names for parameters
    if let Some(params) = node.child_by_field_name("parameters") {
        signature.push_str(&base_get_text(&params));
    } else if let Some(params) = node.child_by_field_name("method_parameters") {
        signature.push_str(&base_get_text(&params));
    } else {
        // Fallback: look for parameter list node
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if matches!(
                child.kind(),
                "parameters" | "method_parameters" | "parameter_list"
            ) {
                signature.push_str(&base_get_text(&child));
                return signature;
            }
        }
        signature.push_str("()");
    }

    // Extract return statements from method body to include in signature
    if let Some(body) = node.child_by_field_name("body") {
        extract_return_statements_from_body(&body, &mut signature, base_get_text);
    } else {
        // Fallback: look for method body in children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "body_statement" || child.kind() == "block" {
                extract_return_statements_from_body(&child, &mut signature, base_get_text);
                break;
            }
        }
    }

    signature
}

/// Extract return statements from a method body and append to signature
fn extract_return_statements_from_body(
    body_node: &Node,
    signature: &mut String,
    base_get_text: impl Fn(&Node) -> String + Copy,
) {
    let mut cursor = body_node.walk();
    for child in body_node.children(&mut cursor) {
        if child.kind() == "return" {
            let return_text = base_get_text(&child);
            if !signature.contains(&return_text) {
                signature.push_str(&format!("\n  {}", return_text));
            }
        } else {
            // Recursively search for return statements in nested blocks
            extract_return_statements_from_body(&child, signature, base_get_text);
        }
    }
}

/// Build singleton method signature (e.g., def obj.method(...))
pub(super) fn build_singleton_method_signature(
    node: &Node,
    name: &str,
    base_get_text: impl Fn(&Node) -> String + Copy,
) -> String {
    let target = extract_singleton_method_target(*node, base_get_text);
    let mut signature = format!("def {}.{}", target, name);

    if let Some(params) = node.child_by_field_name("parameters") {
        signature.push_str(&base_get_text(&params));
    } else {
        signature.push_str("()");
    }

    signature
}
