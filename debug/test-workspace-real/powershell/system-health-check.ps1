<#
.SYNOPSIS
    System Health Check Script for Windows Administration

.DESCRIPTION
    Comprehensive system health monitoring script that checks various system components
    including disk space, memory usage, running services, and network connectivity.

.PARAMETER LogPath
    Path where the health check log will be stored

.PARAMETER ThresholdDiskSpace
    Minimum free disk space percentage before triggering warning

.PARAMETER EmailReport
    Switch to send email report of health check results

.EXAMPLE
    .\system-health-check.ps1 -LogPath "C:\Logs" -ThresholdDiskSpace 10 -EmailReport
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory=$false)]
    [string]$LogPath = "C:\HealthCheck",

    [Parameter(Mandatory=$false)]
    [int]$ThresholdDiskSpace = 15,

    [Parameter(Mandatory=$false)]
    [switch]$EmailReport
)

# Global variables
$Script:HealthResults = @()
$Script:ErrorCount = 0
$Script:WarningCount = 0

# Function to write to log and console
function Write-HealthLog {
    param(
        [Parameter(Mandatory=$true)]
        [string]$Message,

        [Parameter(Mandatory=$false)]
        [ValidateSet("Info", "Warning", "Error")]
        [string]$Level = "Info"
    )

    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $logEntry = "[$timestamp] [$Level] $Message"

    # Output to console with colors
    switch ($Level) {
        "Warning" { Write-Host $logEntry -ForegroundColor Yellow }
        "Error"   { Write-Host $logEntry -ForegroundColor Red }
        default   { Write-Host $logEntry -ForegroundColor Green }
    }

    # Write to log file
    try {
        $logEntry | Out-File -FilePath "$LogPath\HealthCheck-$(Get-Date -Format 'yyyyMMdd').log" -Append -Encoding UTF8
    }
    catch {
        Write-Warning "Failed to write to log file: $($_.Exception.Message)"
    }
}

# Function to check disk space
function Test-DiskSpace {
    Write-HealthLog "Checking disk space..."

    try {
        $disks = Get-WmiObject -Class Win32_LogicalDisk -Filter "DriveType=3" |
                 Select-Object DeviceID, Size, FreeSpace, @{
                     Name="FreeSpacePercent"
                     Expression={[math]::Round(($_.FreeSpace / $_.Size) * 100, 2)}
                 }

        foreach ($disk in $disks) {
            $freeSpaceGB = [math]::Round($disk.FreeSpace / 1GB, 2)
            $totalSpaceGB = [math]::Round($disk.Size / 1GB, 2)

            if ($disk.FreeSpacePercent -lt $ThresholdDiskSpace) {
                Write-HealthLog "WARNING: Drive $($disk.DeviceID) has only $($disk.FreeSpacePercent)% free space ($freeSpaceGB GB of $totalSpaceGB GB)" -Level "Warning"
                $Script:WarningCount++
            }
            else {
                Write-HealthLog "Drive $($disk.DeviceID): $($disk.FreeSpacePercent)% free ($freeSpaceGB GB of $totalSpaceGB GB)" -Level "Info"
            }
        }

        return $disks
    }
    catch {
        Write-HealthLog "Error checking disk space: $($_.Exception.Message)" -Level "Error"
        $Script:ErrorCount++
        return $null
    }
}

# Function to check memory usage
function Test-MemoryUsage {
    Write-HealthLog "Checking memory usage..."

    try {
        $memory = Get-WmiObject -Class Win32_PhysicalMemory |
                  Measure-Object -Property Capacity -Sum

        $totalMemoryGB = [math]::Round($memory.Sum / 1GB, 2)

        $availableMemory = Get-WmiObject -Class Win32_PerfRawData_PerfOS_Memory |
                          Select-Object -ExpandProperty AvailableBytes

        $availableMemoryGB = [math]::Round($availableMemory / 1GB, 2)
        $usedMemoryPercent = [math]::Round((($totalMemoryGB - $availableMemoryGB) / $totalMemoryGB) * 100, 2)

        if ($usedMemoryPercent -gt 90) {
            Write-HealthLog "WARNING: Memory usage is at $usedMemoryPercent% ($availableMemoryGB GB available of $totalMemoryGB GB total)" -Level "Warning"
            $Script:WarningCount++
        }
        else {
            Write-HealthLog "Memory usage: $usedMemoryPercent% ($availableMemoryGB GB available of $totalMemoryGB GB total)" -Level "Info"
        }

        return @{
            TotalGB = $totalMemoryGB
            AvailableGB = $availableMemoryGB
            UsedPercent = $usedMemoryPercent
        }
    }
    catch {
        Write-HealthLog "Error checking memory usage: $($_.Exception.Message)" -Level "Error"
        $Script:ErrorCount++
        return $null
    }
}

