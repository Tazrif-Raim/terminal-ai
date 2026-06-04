if (-not $script:TerminalAiRoot) {
    $script:TerminalAiRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
}

function ai {
    [CmdletBinding()]
    param(
        [Parameter(ValueFromRemainingArguments = $true)]
        [string[]] $Prompt
    )

    $parsedArgs = Split-TerminalAiPromptArgs $Prompt
    if (-not $parsedArgs) {
        return
    }

    $promptText = $parsedArgs.Prompt
    if (-not $promptText) {
        Write-Host 'Usage: ai <what do you want to do?>'
        return
    }

    $aiCore = Get-TerminalAiCore
    if (-not $aiCore) {
        Write-Error 'ai-core was not found. Build ai-core and add it to PATH.'
        return
    }

    $terminalAiEnv = Get-TerminalAiContextEnv
    if (-not $env:TERMINAL_AI_DOTENV_PATH) {
        $terminalAiEnv['TERMINAL_AI_DOTENV_PATH'] = Join-Path $script:TerminalAiRoot '.env'
    }
    $previousTerminalAiEnv = Set-TerminalAiProcessEnv $terminalAiEnv

    try {
        $aiCoreArgs = @('--shell-mode')
        if ($parsedArgs.Files.Count -gt 0) {
            $aiCoreArgs += '--files'
            $aiCoreArgs += $parsedArgs.Files
        }
        $aiCoreArgs += '--'
        $aiCoreArgs += $promptText

        $json = & $aiCore @aiCoreArgs
        if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($json)) {
            return
        }
    }
    finally {
        Restore-TerminalAiProcessEnv $previousTerminalAiEnv
    }

    $result = ConvertFrom-TerminalAiJson $json
    if (-not $result) {
        return
    }

    switch ($result.action) {
        'cancel' {
            return
        }
        'edit' {
            if ([string]::IsNullOrWhiteSpace($result.command)) {
                return
            }

            Copy-TerminalAiCommand $result.command
            return
        }
        'copy' {
            if ([string]::IsNullOrWhiteSpace($result.command)) {
                return
            }

            Copy-TerminalAiCommand $result.command
            return
        }
        'run' {
            if ([string]::IsNullOrWhiteSpace($result.command)) {
                return
            }

            Invoke-TerminalAiCommand $result.command -PrintCommand
            return
        }
        default {
            Write-Error "ai-core returned an unknown action: $($result.action)"
            return
        }
    }
}

function Get-TerminalAiCore {
    $command = Get-Command ai-core -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    $repoBinary = Join-Path $script:TerminalAiRoot 'ai-core\target\debug\ai-core.exe'
    if (Test-Path -LiteralPath $repoBinary) {
        return (Resolve-Path -LiteralPath $repoBinary).Path
    }

    return $null
}

function Split-TerminalAiPromptArgs {
    param(
        [string[]] $InputArgs
    )

    $usage = 'Usage: ai <what do you want to do?> --files <file_1> <file_2>'
    $files = New-Object System.Collections.Generic.List[string]
    $promptParts = New-Object System.Collections.Generic.List[string]

    $filesFlagIndex = -1
    for ($i = 0; $i -lt $InputArgs.Count; $i++) {
        if ($InputArgs[$i] -eq '--files') {
            $filesFlagIndex = $i
            break
        }
    }

    if ($filesFlagIndex -lt 0) {
        foreach ($arg in $InputArgs) {
            $promptParts.Add($arg)
        }

        return [pscustomobject] @{
            Prompt = ($promptParts -join ' ').Trim()
            Files = $files.ToArray()
        }
    }

    for ($i = 0; $i -lt $filesFlagIndex; $i++) {
        $promptParts.Add($InputArgs[$i])
    }

    $separatorIndex = -1
    for ($i = $filesFlagIndex + 1; $i -lt $InputArgs.Count; $i++) {
        if ($InputArgs[$i] -eq '--') {
            $separatorIndex = $i
            break
        }
    }

    $fileEndIndex = $InputArgs.Count
    if ($separatorIndex -ge 0) {
        $fileEndIndex = $separatorIndex
    }

    for ($i = $filesFlagIndex + 1; $i -lt $fileEndIndex; $i++) {
        $files.Add($InputArgs[$i])
    }

    if ($files.Count -eq 0) {
        if ($separatorIndex -ge 0) {
            Write-Host $usage
            return $null
        }

        foreach ($arg in $InputArgs[($filesFlagIndex)..($InputArgs.Count - 1)]) {
            $promptParts.Add($arg)
        }

        return [pscustomobject] @{
            Prompt = ($promptParts -join ' ').Trim()
            Files = @()
        }
    }

    if ($separatorIndex -ge 0) {
        for ($i = $separatorIndex + 1; $i -lt $InputArgs.Count; $i++) {
            $promptParts.Add($InputArgs[$i])
        }
    }
    elseif ($filesFlagIndex -eq 0) {
        Write-Host $usage
        return $null
    }
    elseif (-not (Test-TerminalAiFileArgs $files)) {
        foreach ($arg in $InputArgs[($filesFlagIndex)..($InputArgs.Count - 1)]) {
            $promptParts.Add($arg)
        }
        $files.Clear()
    }

    return [pscustomobject] @{
        Prompt = ($promptParts -join ' ').Trim()
        Files = $files.ToArray()
    }
}

