// Port of Miller's comprehensive PowerShell extractor tests
// Following TDD pattern: RED phase - tests should compile but fail

use crate::extractors::base::{SymbolKind, Visibility};
use crate::extractors::powershell::PowerShellExtractor;
use tree_sitter::Tree;

#[cfg(test)]
mod powershell_extractor_tests {
    use super::*;

    // Helper function to create a PowerShellExtractor and parse PowerShell code
    fn create_extractor_and_parse(code: &str) -> (PowerShellExtractor, Tree) {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_powershell::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(code, None).unwrap();
        let extractor = PowerShellExtractor::new(
            "powershell".to_string(),
            "test.ps1".to_string(),
            code.to_string(),
        );
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
            let functions = symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::Function)
                .collect::<Vec<_>>();
            assert!(functions.len() >= 3, "Should extract at least 3 functions");

            let get_user_info = functions.iter().find(|f| f.name == "Get-UserInfo");
            assert!(
                get_user_info.is_some(),
                "Should extract Get-UserInfo function"
            );
            let get_user_info = get_user_info.unwrap();
            assert!(get_user_info
                .signature
                .as_ref()
                .unwrap()
                .contains("Get-UserInfo"));
            assert_eq!(
                get_user_info.visibility.as_ref().unwrap(),
                &Visibility::Public
            );

            let get_computer_data = functions.iter().find(|f| f.name == "Get-ComputerData");
            assert!(
                get_computer_data.is_some(),
                "Should extract Get-ComputerData function"
            );
            let get_computer_data = get_computer_data.unwrap();
            assert!(get_computer_data
                .signature
                .as_ref()
                .unwrap()
                .contains("[CmdletBinding()]"));

            let set_custom_property = functions.iter().find(|f| f.name == "Set-CustomProperty");
            assert!(
                set_custom_property.is_some(),
                "Should extract Set-CustomProperty function"
            );

