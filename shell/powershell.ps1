$script:TerminalAiRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path

function ai {
    [CmdletBinding()]
    param(
        [switch] $Agent,
        [switch] $DryRun,
        [switch] $AgentLogs,

        [Parameter(ValueFromRemainingArguments = $true)]
        [string[]] $Prompt
    )

    $agentMode = $Agent.IsPresent
    $dryRunMode = $DryRun.IsPresent
    $agentLogsMode = $AgentLogs.IsPresent
    $promptArgs = @($Prompt)
    if ($promptArgs.Count -gt 0 -and $promptArgs[0] -eq '--agent-logs') {
        $agentLogsMode = $true
        if ($promptArgs.Count -eq 1) {
            $promptArgs = @()
        }
        else {
            $promptArgs = @($promptArgs[1..($promptArgs.Count - 1)])
        }
    }

    if ($promptArgs.Count -gt 0 -and $promptArgs[0] -eq '--agent') {
        $agentMode = $true
        if ($promptArgs.Count -eq 1) {
            $promptArgs = @()
        }
        else {
            $promptArgs = @($promptArgs[1..($promptArgs.Count - 1)])
        }
    }

    if ($agentMode -and $promptArgs.Count -gt 0 -and $promptArgs[0] -eq '--dry-run') {
        $dryRunMode = $true
        if ($promptArgs.Count -eq 1) {
            $promptArgs = @()
        }
        else {
            $promptArgs = @($promptArgs[1..($promptArgs.Count - 1)])
        }
    }

    if (-not $agentMode -and -not $agentLogsMode -and $promptArgs.Count -eq 1) {
        switch ($promptArgs[0]) {
            '--help' {
                Show-TerminalAiHelp
                return
            }
            '--version' {
                Show-TerminalAiVersion
                return
            }
            '--config' {
                Invoke-TerminalAiConfig
                return
            }
        }
    }

    if ($agentLogsMode) {
        $aiCore = Get-TerminalAiCore
        if (-not $aiCore) {
            Write-Error 'ai-core was not found. Build ai-core and add it to PATH.'
            return
        }

        $aiCoreArgs = @('--agent-logs')
        $aiCoreArgs += $promptArgs
        & $aiCore @aiCoreArgs
        return
    }

    $parsedArgs = Split-TerminalAiPromptArgs $promptArgs
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

    $terminalAiEnv = Get-TerminalAiBaseEnv
    $contextEnv = Get-TerminalAiContextEnv
    foreach ($key in $contextEnv.Keys) {
        $terminalAiEnv[$key] = $contextEnv[$key]
    }
    $previousTerminalAiEnv = Set-TerminalAiProcessEnv $terminalAiEnv

    try {
        if ($agentMode) {
            $aiCoreArgs = @('--agent')
            if ($dryRunMode) {
                $aiCoreArgs += '--dry-run'
            }
        }
        else {
            $aiCoreArgs = @('--shell-mode')
        }

        if ($parsedArgs.Files.Count -gt 0) {
            $aiCoreArgs += '--files'
            $aiCoreArgs += $parsedArgs.Files
        }
        $aiCoreArgs += '--'
        $aiCoreArgs += $promptText

        if ($agentMode) {
            & $aiCore @aiCoreArgs
            return
        }

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

            Invoke-TerminalAiEditableCommand $result.command
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

function Show-TerminalAiHelp {
    $help = @'
terminal-ai

Usage:
  ai <what do you want to do?>
  ai --agent <what do you want to do?>
  ai --agent --dry-run <what do you want to do?>
  ai --agent-logs [open]

Commands:
  ai --help       Show this help.
  ai --version    Show the installed version.
  ai --config     View or edit LLM BYOK config.
  ai --agent ...  Run the agent workflow.
  ai --agent-logs List recent agent audit logs.

Only those exact invocations are commands. Any extra text is sent as a prompt.

Example usages:
  ai what is running on port 3000
  ai --agent list all files in this directory
  ai --agent --dry-run inspect this repo and propose setup steps
  ai --agent-logs
  ai see these files --files README.md docs\TODO.md
  ai --config
'@

    Write-Host $help
}

function Show-TerminalAiVersion {
    $aiCore = Get-TerminalAiCore
    if (-not $aiCore) {
        Write-Error 'ai-core was not found. Build ai-core and add it to PATH.'
        return
    }

    $version = & $aiCore --version
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($version)) {
        return
    }

    $version -replace '^ai-core\s+', 'terminal-ai '
}

function Invoke-TerminalAiConfig {
    $aiCore = Get-TerminalAiCore
    if (-not $aiCore) {
        Write-Error 'ai-core was not found. Build ai-core and add it to PATH.'
        return
    }

    $terminalAiEnv = Get-TerminalAiBaseEnv
    $previousTerminalAiEnv = Set-TerminalAiProcessEnv $terminalAiEnv

    try {
        & $aiCore --config
    }
    finally {
        Restore-TerminalAiProcessEnv $previousTerminalAiEnv
    }
}

function Get-TerminalAiCore {
    $repoManifest = Join-Path $script:TerminalAiRoot 'ai-core\Cargo.toml'
    $repoBinary = Join-Path $script:TerminalAiRoot 'ai-core\target\debug\ai-core.exe'
    if ((Test-Path -LiteralPath $repoManifest) -and (Test-Path -LiteralPath $repoBinary)) {
        return (Resolve-Path -LiteralPath $repoBinary).Path
    }

    $installedBinary = Join-Path $script:TerminalAiRoot 'bin\ai-core.exe'
    if (Test-Path -LiteralPath $installedBinary) {
        return (Resolve-Path -LiteralPath $installedBinary).Path
    }

    $command = Get-Command ai-core -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
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

function Invoke-TerminalAiEditableCommand {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Command
    )

    if ([Console]::IsInputRedirected -or [Console]::IsOutputRedirected) {
        Copy-TerminalAiCommand $Command
        return
    }

    $editedCommand = Read-TerminalAiEditableLine -InitialText $Command
    if ([string]::IsNullOrWhiteSpace($editedCommand)) {
        return
    }

    Invoke-TerminalAiCommand $editedCommand
}

function Read-TerminalAiEditableLine {
    param(
        [Parameter(Mandatory = $true)]
        [string] $InitialText
    )

    Write-Host '> ' -ForegroundColor DarkGray -NoNewline

    $buffer = $InitialText
    $cursor = $buffer.Length
    $startLeft = [Console]::CursorLeft
    $startTop = [Console]::CursorTop
    $lastLength = 0

    Render-TerminalAiEditableLine `
        -Buffer $buffer `
        -Cursor $cursor `
        -StartLeft $startLeft `
        -StartTop $startTop `
        -PreviousLength ([ref] $lastLength)

    while ($true) {
        $key = [Console]::ReadKey($true)

        if (($key.Modifiers -band [ConsoleModifiers]::Control) -and $key.Key -eq [ConsoleKey]::C) {
            Write-Host ''
            return $null
        }

        switch ($key.Key) {
            ([ConsoleKey]::Enter) {
                Write-Host ''
                return $buffer
            }
            ([ConsoleKey]::Escape) {
                Write-Host ''
                return $null
            }
            ([ConsoleKey]::Backspace) {
                if ($cursor -gt 0) {
                    $buffer = Remove-TerminalAiCharAt -Value $buffer -Index ($cursor - 1)
                    $cursor--
                }
            }
            ([ConsoleKey]::Delete) {
                if ($cursor -lt $buffer.Length) {
                    $buffer = Remove-TerminalAiCharAt -Value $buffer -Index $cursor
                }
            }
            ([ConsoleKey]::LeftArrow) {
                if ($cursor -gt 0) {
                    $cursor--
                }
            }
            ([ConsoleKey]::RightArrow) {
                if ($cursor -lt $buffer.Length) {
                    $cursor++
                }
            }
            ([ConsoleKey]::Home) {
                $cursor = 0
            }
            ([ConsoleKey]::End) {
                $cursor = $buffer.Length
            }
            default {
                if (-not [char]::IsControl($key.KeyChar)) {
                    $buffer = Insert-TerminalAiCharAt -Value $buffer -Index $cursor -Char $key.KeyChar
                    $cursor++
                }
            }
        }

        Render-TerminalAiEditableLine `
            -Buffer $buffer `
            -Cursor $cursor `
            -StartLeft $startLeft `
            -StartTop $startTop `
            -PreviousLength ([ref] $lastLength)
    }
}

