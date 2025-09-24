// Port of Miller's comprehensive PowerShell extractor tests
// Following TDD pattern: RED phase - tests should compile but fail

use crate::extractors::base::{SymbolKind, RelationshipKind, Visibility};
use crate::extractors::powershell::PowerShellExtractor;
use tree_sitter::Tree;

#[cfg(test)]
mod powershell_extractor_tests {
    use super::*;
    use std::collections::HashMap;

    // Helper function to create a PowerShellExtractor and parse PowerShell code
    fn create_extractor_and_parse(code: &str) -> (PowerShellExtractor, Tree) {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_powershell::LANGUAGE.into()).unwrap();
        let tree = parser.parse(code, None).unwrap();
        let extractor = PowerShellExtractor::new("powershell".to_string(), "test.ps1".to_string(), code.to_string());
        (extractor, tree)
    }

    mod functions_and_advanced_functions {
        use super::*;

        #[test]
        fn test_extract_powershell_functions_and_advanced_functions() {
            let powershell_code = r#"
# Simple function
function Get-UserInfo {
    param($UserName)
    Write-Output "User: $UserName"
}

# Advanced function with CmdletBinding
function Get-ComputerData {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory=$true)]
        [string]$ComputerName,

        [Parameter()]
        [switch]$IncludeServices
    )

    begin {
        Write-Verbose "Starting computer data collection"
    }

    process {
        $computer = Get-WmiObject -Class Win32_ComputerSystem -ComputerName $ComputerName
        if ($IncludeServices) {
            $services = Get-Service -ComputerName $ComputerName
        }
    }

    end {
        Write-Verbose "Completed data collection"
    }
}

