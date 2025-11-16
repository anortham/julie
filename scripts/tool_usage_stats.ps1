# Simple tool usage statistics from Julie logs
# Usage: .\scripts\tool_usage_stats.ps1

$date = Get-Date -Format "yyyy-MM-dd"
$logFile = ".julie\logs\julie.log.$date"

if (-not (Test-Path $logFile)) {
    Write-Host "❌ Log file not found: $logFile" -ForegroundColor Red
    Write-Host "💡 Make sure you're running this from the Julie workspace directory" -ForegroundColor Yellow
    exit 1
}

Write-Host "`n📊 Julie Tool Usage Statistics" -ForegroundColor Cyan
Write-Host "================================`n"
Write-Host "Analyzing: $logFile`n"

# Extract tool names and count usage
$toolCalls = Get-Content $logFile |
    Select-String "🛠️  Executing tool:" |
    ForEach-Object {
        $_ -replace '.*Executing tool: ', ''
    }

$stats = $toolCalls |
    Group-Object |
    Sort-Object Count -Descending |
    ForEach-Object {
        [PSCustomObject]@{
            Count = $_.Count
            Tool = $_.Name
        }
    }

$stats | Format-Table -Property @{Label='Count'; Expression={$_.Count}; Width=7},
                                 @{Label='Tool'; Expression={$_.Tool}}

Write-Host "`nTotal tool calls: $($toolCalls.Count)" -ForegroundColor Green
Write-Host ""
