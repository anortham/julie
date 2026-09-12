param(
    [Parameter(Mandatory = $true)]
    [string]$Archive,
    [Parameter(Mandatory = $true)]
    [string]$CandidateSha
)

$ErrorActionPreference = 'Stop'
$root = Join-Path $env:RUNNER_TEMP 'julie-native-qualification'
$profile = Join-Path $root 'fresh-profile'
$old = Join-Path $root 'old-version'
$new = Join-Path $root 'new-version'
Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $profile, $old, $new | Out-Null

$env:JULIE_HOME = Join-Path $profile '.julie'
$env:HOME = $profile
$env:USERPROFILE = $profile
$env:APPDATA = Join-Path $profile 'AppData\\Roaming'
$env:LOCALAPPDATA = Join-Path $profile 'AppData\\Local'
if ($CandidateSha -notmatch '^[0-9a-f]{40}$') {
    throw "invalid candidate SHA: $CandidateSha"
}

$process = $null
$server = $null
try {
    Expand-Archive -LiteralPath $Archive -DestinationPath $old
    $server = Join-Path $old 'julie-server.exe'
    if (!(Test-Path -LiteralPath $server)) {
        throw "missing packaged server: $server"
    }

    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $server
    $start.UseShellExecute = $false
    $start.RedirectStandardInput = $true
    $start.RedirectStandardError = $true
    $start.CreateNoWindow = $true
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    if (!$process.Start()) {
        throw 'failed to start packaged server'
    }

    Start-Sleep -Seconds 1
    if ($process.HasExited) {
        throw "packaged server exited before lock check: $($process.StandardError.ReadToEnd())"
    }

    Expand-Archive -LiteralPath $Archive -DestinationPath $new
    if (!(Test-Path -LiteralPath (Join-Path $new 'julie-server.exe'))) {
        throw 'new version extraction did not produce julie-server.exe'
    }

    @{
        archive = (Resolve-Path -LiteralPath $Archive).Path
        archive_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Archive).Hash.ToLowerInvariant()
        candidate_sha = $CandidateSha
        old_server_pid = $process.Id
        old_server_running_during_extract = -not $process.HasExited
        fresh_profile = $profile
        new_version = $new
    } | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $root 'qualification.json')
}
finally {
    if ($null -ne $server) {
        & $server service stop
        if ($LASTEXITCODE -ne 0) {
            throw "packaged service stop failed: $LASTEXITCODE"
        }
    }
    if ($null -ne $process) {
        $process.StandardInput.Close()
        if (!$process.WaitForExit(5000)) {
            $process.Kill()
            $process.WaitForExit()
        }
        if ($process.ExitCode -ne 0) {
            throw "old shim exited with $($process.ExitCode)"
        }
    }
    if (Test-Path -LiteralPath (Join-Path $env:JULIE_HOME 'service.json')) {
        throw 'service discovery file remains after packaged service stop'
    }
}
