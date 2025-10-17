//! PowerShell command and cmdlet extraction
//! Focuses on Azure, Windows, and cross-platform DevOps commands

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use regex::Regex;
use tree_sitter::Node;

use super::documentation::get_command_documentation;
use super::helpers::find_command_name_node;

/// Extract command and cmdlet symbols (Azure, Windows, DevOps focused)
pub(super) fn extract_command(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Check for dot sourcing first (special case with different AST structure)
    let mut cursor = node.walk();
    let dot_source_node = node.children(&mut cursor).find(|child| {
        child.kind() == "command_invokation_operator" && base.get_node_text(child) == "."
    });

    if dot_source_node.is_some() {
        return None; // Handled separately in imports module
    }

    let command_name_node = find_command_name_node(node)?;
    let command_name = base.get_node_text(&command_name_node);

    // Check for DSC Configuration command (handled separately)
    if command_name == "Configuration" {
        return None; // Handled by extract_dsc_configuration
    }

    // Check for import/module commands first (handled in imports module)
    let import_commands = ["Import-Module", "Export-ModuleMember", "using"];
    if import_commands.contains(&command_name.as_str()) {
        return None; // Handled in imports module
    }

    // Focus on Azure, Windows, and cross-platform DevOps commands
    let devops_commands = [
        // Azure PowerShell
        "Connect-AzAccount",
        "Set-AzContext",
        "New-AzResourceGroup",
        "New-AzResourceGroupDeployment",
        "New-AzContainerGroup",
        "New-AzAksCluster",
        "Get-AzAksCluster",
        // Windows Management
        "Enable-WindowsOptionalFeature",
        "Install-WindowsFeature",
        "Set-ItemProperty",
        "Set-Service",
        "Start-Service",
        "New-Item",
        "Copy-Item",
        // Cross-platform DevOps
        "docker",
        "kubectl",
        "az",
        // PowerShell Core
        "Invoke-Command",
    ];

    let is_interesting = devops_commands.contains(&command_name.as_str())
        || command_name.starts_with("Connect-")
        || command_name.starts_with("New-")
        || command_name.starts_with("Set-")
        || command_name.starts_with("Get-");

    if is_interesting {
        let signature = extract_command_signature(base, node);
        let doc_comment = get_command_documentation(&command_name);

        Some(base.create_symbol(
            &node,
            command_name,
            SymbolKind::Function,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: None,
                doc_comment: Some(doc_comment),
            },
        ))
    } else {
        None
    }
}

/// Extract DSC Configuration command
pub(super) fn extract_dsc_configuration(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // For DSC Configuration commands, extract the configuration name from command arguments
    let mut cursor = node.walk();
    let command_elements: Vec<_> = node
        .children(&mut cursor)
        .filter(|child| child.kind() == "command_elements")
        .collect();

    if command_elements.is_empty() {
        return None;
    }

    // Look for the configuration name in the command elements
    let mut elements_cursor = command_elements[0].walk();
    for element in command_elements[0].children(&mut elements_cursor) {
        if element.kind() == "command_argument_sep" {
            // Skip separators
            continue;
        }
        if element.kind() == "generic_token" || element.kind() == "command_name" {
            let token_text = base.get_node_text(&element);
            // Skip "Configuration" keyword and look for the name
            if token_text != "Configuration" && !token_text.trim().is_empty() {
                let name = token_text.trim().to_string();
                let signature = format!("Configuration {}", name);
                let doc_comment = Some("PowerShell DSC Configuration".to_string());

                return Some(base.create_symbol(
                    &node,
                    name,
                    SymbolKind::Function,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(Visibility::Public),
                        parent_id: parent_id.map(|s| s.to_string()),
                        metadata: None,
                        doc_comment,
                    },
                ));
            }
        }
    }

    None
}

/// Extract command signature
fn extract_command_signature(base: &BaseExtractor, node: Node) -> String {
    let command_text = base.get_node_text(&node);
    // Safely truncate UTF-8 string at character boundary
    BaseExtractor::truncate_string(&command_text, 97)
}

/// Extract configuration name from ERROR node containing DSC configuration
pub(super) fn extract_configuration_from_error(
    _base: &BaseExtractor,
    node_text: &str,
) -> Option<(String, String)> {
    // Extract configuration name from text like "Configuration MyWebServer {"
    if let Some(config_match) = Regex::new(r"Configuration\s+([A-Za-z][A-Za-z0-9-_]*)")
        .unwrap()
        .captures(node_text)
    {
        let name = config_match.get(1).unwrap().as_str().to_string();
        let signature = format!("Configuration {}", name);
        return Some((name, signature));
    }

    None
}

/// Extract function name from ERROR node containing function
pub(super) fn extract_function_from_error(
    _base: &BaseExtractor,
    node_text: &str,
) -> Option<(String, String)> {
    // Extract function name from text like "function MyFunction {"
    if let Some(func_match) = Regex::new(r"function\s+([A-Za-z][A-Za-z0-9-_]*)")
        .unwrap()
        .captures(node_text)
    {
        let name = func_match.get(1).unwrap().as_str().to_string();
        let signature = format!("function {}()", name);
        return Some((name, signature));
    }

    None
}