function Test-TerminalAiFileArgs {
    param(
        [Parameter(Mandatory = $true)]
        [System.Collections.Generic.List[string]] $Values
    )

    foreach ($value in $Values) {
        if (Test-TerminalAiFileArg $value) {
            return $true
        }
    }

    return $false
}

function Test-TerminalAiFileArg {
    param(
        [AllowEmptyString()]
        [string] $Value
    )

    if ([string]::IsNullOrWhiteSpace($Value)) {
        return $false
    }

    if (Test-Path -LiteralPath $Value) {
        return $true
    }

    return $Value.Contains('\') -or $Value.Contains('/') -or [System.IO.Path]::HasExtension($Value)
}

function ConvertFrom-TerminalAiJson {
    param(
        [Parameter(Mandatory = $true)]
        [AllowEmptyString()]
        [string] $Json
    )

    try {
        return $Json | ConvertFrom-Json -ErrorAction Stop
    }
    catch {
        Write-Error "ai-core returned invalid JSON: $Json"
        return $null
    }
}

function Copy-TerminalAiCommand {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Command
    )

    try {
        Set-Clipboard -Value $Command -ErrorAction Stop
        Write-Host 'Copied command. Paste it into the next prompt to edit or run:' -ForegroundColor DarkGray
    }
    catch {
        $clip = Get-Command clip.exe -ErrorAction SilentlyContinue
        if ($clip) {
            $Command | & $clip.Source
            Write-Host 'Copied command. Paste it into the next prompt to edit or run:' -ForegroundColor DarkGray
        }
        else {
            Write-Warning 'Could not copy the command to the clipboard.'
            Write-Host 'Copy this command manually:' -ForegroundColor DarkGray
        }
    }

    Write-Host $Command
}

function Invoke-TerminalAiCommand {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Command,

        [switch] $PrintCommand
    )

    try {
        [Microsoft.PowerShell.PSConsoleReadLine]::AddToHistory($Command)
    }
    catch {
        # Running should still work in hosts where PSReadLine history is unavailable.
    }

    if ($PrintCommand) {
        Write-Host "> $Command" -ForegroundColor DarkGray
    }

    Invoke-Expression $Command
}

function Get-TerminalAiContextEnv {
    $values = @{
        TERMINAL_AI_SHELL_NAME = 'PowerShell'
        TERMINAL_AI_SHELL_VERSION = $PSVersionTable.PSVersion.ToString()
        TERMINAL_AI_OS_VERSION = Get-TerminalAiOsVersion
    }

    $recentCommands = Get-TerminalAiRecentCommands -MaxCount 20
    if ($recentCommands.Count -gt 0) {
        $values['TERMINAL_AI_RECENT_COMMANDS'] = $recentCommands -join "`n"
    }

    return $values
}

function Get-TerminalAiOsVersion {
    if ($PSVersionTable.OS) {
        return $PSVersionTable.OS
    }

    return [System.Environment]::OSVersion.VersionString
}

function Get-TerminalAiRecentCommands {
    param(
        [int] $MaxCount = 20
    )

    $history = Get-History -Count ($MaxCount + 5) -ErrorAction SilentlyContinue
    if (-not $history) {
        return @()
    }

    return @(
        $history |
            Select-Object -ExpandProperty CommandLine |
            Where-Object { $_ -and $_.Trim() -and ($_ -notmatch '^\s*ai(\s|$)') } |
            Select-Object -Last $MaxCount
    )
}

function Set-TerminalAiProcessEnv {
    param(
        [Parameter(Mandatory = $true)]
        [hashtable] $Values
    )

    $previous = @{}
    foreach ($key in $Values.Keys) {
        $previous[$key] = [System.Environment]::GetEnvironmentVariable($key, 'Process')
        [System.Environment]::SetEnvironmentVariable($key, [string] $Values[$key], 'Process')
    }

    return $previous
}

function Restore-TerminalAiProcessEnv {
    param(
        [Parameter(Mandatory = $true)]
        [hashtable] $Previous
    )

    foreach ($key in $Previous.Keys) {
        [System.Environment]::SetEnvironmentVariable($key, $Previous[$key], 'Process')
    }
}
