//! Function and method extraction for C++
//! Handles extraction of functions, methods, constructors, destructors, and operators

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions};
use tree_sitter::Node;

use super::{declarations, helpers};

/// Extract function (definition or declaration)
pub(super) fn extract_function(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let mut func_node = node;
    if node.kind() == "function_definition" {
        // Look for function_declarator or reference_declarator
        let declarator = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "function_declarator" || c.kind() == "reference_declarator");
        if let Some(declarator) = declarator {
            func_node = declarator;
        }
    }

    let name_node = extract_function_name(func_node)?;
    let name = base.get_node_text(&name_node);

    // Skip if it's a field_identifier (should be handled as method)
    if name_node.kind() == "field_identifier" {
        return extract_method(base, node, func_node, &name, parent_id);
    }

    // Check if this is a constructor or destructor
    let is_constructor_flag = is_constructor(base, &name, node);
    let is_destructor = name.starts_with('~');
    let is_operator = name.starts_with("operator");

    let kind = if is_constructor_flag {
        SymbolKind::Constructor
    } else if is_destructor {
        SymbolKind::Destructor
    } else if is_operator {
        SymbolKind::Operator
    } else {
        SymbolKind::Function
    };

    // Build signature from proven approach
    let modifiers = extract_function_modifiers(base, node);
    let return_type = if is_constructor_flag || is_destructor {
        String::new()
    } else {
        extract_basic_return_type(base, node)
    };
    let trailing_return_type = extract_trailing_return_type(base, node);
    let parameters = extract_function_parameters(base, func_node);
    let const_qualifier = extract_const_qualifier(func_node);
    let noexcept_spec = extract_noexcept_specifier(base, func_node);

    let mut signature = String::new();

    // Add template parameters if present
    if let Some(template_params) = helpers::extract_template_parameters(base, node.parent()) {
        signature.push_str(&template_params);
        signature.push('\n');
    }

    // Add modifiers
    if !modifiers.is_empty() {
        signature.push_str(&modifiers.join(" "));
        signature.push(' ');
    }

    // Add return type
    if !return_type.is_empty() {
        signature.push_str(&return_type);
        signature.push(' ');
    }

    // Add function name and parameters
    signature.push_str(&name);
    signature.push_str(&parameters);

    // Add const qualifier
    if const_qualifier {
        signature.push_str(" const");
    }

    // Add noexcept
    if !noexcept_spec.is_empty() {
        signature.push(' ');
        signature.push_str(&noexcept_spec);
    }

    // Add trailing return type
    if !trailing_return_type.is_empty() {
        if trailing_return_type.starts_with("->") {
            signature.push(' ');
            signature.push_str(&trailing_return_type);
        } else {
            signature.push_str(" -> ");
            signature.push_str(&trailing_return_type);
        }
    }

    // Check for = delete, = default (for function_definition nodes)
    if node.kind() == "function_definition" {
        let children: Vec<Node> = node.children(&mut node.walk()).collect();
        for child in &children {
            if child.kind() == "delete_method_clause" {
                signature.push_str(" = delete");
                break;
            } else if child.kind() == "default_method_clause" {
                signature.push_str(" = default");
                break;
            }
        }
    }

    // Extract visibility based on access specifiers (private:/protected:/public:)
    let visibility = declarations::extract_cpp_visibility(base, node);

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        name,
        kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract method (function inside a class)