            // Should extract parameters
            let parameters = symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::Variable && s.parent_id.is_some())
                .collect::<Vec<_>>();
            assert!(
                parameters.len() >= 4,
                "Should extract at least 4 parameters"
            );

            let computer_name_param = parameters.iter().find(|p| p.name == "ComputerName");
            assert!(
                computer_name_param.is_some(),
                "Should extract ComputerName parameter"
            );
            let computer_name_param = computer_name_param.unwrap();
            assert!(computer_name_param
                .signature
                .as_ref()
                .unwrap()
                .contains("[Parameter(Mandatory=$true)]"));
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
            let variables = symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::Variable)
                .collect::<Vec<_>>();
            assert!(variables.len() >= 6, "Should extract at least 6 variables");

            let config_path = variables.iter().find(|v| v.name == "ConfigPath");
            assert!(config_path.is_some(), "Should extract ConfigPath variable");
            let config_path = config_path.unwrap();
            assert!(config_path
                .signature
                .as_ref()
                .unwrap()
                .contains("$Global:ConfigPath"));
            assert_eq!(
                config_path.visibility.as_ref().unwrap(),
                &Visibility::Public
            ); // Global scope

            let log_level = variables.iter().find(|v| v.name == "LogLevel");
            assert!(log_level.is_some(), "Should extract LogLevel variable");
            let log_level = log_level.unwrap();
            assert!(log_level
                .signature
                .as_ref()
                .unwrap()
                .contains("$Script:LogLevel"));

            // Should extract environment variables
            let env_vars = variables
                .iter()
                .filter(|v| {
                    v.name.contains("env:")
                        || (v.signature.is_some()
                            && v.signature.as_ref().unwrap().contains("$env:"))
                })
                .collect::<Vec<_>>();
            assert!(
                env_vars.len() >= 1,
                "Should extract at least 1 environment variable"
            );

            // Should extract automatic variables
            let auto_vars = variables
                .iter()
                .filter(|v| {
                    ["PSVersionTable", "PWD", "LASTEXITCODE", "COMPUTERNAME"]
                        .contains(&v.name.as_str())
                })
                .collect::<Vec<_>>();
            assert!(
                auto_vars.len() >= 2,
                "Should extract at least 2 automatic variables"
            );
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
            let classes = symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::Class)
                .collect::<Vec<_>>();
            assert!(classes.len() >= 2, "Should extract at least 2 classes");

            let computer_info = classes.iter().find(|c| c.name == "ComputerInfo");
            assert!(computer_info.is_some(), "Should extract ComputerInfo class");
            let computer_info = computer_info.unwrap();
            assert_eq!(
                computer_info.visibility.as_ref().unwrap(),
                &Visibility::Public
            );

            let server_info = classes.iter().find(|c| c.name == "ServerInfo");
            assert!(server_info.is_some(), "Should extract ServerInfo class");

            // Should extract methods
            let methods = symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::Method)
                .collect::<Vec<_>>();
            assert!(methods.len() >= 4, "Should extract at least 4 methods");

            let get_uptime = methods.iter().find(|m| m.name == "GetUptime");
            assert!(get_uptime.is_some(), "Should extract GetUptime method");
            let get_uptime = get_uptime.unwrap();
            assert!(get_uptime
                .signature
                .as_ref()
                .unwrap()
                .contains("[string] GetUptime()"));

            let get_local_computer = methods.iter().find(|m| m.name == "GetLocalComputer");
            assert!(
                get_local_computer.is_some(),
                "Should extract GetLocalComputer method"
            );
            let get_local_computer = get_local_computer.unwrap();
            assert!(get_local_computer
                .signature
                .as_ref()
                .unwrap()
                .contains("static"));

            // Should extract properties
            let properties = symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::Property)
                .collect::<Vec<_>>();
            assert!(
                properties.len() >= 5,
                "Should extract at least 5 properties"
            );

            let name_property = properties.iter().find(|p| p.name == "Name");
            assert!(name_property.is_some(), "Should extract Name property");
            let name_property = name_property.unwrap();
            assert!(name_property
                .signature
                .as_ref()
                .unwrap()
                .contains("[string]$Name"));

            let hidden_property = properties.iter().find(|p| p.name == "InternalId");
            assert!(
                hidden_property.is_some(),
                "Should extract InternalId property"
            );
            let hidden_property = hidden_property.unwrap();
            assert_eq!(
                hidden_property.visibility.as_ref().unwrap(),
                &Visibility::Private
            ); // hidden

            // Should extract enums
            let enums = symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::Enum)
                .collect::<Vec<_>>();
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
"#;

            let (mut extractor, tree) = create_extractor_and_parse(powershell_code);
            let symbols = extractor.extract_symbols(&tree);

            // Test DSC configuration
            let web_server_config = symbols.iter().find(|s| s.name == "WebServerConfig");
            assert!(web_server_config.is_some());
            assert_eq!(web_server_config.unwrap().kind, SymbolKind::Function);

            // Test Azure functions
            let deploy_resources = symbols.iter().find(|s| s.name == "Deploy-AzureResources");
            assert!(deploy_resources.is_some());

            let configure_server = symbols.iter().find(|s| s.name == "Configure-WindowsServer");
            assert!(configure_server.is_some());
        }
    }

    mod error_handling_and_exception_management {
        use super::*;

        #[test]
        fn test_extract_powershell_error_handling_and_try_catch() {
            let powershell_code = r###"
# Try-Catch-Finally blocks
function Test-ErrorHandling {
    try {
        $result = Get-Content "nonexistent.txt"
        Write-Output "File content: $result"
    }
    catch [System.IO.FileNotFoundException] {
        Write-Warning "File not found: $($_.Exception.Message)"
        return $null
    }
    catch {
        Write-Error "Unexpected error: $($_.Exception.Message)"
        throw
    }
    finally {
        Write-Verbose "Cleanup operations completed"
    }
}

# Error action preferences
function Set-ErrorPreferences {
    $ErrorActionPreference = "Stop"
    $WarningPreference = "Continue"
    $VerbosePreference = "SilentlyContinue"
    $DebugPreference = "Continue"
}

# Trap statements
trap {
    Write-Host "Trapped error: $($_.Exception.Message)" -ForegroundColor Red
    continue
}

function Process-DataWithTrap {
    param($Data)

    if ($Data -eq $null) {
        throw "Data cannot be null"
    }

    return "Processed: $Data"
}

# Custom error records
function New-CustomError {
    param(
        [string]$Message,
        [string]$ErrorId = "CustomError",
        [System.Management.Automation.ErrorCategory]$Category = "InvalidOperation"
    )

    $errorRecord = [System.Management.Automation.ErrorRecord]::new(
        [System.InvalidOperationException]::new($Message),
        $ErrorId,
        $Category,
        $null
    )

    throw $errorRecord
}

# Error handling with WhatIf and Confirm
function Remove-ItemSafely {
    [CmdletBinding(SupportsShouldProcess = $true, ConfirmImpact = 'High')]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if ($PSCmdlet.ShouldProcess($Path, "Remove item")) {
        try {
            Remove-Item $Path -ErrorAction Stop
            Write-Output "Successfully removed: $Path"
        }
        catch {
            Write-Error "Failed to remove $Path: $($_.Exception.Message)"
            return $false
        }
    }

    return $true
}
"###;

            let (mut extractor, tree) = create_extractor_and_parse(powershell_code);
            let symbols = extractor.extract_symbols(&tree);

            // Test error handling functions
            let test_error_handling = symbols.iter().find(|s| s.name == "Test-ErrorHandling");
            assert!(test_error_handling.is_some());
            assert_eq!(test_error_handling.unwrap().kind, SymbolKind::Function);

            let set_error_preferences = symbols.iter().find(|s| s.name == "Set-ErrorPreferences");
            assert!(set_error_preferences.is_some());

            let process_data_with_trap = symbols.iter().find(|s| s.name == "Process-DataWithTrap");
            assert!(process_data_with_trap.is_some());

            let new_custom_error = symbols.iter().find(|s| s.name == "New-CustomError");
            assert!(new_custom_error.is_some());

            let remove_item_safely = symbols.iter().find(|s| s.name == "Remove-ItemSafely");
            assert!(remove_item_safely.is_some());
        }
    }

    mod workflows_and_parallel_processing {
        use super::*;

        #[test]
        fn test_extract_powershell_workflows_and_parallel_features() {
            let powershell_code = r###"
# PowerShell Workflow
workflow Test-ParallelProcessing {
    param(
        [string[]]$ComputerNames,
        [int]$ThrottleLimit = 4
    )

    # Parallel execution
    foreach -parallel ($computer in $ComputerNames) {
        $result = Test-Connection -ComputerName $computer -Count 1 -Quiet
        Write-Output "$computer is $(if ($result) { 'online' } else { 'offline' })"
    }

    # Sequence block
    sequence {
        Write-Output "Step 1: Initializing"
        Start-Sleep -Seconds 1
        Write-Output "Step 2: Processing"
        Start-Sleep -Seconds 1
        Write-Output "Step 3: Finalizing"
    }

    # InlineScript for non-workflow commands
    $results = InlineScript {
        $data = Get-Process | Where-Object { $_.CPU -gt 10 }
        return $data
    }

    return $results
}

# Parallel processing with ForEach-Object
function Process-ItemsInParallel {
    param([int[]]$Numbers)

    $Numbers | ForEach-Object -Parallel {
        $square = $_ * $_
        $cube = $_ * $_ * $_
        return [PSCustomObject]@{
            Number = $_
            Square = $square
            Cube = $cube
        }
    } -ThrottleLimit 4
}

# Workflow with checkpoints
workflow Long-RunningProcess {
    param([string]$ProcessName)

    # Checkpoint for resumability
    Checkpoint-Workflow

    Write-Output "Starting long process: $ProcessName"
    Start-Sleep -Seconds 5

    Checkpoint-Workflow

    Write-Output "Process $ProcessName completed"
    return $true
}

# Job management
function Start-BackgroundJobs {
    param([string[]]$Commands)

    $jobs = @()
    foreach ($command in $Commands) {
        $job = Start-Job -ScriptBlock ([scriptblock]::Create($command))
        $jobs += $job
    }

    # Wait for all jobs to complete
    $results = $jobs | Wait-Job | Receive-Job

    # Clean up jobs
    $jobs | Remove-Job

    return $results
}

# Runspace pools for parallel execution
function Invoke-ParallelOperations {
    param(
        [scriptblock[]]$Operations,
        [int]$MaxThreads = 4
    )

    $runspacePool = [runspacefactory]::CreateRunspacePool(1, $MaxThreads)
    $runspacePool.Open()

    $runspaces = @()
    foreach ($operation in $Operations) {
        $powershell = [powershell]::Create()
        $powershell.RunspacePool = $runspacePool
        $powershell.AddScript($operation)

        $runspaces += @{
            PowerShell = $powershell
            Handle = $powershell.BeginInvoke()
        }
    }

    # Collect results
    $results = @()
    foreach ($runspace in $runspaces) {
        $results += $runspace.PowerShell.EndInvoke($runspace.Handle)
        $runspace.PowerShell.Dispose()
    }

    $runspacePool.Close()
    return $results
}
"###;

            let (mut extractor, tree) = create_extractor_and_parse(powershell_code);
            let symbols = extractor.extract_symbols(&tree);

            // Test workflow functions
            let test_parallel_processing =
                symbols.iter().find(|s| s.name == "Test-ParallelProcessing");
            assert!(test_parallel_processing.is_some());
            assert_eq!(test_parallel_processing.unwrap().kind, SymbolKind::Function);

            let process_items_in_parallel =
                symbols.iter().find(|s| s.name == "Process-ItemsInParallel");
            assert!(process_items_in_parallel.is_some());

            let long_running_process = symbols.iter().find(|s| s.name == "Long-RunningProcess");
            assert!(long_running_process.is_some());

            let start_background_jobs = symbols.iter().find(|s| s.name == "Start-BackgroundJobs");
            assert!(start_background_jobs.is_some());

            let invoke_parallel_operations = symbols
                .iter()
                .find(|s| s.name == "Invoke-ParallelOperations");
            assert!(invoke_parallel_operations.is_some());
        }
    }

    mod dsc_and_configuration_management {
        use super::*;

        #[test]
        fn test_extract_powershell_dsc_configurations_and_resources() {
            let powershell_code = r###"
# DSC Configuration
Configuration MyWebServer {
    param(
        [string[]]$ComputerName = 'localhost'
    )

    Import-DscResource -ModuleName PSDesiredStateConfiguration
    Import-DscResource -ModuleName xWebAdministration

    Node $ComputerName {
        # Windows features
        WindowsFeature IIS {
            Ensure = 'Present'
            Name = 'Web-Server'
        }

        WindowsFeature IISManagement {
            Ensure = 'Present'
            Name = 'Web-Mgmt-Tools'
        }

        # File resource
        File WebsiteContent {
            Ensure = 'Present'
            SourcePath = '\\server\share\website'
            DestinationPath = 'C:\inetpub\wwwroot'
            Recurse = $true
            DependsOn = '[WindowsFeature]IIS'
        }

        # Registry resource
        Registry DisableFirewall {
            Ensure = 'Present'
            Key = 'HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Services\SharedAccess\Parameters\FirewallPolicy\StandardProfile'
            ValueName = 'EnableFirewall'
            ValueData = 0
            ValueType = 'Dword'
        }

        # Service resource
        Service W3SVC {
            Name = 'W3SVC'
            StartupType = 'Automatic'
            State = 'Running'
            DependsOn = '[WindowsFeature]IIS'
        }

        # Custom DSC resource
        xWebsite DefaultSite {
            Ensure = 'Present'
            Name = 'Default Web Site'
            PhysicalPath = 'C:\inetpub\wwwroot'
            State = 'Started'
            DependsOn = '[File]WebsiteContent'
        }
    }
}

# Apply configuration
MyWebServer -ComputerName 'WEBSERVER01'

# Test configuration
Test-DscConfiguration -ComputerName 'WEBSERVER01'

# Get configuration status
Get-DscConfigurationStatus -ComputerName 'WEBSERVER01'

# Custom DSC resource function
function Get-TargetResource {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,

        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $currentState = @{
        Name = $Name
        Path = $Path
        Ensure = 'Absent'
    }

    if (Test-Path $Path) {
        $currentState.Ensure = 'Present'
    }

    return $currentState
}

function Set-TargetResource {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,

        [Parameter(Mandatory = $true)]
        [string]$Path,

        [ValidateSet('Present', 'Absent')]
        [string]$Ensure = 'Present'
    )

    if ($Ensure -eq 'Present') {
        if (-not (Test-Path $Path)) {
            New-Item -ItemType Directory -Path $Path -Force
        }
    }
    else {
        if (Test-Path $Path) {
            Remove-Item -Path $Path -Recurse -Force
        }
    }
}

function Test-TargetResource {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,

        [Parameter(Mandatory = $true)]
        [string]$Path,

        [ValidateSet('Present', 'Absent')]
        [string]$Ensure = 'Present'
    )

    $currentState = Get-TargetResource -Name $Name -Path $Path
    return $currentState.Ensure -eq $Ensure
}
"###;

            let (mut extractor, tree) = create_extractor_and_parse(powershell_code);
            let symbols = extractor.extract_symbols(&tree);

            // Test DSC configuration
            let my_web_server = symbols.iter().find(|s| s.name == "MyWebServer");
            assert!(my_web_server.is_some());
            assert_eq!(my_web_server.unwrap().kind, SymbolKind::Function);

            // Test DSC resource functions
            let get_target_resource = symbols.iter().find(|s| s.name == "Get-TargetResource");
            assert!(get_target_resource.is_some());

            let set_target_resource = symbols.iter().find(|s| s.name == "Set-TargetResource");
            assert!(set_target_resource.is_some());

            let test_target_resource = symbols.iter().find(|s| s.name == "Test-TargetResource");
            assert!(test_target_resource.is_some());
        }

        #[test]
        fn test_extract_powershell_devops_pipeline_commands() {
            let powershell_code = r#"
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
        Get-Service | Where-Object { $_.Status -eq "Stopped" }
    }
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(powershell_code);
            let symbols = extractor.extract_symbols(&tree);

            // Should extract the main deployment function
            let deployment_func = symbols.iter().find(|s| s.name == "Run-DeploymentPipeline");
            assert!(
                deployment_func.is_some(),
                "Should extract Run-DeploymentPipeline function"
            );
            assert_eq!(deployment_func.unwrap().kind, SymbolKind::Function);

            // Should extract DevOps tool calls
            let devops_commands = symbols
                .iter()
                .filter(|s| {
                    s.kind == SymbolKind::Function
                        && ["docker", "kubectl", "az", "Invoke-Command"].contains(&s.name.as_str())
                })
                .collect::<Vec<_>>();
            assert!(
                devops_commands.len() >= 4,
                "Should extract at least 4 DevOps commands"
            );

            let docker_cmd = devops_commands.iter().find(|c| c.name == "docker");
            assert!(docker_cmd.is_some(), "Should extract docker command");

            let kubectl_cmd = devops_commands.iter().find(|c| c.name == "kubectl");
            assert!(kubectl_cmd.is_some(), "Should extract kubectl command");

            let az_cmd = devops_commands.iter().find(|c| c.name == "az");
            assert!(az_cmd.is_some(), "Should extract az command");

            let invoke_cmd = devops_commands.iter().find(|c| c.name == "Invoke-Command");
            assert!(invoke_cmd.is_some(), "Should extract Invoke-Command");
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
. "$PSScriptRoot\\CommonFunctions.ps1"
. "C:\\Scripts\\HelperFunctions.ps1"

