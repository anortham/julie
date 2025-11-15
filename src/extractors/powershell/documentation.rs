//! Documentation and annotation generation for PowerShell symbols
//! Handles variable classifications, command documentation, variable annotations, and doc comment extraction

use crate::extractors::base::BaseExtractor;
use tree_sitter::Node;

/// Classify and document environment variables
pub(super) fn is_environment_variable(name: &str) -> bool {
    let env_vars = [
        "PATH",
        "COMPUTERNAME",
        "USERNAME",
        "TEMP",
        "TMP",
        "USERPROFILE",
        "AZURE_CLIENT_ID",
        "AZURE_CLIENT_SECRET",
        "AZURE_TENANT_ID",
        "POWERSHELL_TELEMETRY_OPTOUT",
    ];
    env_vars.contains(&name)
        || regex::Regex::new(r"^[A-Z_][A-Z0-9_]*$")
            .unwrap()
            .is_match(name)
}

/// Classify and document automatic variables (PowerShell built-ins)
pub(super) fn is_automatic_variable(name: &str) -> bool {
    let auto_vars = [
        "PSVersionTable",
        "PWD",
        "LASTEXITCODE",
        "Error",
        "Host",
        "Profile",
        "PSScriptRoot",
        "PSCommandPath",
        "MyInvocation",
        "Args",
        "Input",
    ];
    auto_vars.contains(&name)
}

/// Generate variable documentation based on classification
pub(super) fn get_variable_documentation(
    is_environment: bool,
    is_automatic: bool,
    is_global: bool,
    is_script: bool,
) -> String {
    let mut annotations = Vec::new();

    if is_environment {
        annotations.push("Environment Variable");
    }
    if is_automatic {
        annotations.push("Automatic Variable");
    }
    if is_global {
        annotations.push("Global Scope");
    }
    if is_script {
        annotations.push("Script Scope");
    }

    if !annotations.is_empty() {
        format!("[{}]", annotations.join(", "))
    } else {
        String::new()
    }
}

/// Generate documentation for PowerShell commands (Azure, Windows, DevOps)
pub(super) fn get_command_documentation(command_name: &str) -> String {
    let command_docs = [
        ("Connect-AzAccount", "[Azure CLI Call]"),
        ("Set-AzContext", "[Azure Context Management]"),
        ("New-AzResourceGroup", "[Azure Resource Management]"),
        ("New-AzResourceGroupDeployment", "[Azure Deployment]"),
        ("docker", "[Docker Container Call]"),
        ("kubectl", "[Kubernetes CLI Call]"),
        ("az", "[Azure CLI Call]"),
        ("Import-Module", "[PowerShell Module Import]"),
        ("Export-ModuleMember", "[PowerShell Module Export]"),
        ("Invoke-Command", "[PowerShell Remoting]"),
    ];

    // Check direct match first
    for (cmd, doc) in &command_docs {
        if command_name == *cmd {
            return doc.to_string();
        }
    }

    // Pattern matching for commands
    if command_name.starts_with("Connect-Az") {
        return "[Azure CLI Call]".to_string();
    }
    if command_name.starts_with("New-Az") {
        return "[Azure Resource Creation]".to_string();
    }
    if command_name.starts_with("Set-Az") {
        return "[Azure Configuration]".to_string();
    }
    if command_name.starts_with("Get-Az") {
        return "[Azure Information Retrieval]".to_string();
    }
    if command_name.contains("WindowsFeature") {
        return "[Windows Feature Management]".to_string();
    }
    if command_name.contains("Service") {
        return "[Windows Service Management]".to_string();
    }

    "[PowerShell Command]".to_string()
}

/// Extract PowerShell doc comments (comment-based help)
/// Handles both block comments <# #> and single-line # comments
pub(super) fn extract_powershell_doc_comment(base: &BaseExtractor, node: &Node) -> Option<String> {
    let mut comments = Vec::new();

    // First try to find comments as direct siblings of this node
    let mut current = node.prev_named_sibling();
    while let Some(sibling) = current {
        if sibling.kind() == "comment" {
            let comment_text = base.get_node_text(&sibling);
            // PowerShell comments start with # or <#
            if comment_text.trim_start().starts_with("#")
                || comment_text.trim_start().starts_with("<#")
            {
                comments.push(comment_text);
                current = sibling.prev_named_sibling();
            } else {
                break;
            }
        } else {
            break;
        }
    }

    // If no comments found as direct siblings, try looking at ancestor siblings
    if comments.is_empty() {
        let mut current_node = *node;
        for _ in 0..3 {
            if let Some(parent) = current_node.parent() {
                current = parent.prev_named_sibling();
                while let Some(sibling) = current {
                    if sibling.kind() == "comment" {
                        let comment_text = base.get_node_text(&sibling);
                        if comment_text.trim_start().starts_with("#")
                            || comment_text.trim_start().starts_with("<#")
                        {
                            comments.push(comment_text);
                            current = sibling.prev_named_sibling();
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                if !comments.is_empty() {
                    break;
                }
                current_node = parent;
            } else {
                break;
            }
        }
    }

    if comments.is_empty() {
        None
    } else {
        // Reverse to get original order (top to bottom)
        comments.reverse();
        Some(comments.join("\n"))
    }
}