function Render-TerminalAiEditableLine {
    param(
        [Parameter(Mandatory = $true)]
        [AllowEmptyString()]
        [string] $Buffer,

        [Parameter(Mandatory = $true)]
        [int] $Cursor,

        [Parameter(Mandatory = $true)]
        [int] $StartLeft,

        [Parameter(Mandatory = $true)]
        [int] $StartTop,

        [Parameter(Mandatory = $true)]
        [ref] $PreviousLength
    )

    $clearLength = [Math]::Max([int] $PreviousLength.Value, $Buffer.Length)
    [Console]::SetCursorPosition($StartLeft, $StartTop)
    [Console]::Write($Buffer)
    if ($clearLength -gt $Buffer.Length) {
        [Console]::Write((' ' * ($clearLength - $Buffer.Length)))
    }

    $PreviousLength.Value = $Buffer.Length
    Set-TerminalAiCursorForOffset -StartLeft $StartLeft -StartTop $StartTop -Offset $Cursor
}

function Set-TerminalAiCursorForOffset {
    param(
        [Parameter(Mandatory = $true)]
        [int] $StartLeft,

        [Parameter(Mandatory = $true)]
        [int] $StartTop,

        [Parameter(Mandatory = $true)]
        [int] $Offset
    )

    $width = [Console]::BufferWidth
    if ($width -le 0) {
        $width = 120
    }

    $absolute = $StartLeft + $Offset
    $left = $absolute % $width
    $top = $StartTop + [Math]::Floor($absolute / $width)

    [Console]::SetCursorPosition($left, $top)
}

function Insert-TerminalAiCharAt {
    param(
        [Parameter(Mandatory = $true)]
        [AllowEmptyString()]
        [string] $Value,

        [Parameter(Mandatory = $true)]
        [int] $Index,

        [Parameter(Mandatory = $true)]
        [char] $Char
    )

    if ($Index -le 0) {
        return "$Char$Value"
    }

    if ($Index -ge $Value.Length) {
        return "$Value$Char"
    }

    return $Value.Substring(0, $Index) + $Char + $Value.Substring($Index)
}

function Remove-TerminalAiCharAt {
    param(
        [Parameter(Mandatory = $true)]
        [AllowEmptyString()]
        [string] $Value,

        [Parameter(Mandatory = $true)]
        [int] $Index
    )

    if ($Value.Length -eq 0 -or $Index -lt 0 -or $Index -ge $Value.Length) {
        return $Value
    }

    if ($Index -eq 0) {
        return $Value.Substring(1)
    }

    if ($Index -eq ($Value.Length - 1)) {
        return $Value.Substring(0, $Index)
    }

    return $Value.Substring(0, $Index) + $Value.Substring($Index + 1)
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

function Get-TerminalAiBaseEnv {
    $values = @{}

    if (-not $env:TERMINAL_AI_DOTENV_PATH) {
        $values['TERMINAL_AI_DOTENV_PATH'] = Join-Path $script:TerminalAiRoot '.env'
    }

    return $values
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