# Using statements (PowerShell 5.0+)
using namespace System.Collections.Generic
using module Az.Storage

# Module manifest variables
$ModuleManifestData = @{
    RootModule = "MyModule.psm1"
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
            let imports = symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::Import)
                .collect::<Vec<_>>();
            assert!(
                imports.len() >= 4,
                "Should extract at least 4 import statements"
            );

            let az_accounts = imports.iter().find(|i| i.name == "Az.Accounts");
            assert!(az_accounts.is_some(), "Should extract Az.Accounts import");
            let az_accounts = az_accounts.unwrap();
            assert!(az_accounts
                .signature
                .as_ref()
                .unwrap()
                .contains("Import-Module Az.Accounts"));

            let custom_tools = imports.iter().find(|i| i.name == "Custom.Tools");
            assert!(custom_tools.is_some(), "Should extract Custom.Tools import");
            let custom_tools = custom_tools.unwrap();
            assert!(custom_tools
                .signature
                .as_ref()
                .unwrap()
                .contains("RequiredVersion \"2.1.0\""));

            // Should extract using statements
            let using_statements = imports
                .iter()
                .filter(|i| i.signature.as_ref().map_or(false, |s| s.contains("using")))
                .collect::<Vec<_>>();
            assert!(
                using_statements.len() >= 2,
                "Should extract at least 2 using statements"
            );

            // Should extract dot sourcing
            let dot_sourcing = imports
                .iter()
                .filter(|i| i.signature.as_ref().map_or(false, |s| s.contains(". ")))
                .collect::<Vec<_>>();
            assert!(
                dot_sourcing.len() >= 2,
                "Should extract at least 2 dot sourcing statements"
            );

            // Should extract export statements
            let exports = symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::Export)
                .collect::<Vec<_>>();
            assert!(
                exports.len() >= 3,
                "Should extract at least 3 export statements"
            );
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
            let _relationships = extractor.extract_relationships(&tree, &symbols);

            // Should still extract valid symbols
            let valid_function = symbols.iter().find(|s| s.name == "Working-Function");
            assert!(
                valid_function.is_some(),
                "Should extract Working-Function even with malformed code"
            );

            let valid_variable = symbols.iter().find(|s| s.name == "ValidVariable");
            assert!(
                valid_variable.is_some(),
                "Should extract ValidVariable even with malformed code"
            );
        }

        #[test]
        fn test_handle_empty_files_gracefully() {
            let empty_powershell = "";
            let minimal_powershell = "# Just a comment\n";

            let (mut empty_extractor, empty_tree) = create_extractor_and_parse(empty_powershell);
            let (mut minimal_extractor, minimal_tree) =
                create_extractor_and_parse(minimal_powershell);

            let empty_symbols = empty_extractor.extract_symbols(&empty_tree);
            let minimal_symbols = minimal_extractor.extract_symbols(&minimal_tree);

            // Should handle gracefully without errors
            assert_eq!(
                empty_symbols.len(),
                0,
                "Empty file should produce no symbols"
            );
            assert_eq!(
                minimal_symbols.len(),
                0,
                "Comment-only file should produce no symbols"
            );
        }
    }

    // PowerShell Identifier Extraction Tests (TDD RED phase)
    //
    // These tests validate the extract_identifiers() functionality which extracts:
    // - Function/cmdlet calls (command_expression, invocation_expression)
    // - Member access (member_access_expression)
    // - Proper containing symbol tracking (file-scoped)
    //
    // Following the Rust/C# extractor reference implementation pattern
    mod identifier_extraction {
        use super::*;
        use crate::extractors::base::IdentifierKind;

        #[test]
        fn test_powershell_function_calls() {
            let powershell_code = r#"
function Get-UserData {
    param([string]$UserName)
    return "User: $UserName"
}

function Process-Data {
    $result = Get-UserData -UserName "John"  # Function call
    Write-Host $result                        # Cmdlet call
    Get-Process | Where-Object { $_.Status -eq 'Running' }  # Cmdlet calls
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(powershell_code);
            let symbols = extractor.extract_symbols(&tree);

            // NOW extract identifiers (this will FAIL until we implement it)
            let identifiers = extractor.extract_identifiers(&tree, &symbols);

            // Verify we found the function calls
            let get_user_data_call = identifiers
                .iter()
                .find(|id| id.name == "Get-UserData" && id.kind == IdentifierKind::Call);
            assert!(
                get_user_data_call.is_some(),
                "Should extract 'Get-UserData' function call identifier"
            );

            let write_host_call = identifiers
                .iter()
                .find(|id| id.name == "Write-Host" && id.kind == IdentifierKind::Call);
            assert!(
                write_host_call.is_some(),
                "Should extract 'Write-Host' cmdlet call identifier"
            );

            let get_process_call = identifiers
                .iter()
                .find(|id| id.name == "Get-Process" && id.kind == IdentifierKind::Call);
            assert!(
                get_process_call.is_some(),
                "Should extract 'Get-Process' cmdlet call identifier"
            );

            // Verify containing symbol is set
            assert!(
                get_user_data_call.unwrap().containing_symbol_id.is_some(),
                "Function call should have containing symbol"
            );
        }

        #[test]
        fn test_powershell_member_access() {
            let powershell_code = r#"
class User {
    [string]$Name
    [string]$Email

    [void] PrintInfo() {
        Write-Host $this.Name    # Member access: this.Name
        $email = $this.Email      # Member access: this.Email
    }
}

function Get-SystemInfo {
    $process = Get-Process
    $name = $process.Name        # Member access: process.Name
    $id = $process.Id            # Member access: process.Id
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(powershell_code);
            let symbols = extractor.extract_symbols(&tree);
            let identifiers = extractor.extract_identifiers(&tree, &symbols);

            // Verify we found member access identifiers
            let name_access_count = identifiers
                .iter()
                .filter(|id| id.name == "Name" && id.kind == IdentifierKind::MemberAccess)
                .count();
            assert!(
                name_access_count >= 1,
                "Should extract 'Name' member access identifier. Found {} identifiers total",
                identifiers.len()
            );

            let email_access = identifiers
                .iter()
                .find(|id| id.name == "Email" && id.kind == IdentifierKind::MemberAccess);
            assert!(
                email_access.is_some(),
                "Should extract 'Email' member access identifier"
            );

            let id_access = identifiers
                .iter()
                .find(|id| id.name == "Id" && id.kind == IdentifierKind::MemberAccess);
            assert!(
                id_access.is_some(),
                "Should extract 'Id' member access identifier"
            );
        }

        #[test]
        fn test_powershell_identifiers_have_containing_symbol() {
            // This test ensures we ONLY match symbols from the SAME FILE
            // Critical bug fix from Rust implementation
            let powershell_code = r#"
function Get-Data {
    return @{ Value = 42 }
}

function Process-All {
    $data = Get-Data          # Call to Get-Data in same file
    Format-Output $data       # Call to Format-Output
}

function Format-Output {
    param($InputData)
    Write-Host $InputData
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(powershell_code);
            let symbols = extractor.extract_symbols(&tree);
            let identifiers = extractor.extract_identifiers(&tree, &symbols);

            // Find the Get-Data call
            let get_data_call = identifiers
                .iter()
                .find(|id| id.name == "Get-Data" && id.kind == IdentifierKind::Call);
            assert!(get_data_call.is_some());
            let get_data_call = get_data_call.unwrap();

            // Verify it has a containing symbol (the Process-All function)
            assert!(
                get_data_call.containing_symbol_id.is_some(),
                "Get-Data call should have containing symbol from same file"
            );

            // Verify the containing symbol is the Process-All function
            let process_all_function = symbols.iter().find(|s| s.name == "Process-All");
            assert!(process_all_function.is_some());
            let process_all_function = process_all_function.unwrap();

            assert_eq!(
                get_data_call.containing_symbol_id.as_ref(),
                Some(&process_all_function.id),
                "Get-Data call should be contained within Process-All function"
            );
        }

        #[test]
        fn test_powershell_chained_member_access() {
            let powershell_code = r#"
function Get-ProcessInfo {
    $processes = Get-Process
    $name = $processes[0].MainModule.FileName     # Chained member access
    $version = $processes[0].MainModule.FileVersionInfo.ProductVersion  # Deep chaining
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(powershell_code);
            let symbols = extractor.extract_symbols(&tree);
            let identifiers = extractor.extract_identifiers(&tree, &symbols);

            // Should extract the rightmost identifiers in chains
            let filename_access = identifiers
                .iter()
                .find(|id| id.name == "FileName" && id.kind == IdentifierKind::MemberAccess);
            assert!(
                filename_access.is_some(),
                "Should extract 'FileName' from chained member access"
            );

            let product_version_access = identifiers
                .iter()
                .find(|id| id.name == "ProductVersion" && id.kind == IdentifierKind::MemberAccess);
            assert!(
                product_version_access.is_some(),
                "Should extract 'ProductVersion' from deeply chained member access"
            );
        }

        #[test]
        fn test_powershell_duplicate_calls_at_different_locations() {
            let powershell_code = r#"
function Test-Process {
    Get-Process
    Start-Sleep -Seconds 1
    Get-Process    # Same call twice at different locations
}
"#;

            let (mut extractor, tree) = create_extractor_and_parse(powershell_code);
            let symbols = extractor.extract_symbols(&tree);
            let identifiers = extractor.extract_identifiers(&tree, &symbols);

            // Should extract BOTH calls (they're at different locations)
            let process_calls: Vec<_> = identifiers
                .iter()
                .filter(|id| id.name == "Get-Process" && id.kind == IdentifierKind::Call)
                .collect();

            assert_eq!(
                process_calls.len(),
                2,
                "Should extract both Get-Process calls at different locations"
            );

            // Verify they have different line numbers
            assert_ne!(
                process_calls[0].start_line, process_calls[1].start_line,
                "Duplicate calls should have different line numbers"
            );
        }
    }
}
