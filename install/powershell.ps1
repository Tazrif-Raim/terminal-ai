$ErrorActionPreference = 'Stop'

$TerminalAiBaseUrl = if ($env:TERMINAL_AI_BASE_URL) {
    $env:TERMINAL_AI_BASE_URL.TrimEnd('/')
}
else {
    'https://terminal-ai.lab-node.me'
}

$InstallRoot = if ($env:TERMINAL_AI_INSTALL_DIR) {
    $env:TERMINAL_AI_INSTALL_DIR
}
else {
    Join-Path $env:LOCALAPPDATA 'terminal-ai'
}

$BinDir = Join-Path $InstallRoot 'bin'
$ShellDir = Join-Path $InstallRoot 'shell'
$StateDir = Join-Path $InstallRoot 'state'
$LocalManifestPath = Join-Path $InstallRoot 'version.json'
$WrapperPath = Join-Path $ShellDir 'powershell.ps1'
$AiCorePath = Join-Path $BinDir 'ai-core.exe'
$ProfilePath = if ($env:TERMINAL_AI_PROFILE_PATH) {
    $env:TERMINAL_AI_PROFILE_PATH
}
else {
    $PROFILE.CurrentUserAllHosts
}
$MarkerStart = '# >>> terminal-ai >>>'
$MarkerEnd = '# <<< terminal-ai <<<'

function Join-TerminalAiUrl {
    param(
        [Parameter(Mandatory = $true)]
        [string] $BaseUrl,

        [Parameter(Mandatory = $true)]
        [string] $Path
    )

    if ($Path -match '^https?://') {
        return $Path
    }

    return "$($BaseUrl.TrimEnd('/'))/$($Path.TrimStart('/'))"
}

function Invoke-TerminalAiDownload {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Url,

        [Parameter(Mandatory = $true)]
        [string] $OutFile
    )

    Invoke-WebRequest -Uri $Url -OutFile $OutFile -UseBasicParsing
}

function Assert-TerminalAiHash {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path,

        [Parameter(Mandatory = $true)]
        [string] $ExpectedSha256
    )

    $actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $ExpectedSha256.ToLowerInvariant()) {
        throw "Checksum mismatch for $Path. Expected $ExpectedSha256, got $actual."
    }
}

function Test-TerminalAiFileHash {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path,

        [Parameter(Mandatory = $true)]
        [string] $ExpectedSha256
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        return $false
    }

    $actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    return $actual -eq $ExpectedSha256.ToLowerInvariant()
}

function Get-TerminalAiLocalVersion {
    if (-not (Test-Path -LiteralPath $LocalManifestPath)) {
        return $null
    }

    try {
        return (Get-Content -LiteralPath $LocalManifestPath -Raw | ConvertFrom-Json).version
    }
    catch {
        return $null
    }
}

