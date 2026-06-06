[CmdletBinding()]
param(
    [string] $BaseUrl = 'https://terminal-ai.lab-node.me',
    [string] $OutputRoot,
    [switch] $SkipBuild
)

$ErrorActionPreference = 'Stop'

$RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $RepoRoot 'dist\site'
}

$CargoToml = Join-Path $RepoRoot 'ai-core\Cargo.toml'
$WrapperSource = Join-Path $RepoRoot 'shell\powershell.ps1'
$InstallerSource = Join-Path $RepoRoot 'install\powershell.ps1'
$UninstallerSource = Join-Path $RepoRoot 'install\uninstall.ps1'
$ReleaseBinary = Join-Path $RepoRoot 'ai-core\target\release\ai-core.exe'

function Get-TerminalAiVersion {
    param(
        [Parameter(Mandatory = $true)]
        [string] $CargoToml
    )

    $line = Select-String -LiteralPath $CargoToml -Pattern '^\s*version\s*=\s*"([^"]+)"' | Select-Object -First 1
    if (-not $line) {
        throw "Could not read version from $CargoToml."
    }

    return $line.Matches[0].Groups[1].Value
}

function Get-TerminalAiSha256 {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path
    )

    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Join-TerminalAiUrl {
    param(
        [Parameter(Mandatory = $true)]
        [string] $BaseUrl,

        [Parameter(Mandatory = $true)]
        [string] $Path
    )

    return "$($BaseUrl.TrimEnd('/'))/$($Path.TrimStart('/'))"
}

function Set-TerminalAiUtf8File {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path,

        [Parameter(Mandatory = $true)]
        [string] $Value
    )

    [IO.File]::WriteAllText($Path, $Value, [Text.UTF8Encoding]::new($false))
}

$version = Get-TerminalAiVersion -CargoToml $CargoToml

if (-not $SkipBuild) {
    & cargo build --manifest-path $CargoToml --release
    if ($LASTEXITCODE -ne 0) {
        throw 'cargo build failed.'
    }
}

if (-not (Test-Path -LiteralPath $ReleaseBinary)) {
    throw "Release binary not found: $ReleaseBinary"
}

$releaseDir = Join-Path $OutputRoot "releases\$version\windows-x64"
$releaseShellDir = Join-Path $releaseDir 'shell'
New-Item -ItemType Directory -Path $OutputRoot, $releaseDir, $releaseShellDir -Force | Out-Null

$aiCoreOut = Join-Path $releaseDir 'ai-core.exe'
$wrapperOut = Join-Path $releaseShellDir 'powershell.ps1'
$installerOut = Join-Path $OutputRoot 'powershell.ps1'
$uninstallerOut = Join-Path $OutputRoot 'uninstall.ps1'
$healthOut = Join-Path $OutputRoot 'health.txt'
$checksumsOut = Join-Path $releaseDir 'checksums.txt'
$manifestOut = Join-Path $OutputRoot 'version.json'

Copy-Item -LiteralPath $ReleaseBinary -Destination $aiCoreOut -Force
Copy-Item -LiteralPath $WrapperSource -Destination $wrapperOut -Force
Copy-Item -LiteralPath $InstallerSource -Destination $installerOut -Force
Copy-Item -LiteralPath $UninstallerSource -Destination $uninstallerOut -Force

$aiCoreSha = Get-TerminalAiSha256 -Path $aiCoreOut
$wrapperSha = Get-TerminalAiSha256 -Path $wrapperOut

$aiCoreUrl = "/releases/$version/windows-x64/ai-core.exe"
$wrapperUrl = "/releases/$version/windows-x64/shell/powershell.ps1"

$manifest = [ordered] @{
    version = $version
    channel = 'stable'
    windows_x64 = [ordered] @{
        ai_core = [ordered] @{
            url = $aiCoreUrl
            sha256 = $aiCoreSha
        }
        powershell_wrapper = [ordered] @{
            url = $wrapperUrl
            sha256 = $wrapperSha
        }
    }
}

$manifestJson = $manifest | ConvertTo-Json -Depth 8
Set-TerminalAiUtf8File -Path $manifestOut -Value "$manifestJson`n"
Set-TerminalAiUtf8File -Path $healthOut -Value "ok`n"
$checksums = @(
    "$aiCoreSha  releases/$version/windows-x64/ai-core.exe"
    "$wrapperSha  releases/$version/windows-x64/shell/powershell.ps1"
) -join "`n"
Set-TerminalAiUtf8File -Path $checksumsOut -Value "$checksums`n"

Write-Host "Packaged terminal-ai $version"
Write-Host "Output: $OutputRoot"
Write-Host "Install URL: $(Join-TerminalAiUrl $BaseUrl '/powershell.ps1')"
Write-Host "Uninstall URL: $(Join-TerminalAiUrl $BaseUrl '/uninstall.ps1')"
