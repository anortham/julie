use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract function/method declarations
pub(super) fn extract_function(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
    is_public_fn: fn(&BaseExtractor, Node) -> bool,
    is_export_fn: fn(&BaseExtractor, Node) -> bool,
    is_inside_struct_fn: fn(Node) -> bool,
) -> Option<Symbol> {
    let name_node = base.find_child_by_type(&node, "identifier")?;
    let name = base.get_node_text(&name_node);

    // Check function visibility and modifiers
    let is_public = is_public_fn(base, node);
    let is_export = is_export_fn(base, node);
    let is_inside_struct = is_inside_struct_fn(node);

    let symbol_kind = if is_inside_struct {
        SymbolKind::Method
    } else {
        SymbolKind::Function
    };

    let signature = extract_function_signature(base, node, is_public_fn, is_export_fn);
    let visibility = if is_public || is_export {
        Visibility::Public
    } else {
        Visibility::Private
    };

    let doc_comment = base.extract_documentation(&node);

    Some(base.create_symbol(
        &node,
        name,
        symbol_kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.cloned(),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract test declarations (test "name" {...})
pub(super) fn extract_test(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
) -> Option<Symbol> {
    // Extract test name from string node
    let string_node = base.find_child_by_type(&node, "string")?;

    // Get the actual test name from string_content
    let string_content_node = base.find_child_by_type(&string_node, "string_content");
    let test_name = if let Some(content_node) = string_content_node {
        base.get_node_text(&content_node)
    } else {
        // Fallback to the full string text, removing quotes
        let full_text = base.get_node_text(&string_node);
        full_text.trim_matches('"').to_string()
    };

    let signature = format!("test \"{}\"", test_name);
    let doc_comment = base.extract_documentation(&node);

    let metadata = Some({
        let mut meta = HashMap::new();
        meta.insert("isTest".to_string(), serde_json::Value::Bool(true));
        meta
    });

    Some(base.create_symbol(
        &node,
        test_name,
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.cloned(),
            metadata,
            doc_comment,
        },
    ))
}

/// Extract parameter declarations
pub(super) fn extract_parameter(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
) -> Option<Symbol> {
    let name_node = base.find_child_by_type(&node, "identifier")?;
    let param_name = base.get_node_text(&name_node);

    let type_node = base
        .find_child_by_type(&node, "type_expression")
        .or_else(|| base.find_child_by_type(&node, "builtin_type"))
        .or_else(|| {
            // Look for identifier after colon for type
            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();
            let colon_index = children.iter().position(|child| child.kind() == ":")?;
            children.get(colon_index + 1).copied()
        });

    let param_type = if let Some(type_node) = type_node {
        base.get_node_text(&type_node)
    } else {
        "unknown".to_string()
    };

    let signature = format!("{}: {}", param_name, param_type);

    Some(base.create_symbol(
        &node,
        param_name,
        SymbolKind::Variable,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.cloned(),
            metadata: None,
            doc_comment: None,
        },
    ))
}

/// Extract full function signature including modifiers, parameters, and return type
fn extract_function_signature(
    base: &mut BaseExtractor,
    node: Node,
    is_public_fn: fn(&BaseExtractor, Node) -> bool,
    is_export_fn: fn(&BaseExtractor, Node) -> bool,
) -> String {
    let name_node = base.find_child_by_type(&node, "identifier");
    let name = if let Some(name_node) = name_node {
        base.get_node_text(&name_node)
    } else {
        "unknown".to_string()
    };

    // Check for visibility and function modifiers
    let is_public = is_public_fn(base, node);
    let is_export = is_export_fn(base, node);
    let is_inline = is_inline_function(base, node);

    let mut modifier_prefix = String::new();
    if is_public {
        modifier_prefix.push_str("pub ");
    }
    if is_export {
        modifier_prefix.push_str("export ");
    }
    if is_inline {
        modifier_prefix.push_str("inline ");
    }

    // Check for extern prefix
    let extern_node = base.find_child_by_type(&node, "extern");
    let string_node = base.find_child_by_type(&node, "string");
    let mut extern_prefix = String::new();
    if let (Some(_extern), Some(string_n)) = (extern_node, string_node) {
        let linkage = base.get_node_text(&string_n);
        extern_prefix = format!("extern {} ", linkage);
    }

    // Extract parameters
    let mut params = Vec::new();
    let param_list = base
        .find_child_by_type(&node, "parameters")
        .or_else(|| base.find_child_by_type(&node, "parameter_list"));

    if let Some(param_list) = param_list {
        let mut cursor = param_list.walk();
        for child in param_list.children(&mut cursor) {
            if child.kind() == "parameter" {
                // Handle comptime parameters
                let comptime_node = base.find_child_by_type(&child, "comptime");
                let param_name_node = base.find_child_by_type(&child, "identifier");

                // Look for type nodes
                let type_node = base
                    .find_child_by_type(&child, "type_expression")
                    .or_else(|| base.find_child_by_type(&child, "builtin_type"))
                    .or_else(|| base.find_child_by_type(&child, "pointer_type"))
                    .or_else(|| base.find_child_by_type(&child, "slice_type"))
                    .or_else(|| base.find_child_by_type(&child, "optional_type"))
                    .or_else(|| {
                        // Look for identifier after colon
                        let mut param_cursor = child.walk();
                        let param_children: Vec<Node> = child.children(&mut param_cursor).collect();
                        let colon_index = param_children.iter().position(|c| c.kind() == ":")?;
                        param_children.get(colon_index + 1).copied()
                    });

                if let Some(param_name_node) = param_name_node {
                    let param_name = base.get_node_text(&param_name_node);
                    let param_type = if let Some(type_node) = type_node {
                        base.get_node_text(&type_node)
                    } else {
                        String::new()
                    };

                    let param_str = if comptime_node.is_some() {
                        if param_type.is_empty() {
                            format!("comptime {}", param_name)
                        } else {
                            format!("comptime {}: {}", param_name, param_type)
                        }
                    } else if !param_type.is_empty() {
                        format!("{}: {}", param_name, param_type)
                    } else {
                        param_name
                    };

                    params.push(param_str);
                }
            } else if child.kind() == "variadic_parameter" || base.get_node_text(&child) == "..." {
                params.push("...".to_string());
            }
        }
    }

    // Check if the raw function text contains "..." for variadic parameters
    let full_function_text = base.get_node_text(&node);
    if full_function_text.contains("...") && !params.iter().any(|p| p == "...") {
        params.push("...".to_string());
    }

    // Extract return type
    let return_type_node = base
        .find_child_by_type(&node, "return_type")
        .or_else(|| base.find_child_by_type(&node, "type_expression"))
        .or_else(|| base.find_child_by_type(&node, "pointer_type"))
        .or_else(|| base.find_child_by_type(&node, "error_union_type"))
        .or_else(|| base.find_child_by_type(&node, "nullable_type"))
        .or_else(|| base.find_child_by_type(&node, "optional_type"))
        .or_else(|| base.find_child_by_type(&node, "slice_type"))
        .or_else(|| base.find_child_by_type(&node, "builtin_type"));

    let return_type = if let Some(return_type_node) = return_type_node {
        base.get_node_text(&return_type_node)
    } else {
        "void".to_string()
    };

    format!(
        "{}{}fn {}({}) {}",
        modifier_prefix,
        extern_prefix,
        name,
        params.join(", "),
        return_type
    )
}

/// Check if function has inline modifier
fn is_inline_function(base: &BaseExtractor, node: Node) -> bool {
    // Check for "inline" keyword in function children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "inline" || base.get_node_text(&child) == "inline" {
            return true;
        }
    }

    // Also check for "inline" keyword before function (fallback)
    if let Some(prev) = node.prev_sibling() {
        if prev.kind() == "inline" || base.get_node_text(&prev) == "inline" {
            return true;
        }
    }

    false
}