function Add-TerminalAiToUserPath {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path
    )

    $current = [Environment]::GetEnvironmentVariable('Path', 'User')
    $parts = @($current -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($parts | Where-Object { $_.TrimEnd('\') -ieq $Path.TrimEnd('\') }) {
        return $false
    }

    $updated = @($parts + $Path) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $updated, 'User')
    if (-not (($env:Path -split ';') | Where-Object { $_.TrimEnd('\') -ieq $Path.TrimEnd('\') })) {
        $env:Path = "$env:Path;$Path"
    }

    return $true
}

function Set-TerminalAiProfileBlock {
    param(
        [Parameter(Mandatory = $true)]
        [string] $ProfilePath,

        [Parameter(Mandatory = $true)]
        [string] $WrapperPath
    )

    $profileDir = Split-Path -Parent $ProfilePath
    if ($profileDir) {
        New-Item -ItemType Directory -Path $profileDir -Force | Out-Null
    }

    $content = if (Test-Path -LiteralPath $ProfilePath) {
        Get-Content -LiteralPath $ProfilePath -Raw
    }
    else {
        ''
    }

    $pattern = "(?ms)^$([regex]::Escape($MarkerStart))\r?\n.*?\r?\n$([regex]::Escape($MarkerEnd))\r?\n?"
    $content = ([regex]::Replace($content, $pattern, '')).TrimEnd()
    $escapedWrapperPath = $WrapperPath -replace "'", "''"
    $block = @"
$MarkerStart
. '$escapedWrapperPath'
$MarkerEnd
"@

    $updated = if ([string]::IsNullOrWhiteSpace($content)) {
        $block
    }
    else {
        "$content`r`n`r`n$block"
    }

    Set-Content -LiteralPath $ProfilePath -Value $updated -Encoding UTF8
}

if ($PSVersionTable.PSEdition -eq 'Desktop') {
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
}

if ($env:PROCESSOR_ARCHITECTURE -notin @('AMD64', 'ARM64')) {
    throw "terminal-ai Windows MVP currently supports x64 Windows. Detected $env:PROCESSOR_ARCHITECTURE."
}

New-Item -ItemType Directory -Path $BinDir, $ShellDir, $StateDir -Force | Out-Null

$manifestUrl = Join-TerminalAiUrl -BaseUrl $TerminalAiBaseUrl -Path '/version.json'
$manifestJson = (Invoke-WebRequest -Uri $manifestUrl -UseBasicParsing).Content
$manifestJson = $manifestJson.TrimStart([char] 0xfeff)
if ($manifestJson.StartsWith('ï»¿')) {
    $manifestJson = $manifestJson.Substring(3)
}
$manifest = $manifestJson | ConvertFrom-Json
$assetProperty = $manifest.PSObject.Properties['windows_x64']
if ($null -eq $assetProperty) {
    throw 'version.json does not include windows_x64 assets.'
}
$asset = $assetProperty.Value

$localVersion = Get-TerminalAiLocalVersion
$isCurrent =
    $localVersion -eq $manifest.version -and
    (Test-TerminalAiFileHash -Path $AiCorePath -ExpectedSha256 $asset.ai_core.sha256) -and
    (Test-TerminalAiFileHash -Path $WrapperPath -ExpectedSha256 $asset.powershell_wrapper.sha256)

if (-not $isCurrent) {
    $tempDir = Join-Path ([IO.Path]::GetTempPath()) "terminal-ai-install-$([guid]::NewGuid())"
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

    try {
        $aiCoreDownload = Join-Path $tempDir 'ai-core.exe'
        $wrapperDownload = Join-Path $tempDir 'powershell.ps1'

        Invoke-TerminalAiDownload -Url (Join-TerminalAiUrl $TerminalAiBaseUrl $asset.ai_core.url) -OutFile $aiCoreDownload
        Invoke-TerminalAiDownload -Url (Join-TerminalAiUrl $TerminalAiBaseUrl $asset.powershell_wrapper.url) -OutFile $wrapperDownload
        Assert-TerminalAiHash -Path $aiCoreDownload -ExpectedSha256 $asset.ai_core.sha256
        Assert-TerminalAiHash -Path $wrapperDownload -ExpectedSha256 $asset.powershell_wrapper.sha256

        Copy-Item -LiteralPath $aiCoreDownload -Destination $AiCorePath -Force
        Copy-Item -LiteralPath $wrapperDownload -Destination $WrapperPath -Force
        $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $LocalManifestPath -Encoding UTF8
    }
    finally {
        if (Test-Path -LiteralPath $tempDir) {
            Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

$pathChanged = $false
if ($env:TERMINAL_AI_SKIP_PATH -ne '1') {
    $pathChanged = Add-TerminalAiToUserPath -Path $BinDir
}
Set-TerminalAiProfileBlock -ProfilePath $ProfilePath -WrapperPath $WrapperPath

if ($isCurrent) {
    Write-Host "terminal-ai $($manifest.version) is already up to date."
}
elseif ($localVersion) {
    Write-Host "Updated terminal-ai from $localVersion to $($manifest.version)."
}
else {
    Write-Host "Installed terminal-ai $($manifest.version)."
}

Write-Host "Install root: $InstallRoot"
Write-Host "PowerShell profile: $ProfilePath"
if ($pathChanged) {
    Write-Host 'PATH updated. Open a new terminal if ai-core is not found in this one.'
}
Write-Host 'Reload your profile with: . $PROFILE'
Write-Host 'Next: ai --config'
