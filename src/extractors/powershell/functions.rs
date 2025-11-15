//! PowerShell function extraction
//! Handles simple functions, advanced functions with [CmdletBinding()], and parameters

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use regex::Regex;
use std::sync::LazyLock;
use tree_sitter::Node;

use super::documentation;
use super::helpers::{
    extract_function_name_from_param_block, extract_parameter_attributes, find_function_name_node,
    find_nodes_by_type, find_parameter_name_node, has_attribute, has_parameter_attribute,
};

static FUNCTION_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"function\s+([A-Za-z][A-Za-z0-9-_]*)").unwrap());

/// Extract function symbols (simple functions)
pub(super) fn extract_function(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = find_function_name_node(node)?;
    let name = base.get_node_text(&name_node);

    // Check if it's an advanced function with [CmdletBinding()]
    let _is_advanced = has_attribute(base, node, "CmdletBinding");

    let signature = extract_function_signature(base, node);

    // Extract doc comment (PowerShell comment-based help)
    let doc_comment = documentation::extract_powershell_doc_comment(base, &node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public), // PowerShell functions are generally public
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract advanced function symbols (from param_block nodes)
pub(super) fn extract_advanced_function(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // For param_block nodes (advanced functions), extract function name from ERROR node content
    let function_name = extract_function_name_from_param_block(base, node, &FUNCTION_NAME_RE)?;

    // Check for CmdletBinding attribute
    let _has_cmdlet_binding = has_attribute(base, node, "CmdletBinding");

    let signature = extract_advanced_function_signature(base, node, &function_name);

    // Extract doc comment (PowerShell comment-based help)
    let doc_comment = documentation::extract_powershell_doc_comment(base, &node);

    Some(base.create_symbol(
        &node,
        function_name,
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract function parameters from a function node
pub(super) fn extract_function_parameters(
    base: &mut BaseExtractor,
    func_node: Node,
    parent_id: &str,
) -> Vec<Symbol> {
    let mut parameters = Vec::new();

    // Handle simple functions - look for param_block with parameter_definition
    let param_blocks = find_nodes_by_type(func_node, "param_block");
    for param_block in param_blocks {
        let param_defs = find_nodes_by_type(param_block, "parameter_definition");

        for param_def in param_defs {
            if let Some(name_node) = find_parameter_name_node(param_def) {
                let param_name = base.get_node_text(&name_node).replace("$", "");
                let is_mandatory = has_parameter_attribute(base, param_def, "Mandatory");

                let signature = extract_parameter_signature(base, param_def);
                let doc_comment = if is_mandatory {
                    Some("Mandatory parameter".to_string())
                } else {
                    Some("Optional parameter".to_string())
                };

                let param_symbol = base.create_symbol(
                    &param_def,
                    param_name,
                    SymbolKind::Variable,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(Visibility::Public),
                        parent_id: Some(parent_id.to_string()),
                        metadata: None,
                        doc_comment,
                    },
                );

                parameters.push(param_symbol);
            }
        }
    }

    // Handle advanced functions - look for parameter_list with script_parameter
    let param_lists = find_nodes_by_type(func_node, "parameter_list");
    for param_list in param_lists {
        let script_params = find_nodes_by_type(param_list, "script_parameter");

        for script_param in script_params {
            // Find the direct child variable node (the parameter name), not any variable in the subtree
            let mut cursor = script_param.walk();
            let children: Vec<_> = script_param.children(&mut cursor).collect();
            if let Some(variable_node) = children
                .into_iter()
                .find(|child| child.kind() == "variable")
            {
                let param_name = base.get_node_text(&variable_node).replace("$", "");
                let is_mandatory = has_parameter_attribute(base, script_param, "Mandatory");

                let signature = extract_script_parameter_signature(base, script_param);
                let doc_comment = if is_mandatory {
                    Some("Mandatory parameter".to_string())
                } else {
                    Some("Optional parameter".to_string())
                };

                let param_symbol = base.create_symbol(
                    &script_param,
                    param_name,
                    SymbolKind::Variable,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(Visibility::Public),
                        parent_id: Some(parent_id.to_string()),
                        metadata: None,
                        doc_comment,
                    },
                );

                parameters.push(param_symbol);
            }
        }
    }

    parameters
}

/// Extract function signature
fn extract_function_signature(base: &BaseExtractor, node: Node) -> String {
    let name = find_function_name_node(node)
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "unknown".to_string());

    let has_attributes = has_attribute(base, node, "CmdletBinding");
    let prefix = if has_attributes {
        "[CmdletBinding()] "
    } else {
        ""
    };

    format!("{}function {}()", prefix, name)
}

/// Extract advanced function signature
fn extract_advanced_function_signature(
    base: &BaseExtractor,
    node: Node,
    function_name: &str,
) -> String {
    let has_cmdlet_binding = has_attribute(base, node, "CmdletBinding");
    let has_output_type = has_attribute(base, node, "OutputType");

    let mut signature = String::new();
    if has_cmdlet_binding {
        signature.push_str("[CmdletBinding()] ");
    }
    if has_output_type {
        signature.push_str("[OutputType([void])] ");
    }
    signature.push_str(&format!("function {}()", function_name));

    signature
}

/// Extract parameter signature
fn extract_parameter_signature(base: &BaseExtractor, node: Node) -> String {
    let name = find_parameter_name_node(node)
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "unknown".to_string());

    let attributes = extract_parameter_attributes(base, node);
    if !attributes.is_empty() {
        format!("{} {}", attributes, name)
    } else {
        name
    }
}

/// Extract script parameter signature
fn extract_script_parameter_signature(base: &BaseExtractor, node: Node) -> String {
    // Extract variable name
    let name = find_nodes_by_type(node, "variable")
        .first()
        .map(|n| base.get_node_text(n))
        .unwrap_or_else(|| "$unknown".to_string());

    // Extract type and attributes from attribute_list
    let attribute_list = find_nodes_by_type(node, "attribute_list");
    if attribute_list.is_empty() {
        return name;
    }

    let mut attributes = Vec::new();
    let attribute_nodes = find_nodes_by_type(attribute_list[0], "attribute");

    for attr in attribute_nodes {
        let attr_text = base.get_node_text(&attr);

        // Collect Parameter attributes and type brackets (like [string], [switch])
        if attr_text.contains("Parameter") || super::types::is_type_bracket(&attr_text) {
            attributes.push(attr_text);
        }
    }

    if !attributes.is_empty() {
        format!("{} {}", attributes.join(" "), name)
    } else {
        name
    }
}