fn extract_method(
    base: &mut BaseExtractor,
    node: Node,
    func_node: Node,
    name: &str,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let is_constructor = is_constructor(base, name, node);
    let is_destructor = name.starts_with('~');
    let is_operator = name.starts_with("operator");

    let kind = if is_constructor {
        SymbolKind::Constructor
    } else if is_destructor {
        SymbolKind::Destructor
    } else if is_operator {
        SymbolKind::Operator
    } else {
        SymbolKind::Method
    };

    // For methods in classes, look for modifiers in the parent declaration node as well
    let modifiers = extract_method_modifiers(base, node, func_node);
    let return_type = if is_constructor || is_destructor {
        String::new()
    } else {
        extract_basic_return_type(base, node)
    };
    let parameters = extract_function_parameters(base, func_node);
    let const_qualifier = extract_const_qualifier(func_node);

    let mut signature = String::new();
    if !modifiers.is_empty() {
        signature.push_str(&modifiers.join(" "));
        signature.push(' ');
    }
    if !return_type.is_empty() {
        signature.push_str(&return_type);
        signature.push(' ');
    }
    signature.push_str(name);
    signature.push_str(&parameters);
    if const_qualifier {
        signature.push_str(" const");
    }

    // Extract visibility based on access specifiers (private:/protected:/public:)
    let visibility = declarations::extract_cpp_visibility(base, node);

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        name.to_string(),
        kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract function name from function declarator
pub(super) fn extract_function_name(func_node: Node) -> Option<Node> {
    // operator_name (operator overloading)
    if let Some(operator_node) = func_node
        .children(&mut func_node.walk())
        .find(|c| c.kind() == "operator_name")
    {
        return Some(operator_node);
    }

    // destructor_name
    if let Some(destructor_node) = func_node
        .children(&mut func_node.walk())
        .find(|c| c.kind() == "destructor_name")
    {
        return Some(destructor_node);
    }

    // field_identifier (methods)
    if let Some(field_id_node) = func_node
        .children(&mut func_node.walk())
        .find(|c| c.kind() == "field_identifier")
    {
        return Some(field_id_node);
    }

    // identifier (regular functions)
    if let Some(identifier_node) = func_node
        .children(&mut func_node.walk())
        .find(|c| c.kind() == "identifier")
    {
        return Some(identifier_node);
    }

    // qualified_identifier (e.g., ClassName::method)
    if let Some(qualified_node) = func_node
        .children(&mut func_node.walk())
        .find(|c| c.kind() == "qualified_identifier")
    {
        return Some(qualified_node);
    }

    None
}

/// Check if a function name matches a containing class name (is constructor)
pub(super) fn is_constructor(base: &BaseExtractor, name: &str, node: Node) -> bool {
    let mut current = Some(node);
    while let Some(parent) = current {
        if matches!(parent.kind(), "class_specifier" | "struct_specifier") {
            if let Some(class_name_node) = parent
                .children(&mut parent.walk())
                .find(|c| c.kind() == "type_identifier")
            {
                let class_name = base.get_node_text(&class_name_node);
                if class_name == name {
                    return true;
                }
            }
        }
        current = parent.parent();
    }
    false
}

/// Extract function modifiers (virtual, static, explicit, inline, etc.)
pub(super) fn extract_function_modifiers(base: &mut BaseExtractor, node: Node) -> Vec<String> {
    let mut modifiers = Vec::new();
    let modifier_types = ["virtual", "static", "explicit", "friend", "inline"];

    helpers::collect_modifiers_recursive(base, node, &mut modifiers, &modifier_types);

    modifiers
}

/// Extract method modifiers (checks multiple tree levels)
fn extract_method_modifiers(
    base: &mut BaseExtractor,
    declaration_node: Node,
    func_node: Node,
) -> Vec<String> {
    let mut modifiers = Vec::new();
    let modifier_types = [
        "virtual", "static", "explicit", "friend", "inline", "override",
    ];

    let mut nodes_to_check = vec![declaration_node, func_node];

    // Add parent nodes to check
    if let Some(parent) = declaration_node.parent() {
        nodes_to_check.push(parent);
        if let Some(grandparent) = parent.parent() {
            nodes_to_check.push(grandparent);
        }
    }

    // Check all these nodes for modifiers
    for node in nodes_to_check {
        if node.kind() == "field_declaration" || node.kind() == "declaration" {
            // Check direct children for modifier keywords
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

        // Also do recursive search within each node
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
fn extract_trailing_return_type(base: &mut BaseExtractor, node: Node) -> String {
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
fn extract_const_qualifier(func_node: Node) -> bool {
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
