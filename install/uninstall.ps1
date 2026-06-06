$ErrorActionPreference = 'Stop'

$InstallRoot = if ($env:TERMINAL_AI_INSTALL_DIR) {
    $env:TERMINAL_AI_INSTALL_DIR
}
else {
    Join-Path $env:LOCALAPPDATA 'terminal-ai'
}

$BinDir = Join-Path $InstallRoot 'bin'
$ProfilePath = if ($env:TERMINAL_AI_PROFILE_PATH) {
    $env:TERMINAL_AI_PROFILE_PATH
}
else {
    $PROFILE.CurrentUserAllHosts
}
$ConfigRoot = if ($env:TERMINAL_AI_CONFIG_DIR) {
    $env:TERMINAL_AI_CONFIG_DIR
}
else {
    Join-Path $env:APPDATA 'terminal-ai'
}
$MarkerStart = '# >>> terminal-ai >>>'
$MarkerEnd = '# <<< terminal-ai <<<'
$Removed = New-Object System.Collections.Generic.List[string]

function Test-TerminalAiOwnedPath {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path,

        [Parameter(Mandatory = $true)]
        [string] $Parent
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        return $false
    }

    $resolvedPath = (Resolve-Path -LiteralPath $Path).Path.TrimEnd('\')
    $resolvedParent = (Resolve-Path -LiteralPath $Parent).Path.TrimEnd('\')
    $leaf = Split-Path -Leaf $resolvedPath

    return $leaf -eq 'terminal-ai' -and $resolvedPath.StartsWith($resolvedParent, [StringComparison]::OrdinalIgnoreCase)
}

function Remove-TerminalAiDirectory {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path,

        [Parameter(Mandatory = $true)]
        [string] $Parent,

        [System.Collections.Generic.List[string]] $Removed
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        return
    }

    if (-not (Test-TerminalAiOwnedPath -Path $Path -Parent $Parent)) {
        throw "Refusing to remove unexpected path: $Path"
    }

    Remove-Item -LiteralPath $Path -Recurse -Force
    $Removed.Add($Path)
}

function Remove-TerminalAiProfileBlock {
    param(
        [Parameter(Mandatory = $true)]
        [string] $ProfilePath,

        [System.Collections.Generic.List[string]] $Removed
    )

    if (-not (Test-Path -LiteralPath $ProfilePath)) {
        return
    }

    $content = Get-Content -LiteralPath $ProfilePath -Raw
    $pattern = "(?ms)^$([regex]::Escape($MarkerStart))\r?\n.*?\r?\n$([regex]::Escape($MarkerEnd))\r?\n?"
    $updated = ([regex]::Replace($content, $pattern, '')).TrimEnd()

    if ($updated -ne $content.TrimEnd()) {
        Set-Content -LiteralPath $ProfilePath -Value $updated -Encoding UTF8
        $Removed.Add("profile block from $ProfilePath")
    }
}

function Remove-TerminalAiFromUserPath {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path,

        [System.Collections.Generic.List[string]] $Removed
    )

    $current = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ([string]::IsNullOrWhiteSpace($current)) {
        return
    }

    $parts = @($current -split ';' | Where-Object {
            -not [string]::IsNullOrWhiteSpace($_) -and
            $_.TrimEnd('\') -ine $Path.TrimEnd('\')
        })
    $updated = $parts -join ';'

    if ($updated -ne $current) {
        [Environment]::SetEnvironmentVariable('Path', $updated, 'User')
        $Removed.Add("PATH entry $Path")
    }
}

function Test-TerminalAiRemoveUserData {
    if ($env:TERMINAL_AI_UNINSTALL_ALL -eq '1') {
        return $true
    }

    if ($env:TERMINAL_AI_UNINSTALL_KEEP_CONFIG -eq '1') {
        return $false
    }

    $answer = Read-Host "Remove terminal-ai config and local history at $ConfigRoot? [y/N]"
    return $answer -match '^(y|yes)$'
}

Remove-TerminalAiProfileBlock -ProfilePath $ProfilePath -Removed $Removed
if ($env:TERMINAL_AI_SKIP_PATH -ne '1') {
    Remove-TerminalAiFromUserPath -Path $BinDir -Removed $Removed
}
Remove-TerminalAiDirectory -Path $InstallRoot -Parent $env:LOCALAPPDATA -Removed $Removed

if (Test-TerminalAiRemoveUserData) {
    $configParent = Split-Path -Parent $ConfigRoot
    Remove-TerminalAiDirectory -Path $ConfigRoot -Parent $configParent -Removed $Removed
}

if ($Removed.Count -eq 0) {
    Write-Host 'terminal-ai was not installed, or it was already removed.'
}
else {
    Write-Host 'Removed:'
    foreach ($item in $Removed) {
        Write-Host "  $item"
    }
}