# Function with pipeline support
function Set-CustomProperty {
    [CmdletBinding()]
    param(
        [Parameter(ValueFromPipeline=$true)]
        [PSObject]$InputObject,

        [string]$PropertyName,
        [string]$PropertyValue
    )

    process {
        $InputObject | Add-Member -NotePropertyName $PropertyName -NotePropertyValue $PropertyValue -PassThru
    }
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(powershell_code);
            let symbols = extractor.extract_symbols(&tree);

            // Should extract functions
            let functions = symbols.iter().filter(|s| s.kind == SymbolKind::Function).collect::<Vec<_>>();
            assert!(functions.len() >= 3, "Should extract at least 3 functions");

            let get_user_info = functions.iter().find(|f| f.name == "Get-UserInfo");
            assert!(get_user_info.is_some(), "Should extract Get-UserInfo function");
            let get_user_info = get_user_info.unwrap();
            assert!(get_user_info.signature.as_ref().unwrap().contains("Get-UserInfo"));
            assert_eq!(get_user_info.visibility.as_ref().unwrap(), &Visibility::Public);

            let get_computer_data = functions.iter().find(|f| f.name == "Get-ComputerData");
            assert!(get_computer_data.is_some(), "Should extract Get-ComputerData function");
            let get_computer_data = get_computer_data.unwrap();
            assert!(get_computer_data.signature.as_ref().unwrap().contains("[CmdletBinding()]"));

            let set_custom_property = functions.iter().find(|f| f.name == "Set-CustomProperty");
            assert!(set_custom_property.is_some(), "Should extract Set-CustomProperty function");

            // Should extract parameters
            let parameters = symbols.iter().filter(|s| s.kind == SymbolKind::Variable && s.parent_id.is_some()).collect::<Vec<_>>();
            assert!(parameters.len() >= 4, "Should extract at least 4 parameters");

            let computer_name_param = parameters.iter().find(|p| p.name == "ComputerName");
            assert!(computer_name_param.is_some(), "Should extract ComputerName parameter");
            let computer_name_param = computer_name_param.unwrap();
            assert!(computer_name_param.signature.as_ref().unwrap().contains("[Parameter(Mandatory=$true)]"));
        }
    }

    mod variables_and_automatic_variables {
        use super::*;

        #[test]
        fn test_extract_powershell_variables_and_automatic_variables() {
            let powershell_code = r#"
# User-defined variables
$Global:ConfigPath = "C:\Config\app.config"
$Script:LogLevel = "Debug"
$Local:TempData = @{}

# Variables with different scopes
$env:POWERSHELL_TELEMETRY_OPTOUT = 1
$using:RemoteVariable = $LocalValue

# Complex variable assignments
$Services = Get-Service | Where-Object { $_.Status -eq 'Running' }
$HashTable = @{
    Name = "Test"
    Value = 42
    Active = $true
}

# Array and string manipulation
$Array = @("Item1", "Item2", "Item3")
$ComputerName = $env:COMPUTERNAME
$ProcessList = Get-Process -Name "powershell*"

# Automatic variables usage
Write-Host "PowerShell version: $($PSVersionTable.PSVersion)"
Write-Host "Current location: $PWD"
Write-Host "Last exit code: $LASTEXITCODE"
"#;

            let (mut extractor, tree) = create_extractor_and_parse(powershell_code);
            let symbols = extractor.extract_symbols(&tree);

            // Should extract user-defined variables
            let variables = symbols.iter().filter(|s| s.kind == SymbolKind::Variable).collect::<Vec<_>>();
            assert!(variables.len() >= 6, "Should extract at least 6 variables");

            let config_path = variables.iter().find(|v| v.name == "ConfigPath");
            assert!(config_path.is_some(), "Should extract ConfigPath variable");
            let config_path = config_path.unwrap();
            assert!(config_path.signature.as_ref().unwrap().contains("$Global:ConfigPath"));
            assert_eq!(config_path.visibility.as_ref().unwrap(), &Visibility::Public); // Global scope

            let log_level = variables.iter().find(|v| v.name == "LogLevel");
            assert!(log_level.is_some(), "Should extract LogLevel variable");
            let log_level = log_level.unwrap();
            assert!(log_level.signature.as_ref().unwrap().contains("$Script:LogLevel"));

            // Should extract environment variables
            let env_vars = variables.iter().filter(|v|
                v.name.contains("env:") ||
                (v.signature.is_some() && v.signature.as_ref().unwrap().contains("$env:"))
            ).collect::<Vec<_>>();
            assert!(env_vars.len() >= 1, "Should extract at least 1 environment variable");

            // Should extract automatic variables
            let auto_vars = variables.iter().filter(|v|
                ["PSVersionTable", "PWD", "LASTEXITCODE", "COMPUTERNAME"].contains(&v.name.as_str())
            ).collect::<Vec<_>>();
            assert!(auto_vars.len() >= 2, "Should extract at least 2 automatic variables");
        }
    }

    mod classes_and_methods {
        use super::*;

        #[test]
        fn test_extract_powershell_classes_and_methods() {
            let powershell_code = r#"
# PowerShell class definition
class ComputerInfo {
    [string]$Name
    [string]$OS
    [datetime]$LastBoot
    hidden [string]$InternalId

    # Constructor
    ComputerInfo([string]$computerName) {
        $this.Name = $computerName
        $this.OS = (Get-WmiObject Win32_OperatingSystem).Caption
        $this.LastBoot = (Get-WmiObject Win32_OperatingSystem).LastBootUpTime
        $this.InternalId = [System.Guid]::NewGuid().ToString()
    }

    # Instance method
    [string] GetUptime() {
        $uptime = (Get-Date) - $this.LastBoot
        return "$($uptime.Days) days, $($uptime.Hours) hours"
    }

    # Static method
    static [ComputerInfo] GetLocalComputer() {
        return [ComputerInfo]::new($env:COMPUTERNAME)
    }

    # Method with parameters
    [void] UpdateOS([string]$newOS) {
        $this.OS = $newOS
        Write-Verbose "OS updated to: $newOS"
    }
}

# Enum definition
enum LogLevel {
    Error = 1
    Warning = 2
    Information = 3
    Debug = 4
}

# Class inheritance
class ServerInfo : ComputerInfo {
    [string]$Role
    [int]$Port

    ServerInfo([string]$name, [string]$role, [int]$port) : base($name) {
        $this.Role = $role
        $this.Port = $port
    }

    [string] GetServiceInfo() {
        return "Server: $($this.Name), Role: $($this.Role), Port: $($this.Port)"
    }
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(powershell_code);
            let symbols = extractor.extract_symbols(&tree);

            // Should extract classes
            let classes = symbols.iter().filter(|s| s.kind == SymbolKind::Class).collect::<Vec<_>>();
            assert!(classes.len() >= 2, "Should extract at least 2 classes");

            let computer_info = classes.iter().find(|c| c.name == "ComputerInfo");
            assert!(computer_info.is_some(), "Should extract ComputerInfo class");
            let computer_info = computer_info.unwrap();
            assert_eq!(computer_info.visibility.as_ref().unwrap(), &Visibility::Public);

            let server_info = classes.iter().find(|c| c.name == "ServerInfo");
            assert!(server_info.is_some(), "Should extract ServerInfo class");

            // Should extract methods
            let methods = symbols.iter().filter(|s| s.kind == SymbolKind::Method).collect::<Vec<_>>();
            assert!(methods.len() >= 4, "Should extract at least 4 methods");

            let get_uptime = methods.iter().find(|m| m.name == "GetUptime");
            assert!(get_uptime.is_some(), "Should extract GetUptime method");
            let get_uptime = get_uptime.unwrap();
            assert!(get_uptime.signature.as_ref().unwrap().contains("[string] GetUptime()"));

            let get_local_computer = methods.iter().find(|m| m.name == "GetLocalComputer");
            assert!(get_local_computer.is_some(), "Should extract GetLocalComputer method");
            let get_local_computer = get_local_computer.unwrap();
            assert!(get_local_computer.signature.as_ref().unwrap().contains("static"));

            // Should extract properties
            let properties = symbols.iter().filter(|s| s.kind == SymbolKind::Property).collect::<Vec<_>>();
            assert!(properties.len() >= 5, "Should extract at least 5 properties");

            let name_property = properties.iter().find(|p| p.name == "Name");
            assert!(name_property.is_some(), "Should extract Name property");
            let name_property = name_property.unwrap();
            assert!(name_property.signature.as_ref().unwrap().contains("[string]$Name"));

            let hidden_property = properties.iter().find(|p| p.name == "InternalId");
            assert!(hidden_property.is_some(), "Should extract InternalId property");
            let hidden_property = hidden_property.unwrap();
            assert_eq!(hidden_property.visibility.as_ref().unwrap(), &Visibility::Private); // hidden

            // Should extract enums
            let enums = symbols.iter().filter(|s| s.kind == SymbolKind::Enum).collect::<Vec<_>>();
            assert!(enums.len() >= 1, "Should extract at least 1 enum");

            let log_level = enums.iter().find(|e| e.name == "LogLevel");
            assert!(log_level.is_some(), "Should extract LogLevel enum");
        }
    }

    mod azure_and_windows_devops_commands {
        use super::*;

        #[test]
        fn test_extract_azure_and_windows_devops_commands() {
            let powershell_code = r#"
# Azure PowerShell commands
function Deploy-AzureResources {
    param($ResourceGroupName, $SubscriptionId)

    # Azure authentication and context
    Connect-AzAccount -SubscriptionId $SubscriptionId
    Set-AzContext -SubscriptionId $SubscriptionId

    # Resource deployment
    New-AzResourceGroup -Name $ResourceGroupName -Location "East US"
    New-AzResourceGroupDeployment -ResourceGroupName $ResourceGroupName -TemplateFile "template.json"

    # Azure Container Instances
    New-AzContainerGroup -ResourceGroupName $ResourceGroupName -Name "myapp-container"

    # Azure Kubernetes Service
    New-AzAksCluster -ResourceGroupName $ResourceGroupName -Name "myapp-aks"
    Get-AzAksCluster | kubectl config use-context
}

# Windows Server management
function Configure-WindowsServer {
    # Windows Features
    Enable-WindowsOptionalFeature -Online -FeatureName IIS-WebServerRole
    Install-WindowsFeature -Name Web-Server -IncludeManagementTools

    # Registry operations
    Set-ItemProperty -Path "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion" -Name "CustomSetting" -Value "Configured"

    # Service management
    Set-Service -Name "W3SVC" -StartupType Automatic
    Start-Service -Name "W3SVC"

    # File and folder operations
    New-Item -Path "C:\inetpub\wwwroot\api" -ItemType Directory -Force
    Copy-Item -Path "app\*" -Destination "C:\inetpub\wwwroot\api" -Recurse

    # PowerShell DSC
    Configuration WebServerConfig {
        Node "localhost" {
            WindowsFeature IIS {
                Ensure = "Present"
                Name = "Web-Server"
            }
        }
    }
}

# DevOps pipeline commands
function Run-DeploymentPipeline {
    # Docker operations
    docker build -t myapp:latest .
    docker push myregistry.azurecr.io/myapp:latest

    # Kubernetes deployments
    kubectl apply -f k8s/deployment.yaml
    kubectl rollout status deployment/myapp

    # Azure CLI operations
    az login --service-principal --username $env:AZURE_CLIENT_ID --password $env:AZURE_CLIENT_SECRET --tenant $env:AZURE_TENANT_ID
    az aks get-credentials --resource-group $ResourceGroupName --name $ClusterName

    # PowerShell remoting
    Invoke-Command -ComputerName $ServerList -ScriptBlock {
        Get-Service | Where-Object { $_.Status -eq 'Stopped' }
    }
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(powershell_code);
            let symbols = extractor.extract_symbols(&tree);

            // Should extract Azure commands
            let azure_commands = symbols.iter().filter(|s|
                s.kind == SymbolKind::Function &&
                (s.name.starts_with("Connect-Az") || s.name.starts_with("New-Az") || s.name.starts_with("Set-Az"))
            ).collect::<Vec<_>>();
            assert!(azure_commands.len() >= 4, "Should extract at least 4 Azure commands");

            let connect_az = azure_commands.iter().find(|c| c.name == "Connect-AzAccount");
            assert!(connect_az.is_some(), "Should extract Connect-AzAccount command");
            let connect_az = connect_az.unwrap();
            assert!(connect_az.doc_comment.as_ref().unwrap().contains("[Azure CLI Call]"));

            // Should extract Windows management commands
            let windows_commands = symbols.iter().filter(|s|
                s.kind == SymbolKind::Function &&
                (s.name.contains("Windows") || s.name.contains("Service") || s.name.contains("Registry"))
            ).collect::<Vec<_>>();
            assert!(windows_commands.len() >= 3, "Should extract at least 3 Windows commands");

            // Should extract cross-platform DevOps commands
            let devops_commands = symbols.iter().filter(|s|
                s.kind == SymbolKind::Function &&
                ["docker", "kubectl", "az"].contains(&s.name.as_str())
            ).collect::<Vec<_>>();
            assert!(devops_commands.len() >= 3, "Should extract at least 3 DevOps commands");

            let docker_cmd = devops_commands.iter().find(|c| c.name == "docker");
            assert!(docker_cmd.is_some(), "Should extract docker command");
            let docker_cmd = docker_cmd.unwrap();
            assert!(docker_cmd.doc_comment.as_ref().unwrap().contains("[Docker Container Call]"));

            let kubectl_cmd = devops_commands.iter().find(|c| c.name == "kubectl");
            assert!(kubectl_cmd.is_some(), "Should extract kubectl command");
            let kubectl_cmd = kubectl_cmd.unwrap();
            assert!(kubectl_cmd.doc_comment.as_ref().unwrap().contains("[Kubernetes CLI Call]"));
        }
    }

    mod modules_and_imports {
        use super::*;

        #[test]
        fn test_extract_powershell_modules_and_imports() {
            let powershell_code = r#"
# Module imports
Import-Module Az.Accounts
Import-Module Az.Resources -Force
Import-Module -Name "Custom.Tools" -RequiredVersion "2.1.0"

# Dot sourcing
. "$PSScriptRoot\CommonFunctions.ps1"
. "C:\Scripts\HelperFunctions.ps1"

# Using statements (PowerShell 5.0+)
using namespace System.Collections.Generic
using module Az.Storage

# Module manifest variables
$ModuleManifestData = @{
    RootModule = 'MyModule.psm1'
    ModuleVersion = '1.0.0'
    GUID = [System.Guid]::NewGuid()
    Author = 'DevOps Team'
    CompanyName = 'MyCompany'
    PowerShellVersion = '5.1'
    RequiredModules = @('Az.Accounts', 'Az.Resources')
}

# Export module members
Export-ModuleMember -Function Get-CustomData
Export-ModuleMember -Variable ConfigSettings
Export-ModuleMember -Alias gcd
"#;

            let (mut extractor, tree) = create_extractor_and_parse(powershell_code);
            let symbols = extractor.extract_symbols(&tree);

            // Should extract import statements
            let imports = symbols.iter().filter(|s| s.kind == SymbolKind::Import).collect::<Vec<_>>();
            assert!(imports.len() >= 4, "Should extract at least 4 import statements");

            let az_accounts = imports.iter().find(|i| i.name == "Az.Accounts");
            assert!(az_accounts.is_some(), "Should extract Az.Accounts import");
            let az_accounts = az_accounts.unwrap();
            assert!(az_accounts.signature.as_ref().unwrap().contains("Import-Module Az.Accounts"));

            let custom_tools = imports.iter().find(|i| i.name == "Custom.Tools");
            assert!(custom_tools.is_some(), "Should extract Custom.Tools import");
            let custom_tools = custom_tools.unwrap();
            assert!(custom_tools.signature.as_ref().unwrap().contains("RequiredVersion \"2.1.0\""));

            // Should extract using statements
            let using_statements = imports.iter().filter(|i|
                i.signature.as_ref().map_or(false, |s| s.contains("using"))
            ).collect::<Vec<_>>();
            assert!(using_statements.len() >= 2, "Should extract at least 2 using statements");

            // Should extract dot sourcing
            let dot_sourcing = imports.iter().filter(|i|
                i.signature.as_ref().map_or(false, |s| s.contains(". "))
            ).collect::<Vec<_>>();
            assert!(dot_sourcing.len() >= 2, "Should extract at least 2 dot sourcing statements");

            // Should extract export statements
            let exports = symbols.iter().filter(|s| s.kind == SymbolKind::Export).collect::<Vec<_>>();
            assert!(exports.len() >= 3, "Should extract at least 3 export statements");
        }
    }

    mod error_handling_and_edge_cases {
        use super::*;

        #[test]
        fn test_handle_malformed_powershell_gracefully() {
            let malformed_powershell = r#"
# Incomplete function
function Incomplete-Function {
    param($Parameter
    # Missing closing brace and parameter definition

# Incomplete class
class Broken-Class {
    [string]$Property
    # Missing closing brace

# Invalid syntax
if ($condition -eq {
    Write-Output "incomplete if statement"

# But should still extract what it can
function Working-Function {
    param([string]$Name)
    Write-Output "Hello, $Name"
}

$ValidVariable = "This should work"
"#;

            let (mut extractor, tree) = create_extractor_and_parse(malformed_powershell);

            // Should not panic or throw errors
            let symbols = extractor.extract_symbols(&tree);
            let relationships = extractor.extract_relationships(&tree, &symbols);

            // Should still extract valid symbols
            let valid_function = symbols.iter().find(|s| s.name == "Working-Function");
            assert!(valid_function.is_some(), "Should extract Working-Function even with malformed code");

            let valid_variable = symbols.iter().find(|s| s.name == "ValidVariable");
            assert!(valid_variable.is_some(), "Should extract ValidVariable even with malformed code");
        }

        #[test]
        fn test_handle_empty_files_gracefully() {
            let empty_powershell = "";
            let minimal_powershell = "# Just a comment\n";

            let (mut empty_extractor, empty_tree) = create_extractor_and_parse(empty_powershell);
            let (mut minimal_extractor, minimal_tree) = create_extractor_and_parse(minimal_powershell);

            let empty_symbols = empty_extractor.extract_symbols(&empty_tree);
            let minimal_symbols = minimal_extractor.extract_symbols(&minimal_tree);

            // Should handle gracefully without errors
            assert_eq!(empty_symbols.len(), 0, "Empty file should produce no symbols");
            assert_eq!(minimal_symbols.len(), 0, "Comment-only file should produce no symbols");
        }
    }
}