# Function to check critical services
function Test-CriticalServices {
    Write-HealthLog "Checking critical services..."

    # Define critical services to monitor
    $criticalServices = @(
        "Spooler",
        "Themes",
        "AudioSrv",
        "BITS",
        "EventLog"
    )

    $serviceResults = @()

    foreach ($serviceName in $criticalServices) {
        try {
            $service = Get-Service -Name $serviceName -ErrorAction Stop

            if ($service.Status -ne "Running") {
                Write-HealthLog "ERROR: Critical service '$serviceName' is $($service.Status)" -Level "Error"
                $Script:ErrorCount++
            }
            else {
                Write-HealthLog "Service '$serviceName' is running normally" -Level "Info"
            }

            $serviceResults += [PSCustomObject]@{
                Name = $serviceName
                Status = $service.Status
                StartType = $service.StartType
            }
        }
        catch {
            Write-HealthLog "Error checking service '$serviceName': $($_.Exception.Message)" -Level "Error"
            $Script:ErrorCount++
        }
    }

    return $serviceResults
}

# Function to test network connectivity
function Test-NetworkConnectivity {
    Write-HealthLog "Testing network connectivity..."

    $testHosts = @(
        "8.8.8.8",      # Google DNS
        "1.1.1.1",      # Cloudflare DNS
        "microsoft.com" # Microsoft
    )

    $networkResults = @()

    foreach ($host in $testHosts) {
        try {
            $pingResult = Test-Connection -ComputerName $host -Count 1 -Quiet -ErrorAction Stop

            if ($pingResult) {
                Write-HealthLog "Network connectivity to $host: OK" -Level "Info"
            }
            else {
                Write-HealthLog "WARNING: Cannot reach $host" -Level "Warning"
                $Script:WarningCount++
            }

            $networkResults += [PSCustomObject]@{
                Host = $host
                Reachable = $pingResult
            }
        }
        catch {
            Write-HealthLog "Error testing connectivity to $host: $($_.Exception.Message)" -Level "Error"
            $Script:ErrorCount++
        }
    }

    return $networkResults
}

# Function to generate summary report
function New-HealthReport {
    param(
        [hashtable]$Results
    )

    Write-HealthLog "Generating health check summary..."

    $summary = @"
========================================
SYSTEM HEALTH CHECK SUMMARY
========================================
Timestamp: $(Get-Date)
Computer: $env:COMPUTERNAME
User: $env:USERNAME

RESULTS SUMMARY:
- Errors: $Script:ErrorCount
- Warnings: $Script:WarningCount
- Status: $(if ($Script:ErrorCount -eq 0 -and $Script:WarningCount -eq 0) { "HEALTHY" } elseif ($Script:ErrorCount -eq 0) { "WARNING" } else { "CRITICAL" })

COMPONENT STATUS:
- Disk Space: $(if ($Results.DiskSpace) { "Checked" } else { "Failed" })
- Memory Usage: $(if ($Results.Memory) { "Checked" } else { "Failed" })
- Critical Services: $(if ($Results.Services) { "Checked" } else { "Failed" })
- Network Connectivity: $(if ($Results.Network) { "Checked" } else { "Failed" })
========================================
"@

    Write-HealthLog $summary

    # Save summary to file
    try {
        $summary | Out-File -FilePath "$LogPath\HealthSummary-$(Get-Date -Format 'yyyyMMdd-HHmmss').txt" -Encoding UTF8
    }
    catch {
        Write-HealthLog "Failed to save summary report: $($_.Exception.Message)" -Level "Warning"
    }

    return $summary
}

# Main execution function
function Invoke-HealthCheck {
    Write-HealthLog "Starting system health check on $env:COMPUTERNAME"

    # Create log directory if it doesn't exist
    if (!(Test-Path -Path $LogPath)) {
        try {
            New-Item -ItemType Directory -Path $LogPath -Force | Out-Null
            Write-HealthLog "Created log directory: $LogPath"
        }
        catch {
            Write-HealthLog "Failed to create log directory: $($_.Exception.Message)" -Level "Error"
            return
        }
    }

    # Initialize results hashtable
    $results = @{}

    # Execute health checks
    $results.DiskSpace = Test-DiskSpace
    $results.Memory = Test-MemoryUsage
    $results.Services = Test-CriticalServices
    $results.Network = Test-NetworkConnectivity

    # Generate summary report
    $summary = New-HealthReport -Results $results

    # Email report if requested
    if ($EmailReport) {
        Write-HealthLog "Email reporting requested but not implemented in this example" -Level "Warning"
    }

    Write-HealthLog "Health check completed. Errors: $Script:ErrorCount, Warnings: $Script:WarningCount"

    return $results
}

# Script execution starts here
try {
    $healthResults = Invoke-HealthCheck

    # Exit with appropriate code
    if ($Script:ErrorCount -gt 0) {
        exit 2  # Critical errors found
    }
    elseif ($Script:WarningCount -gt 0) {
        exit 1  # Warnings found
    }
    else {
        exit 0  # All healthy
    }
}
catch {
    Write-HealthLog "Fatal error during health check: $($_.Exception.Message)" -Level "Error"
    exit 3  # Fatal error
}