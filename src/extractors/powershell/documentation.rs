//! Documentation and annotation generation for PowerShell symbols
//! Handles variable classifications, command documentation, and variable annotations

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
