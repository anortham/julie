use crate::base::BaseExtractor;
use tree_sitter::Node;

use super::helpers;

/// Extract function modifiers (virtual, static, explicit, inline, etc.)
pub(super) fn extract_function_modifiers(base: &mut BaseExtractor, node: Node) -> Vec<String> {
    let mut modifiers = Vec::new();
    let modifier_types = ["virtual", "static", "explicit", "friend", "inline"];

    helpers::collect_modifiers_recursive(base, node, &mut modifiers, &modifier_types);

    modifiers
}

/// Extract method modifiers (checks multiple tree levels)
pub(super) fn extract_method_modifiers(
    base: &mut BaseExtractor,
    declaration_node: Node,
    func_node: Node,
) -> Vec<String> {
    let mut modifiers = Vec::new();
    let modifier_types = [
        "virtual", "static", "explicit", "friend", "inline", "override",
    ];

    let mut nodes_to_check = vec![declaration_node, func_node];

    if let Some(parent) = declaration_node.parent() {
        nodes_to_check.push(parent);
        if let Some(grandparent) = parent.parent() {
            nodes_to_check.push(grandparent);
        }
    }

    for node in nodes_to_check {
        if node.kind() == "field_declaration" || node.kind() == "declaration" {
            for child in node.children(&mut node.walk()) {
                if modifier_types.contains(&child.kind()) {
                    let modifier = base.get_node_text(&child);
                    if !modifiers.contains(&modifier) {
                        modifiers.push(modifier);
                    }
                } else if child.kind() == "storage_class_specifier" {
                    let text = base.get_node_text(&child);
                    if modifier_types.contains(&text.as_str()) && !modifiers.contains(&text) {
                        modifiers.push(text);
                    }
                }
            }
        }

        helpers::collect_modifiers_recursive(base, node, &mut modifiers, &modifier_types);
    }

    modifiers
}

/// Extract return type from function node
pub(super) fn extract_basic_return_type(base: &mut BaseExtractor, node: Node) -> String {
    for child in node.children(&mut node.walk()) {
        if matches!(
            child.kind(),
            "primitive_type"
                | "type_identifier"
                | "qualified_identifier"
                | "auto"
                | "placeholder_type_specifier"
        ) {
            return base.get_node_text(&child);
        }
    }
    String::new()
}

/// Extract trailing return type (for auto return type deduction)
pub(super) fn extract_trailing_return_type(base: &mut BaseExtractor, node: Node) -> String {
    let func_declarator = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "function_declarator");

    if let Some(declarator) = func_declarator {
        let children: Vec<Node> = declarator.children(&mut declarator.walk()).collect();

        for (i, child) in children.iter().enumerate() {
            if child.kind() == "->" && i + 1 < children.len() {
                return base.get_node_text(&children[i + 1]);
            } else if child.kind() == "trailing_return_type" {
                return child
                    .children(&mut child.walk())
                    .find(|c| {
                        matches!(
                            c.kind(),
                            "primitive_type" | "type_identifier" | "qualified_identifier"
                        )
                    })
                    .map(|type_node| base.get_node_text(&type_node))
                    .unwrap_or_else(|| base.get_node_text(child));
            }
        }
    }

    String::new()
}

/// Extract function parameters as string
pub(super) fn extract_function_parameters(base: &mut BaseExtractor, func_node: Node) -> String {
    if let Some(param_list) = func_node
        .children(&mut func_node.walk())
        .find(|c| c.kind() == "parameter_list")
    {
        base.get_node_text(&param_list)
    } else {
        "()".to_string()
    }
}

/// Check if function has const qualifier
pub(super) fn extract_const_qualifier(func_node: Node) -> bool {
    func_node
        .children(&mut func_node.walk())
        .any(|c| c.kind() == "type_qualifier")
}

/// Extract noexcept specifier
pub(super) fn extract_noexcept_specifier(base: &mut BaseExtractor, func_node: Node) -> String {
    for child in func_node.children(&mut func_node.walk()) {
        if child.kind() == "noexcept" {
            return base.get_node_text(&child);
        }
    }
    String::new()
}
