#!/usr/bin/env pwsh
#
# Merge Windows and Linux release artifacts into a single dist/site directory.
#
# Usage:
#   ./scripts/merge-release.ps1 -WindowsDir <path> -LinuxDir <path> -OutputRoot <path>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $WindowsDir,

    [Parameter(Mandatory = $true)]
    [string] $LinuxDir,

    [Parameter(Mandatory = $true)]
    [string] $OutputRoot
)

$ErrorActionPreference = 'Stop'

# Copy everything from Windows dir to output
if (Test-Path -LiteralPath $WindowsDir) {
    $windowsContents = Join-Path $WindowsDir '*'
    Copy-Item -Path $windowsContents -Destination $OutputRoot -Recurse -Force
}

# Copy Linux release files into the output (overlapping root files are identical)
$linuxReleases = Join-Path $LinuxDir 'releases'
if (Test-Path -LiteralPath $linuxReleases) {
    $outputReleases = Join-Path $OutputRoot 'releases'
    Copy-Item -LiteralPath $linuxReleases -Destination $outputReleases -Recurse -Force
}

# Copy Linux install/uninstall scripts if they don't exist or are newer
$linuxInstallSh = Join-Path $LinuxDir 'install.sh'
$linuxUninstallSh = Join-Path $LinuxDir 'uninstall.sh'
if (Test-Path -LiteralPath $linuxInstallSh) {
    Copy-Item -LiteralPath $linuxInstallSh -Destination (Join-Path $OutputRoot 'install.sh') -Force
}
if (Test-Path -LiteralPath $linuxUninstallSh) {
    Copy-Item -LiteralPath $linuxUninstallSh -Destination (Join-Path $OutputRoot 'uninstall.sh') -Force
}

# Update version.json to include Linux platform entries
$manifestPath = Join-Path $OutputRoot 'version.json'
if (Test-Path -LiteralPath $manifestPath) {
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json

    # Read Linux checksums
    $linuxChecksumsPath = Join-Path $LinuxDir 'releases' | Join-Path -ChildPath '*'
    $linuxChecksums = Get-ChildItem -Path "$linuxChecksumsPath/checksums.txt" -ErrorAction SilentlyContinue | Select-Object -First 1

    if ($linuxChecksums -and $manifest.PSObject.Properties['windows_x64']) {
        $version = $manifest.version
        $linuxReleaseDir = "releases/$version/linux-x64"

        $linuxChecksumLines = Get-Content -LiteralPath $linuxChecksums.FullName
        $linuxAiCoreSha = $null
        $linuxBashWrapperSha = $null

        foreach ($line in $linuxChecksumLines) {
            $parts = $line -split '\s+', 2
            if ($parts.Count -ge 2) {
                $sha = $parts[0].Trim()
                $path = $parts[1].Trim()
                if ($path -match 'ai-core$') {
                    $linuxAiCoreSha = $sha
                }
                elseif ($path -match 'bash\.sh$') {
                    $linuxBashWrapperSha = $sha
                }
            }
        }

        if ($linuxAiCoreSha) {
            $linuxUrl = "/$linuxReleaseDir/ai-core"

            # Add linux_x64 entry
            $linuxEntry = [ordered] @{
                ai_core = [ordered] @{
                    url = $linuxUrl
                    sha256 = $linuxAiCoreSha
                }
                bash_wrapper = [ordered] @{
                    url = "/$linuxReleaseDir/shell/bash.sh"
                    sha256 = $linuxBashWrapperSha
                }
            }

            $manifest | Add-Member -Name 'linux_x64' -Value $linuxEntry -MemberType NoteProperty -Force
        }

        # Write updated manifest
        $manifestJson = $manifest | ConvertTo-Json -Depth 8
        [IO.File]::WriteAllText($manifestPath, "$manifestJson`n", [Text.UTF8Encoding]::new($false))
    }
}

Write-Host "Merged releases into: $OutputRoot"