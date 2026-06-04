if (-not $script:TerminalAiRoot) {
    $script:TerminalAiRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
}

function ai {
    [CmdletBinding()]
    param(
        [Parameter(ValueFromRemainingArguments = $true)]
        [string[]] $Prompt
    )

    $promptText = ($Prompt -join ' ').Trim()
    if (-not $promptText) {
        Write-Host 'Usage: ai <what do you want to do?>'
        return
    }

    $aiCore = Get-TerminalAiCore
    if (-not $aiCore) {
        Write-Error 'ai-core was not found. Build ai-core and add it to PATH.'
        return
    }

    $json = & $aiCore --shell-mode -- $promptText
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($json)) {
        return
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
