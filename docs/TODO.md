# TODO.md — Terminal AI Command Assistant

## Project Summary

Build a fast terminal-native AI command assistant.

Target UX:

```bash
ai what is running on port 3000
```

Flow:

```txt
prompt -> generate 1-3 command options -> select edit/run -> close picker -> return to terminal -> insert or run command natively
```

Core idea:

```txt
Shell wrapper handles native terminal behavior
Rust binary handles LLM, option generation, picker UI, and JSON output
```

Primary target:

```txt
Windows Terminal + PowerShell
```

Future targets:

```txt
zsh
fish
bash
Linux
macOS
```

---

## Phase 1 — Project Setup

- [x] Create repository. (already in terminal-ai directory/folder)
- [x] Create base folder structure:

```txt
terminal-ai/
  ai-core/
    Cargo.toml
    src/
      main.rs
      config.rs
      llm.rs
      picker.rs
      prompt.rs
      safety.rs
      types.rs
  shell/
    powershell.ps1
    zsh.zsh
    fish.fish
    bash.sh
  docs/
    TODO.md
  README.md
```

- [x] Initialize Rust project:

```bash
cargo new ai-core
```

- [x] Add core Rust dependencies:
  - [x] `clap` for CLI args.
  - [x] `reqwest` for HTTP requests.
  - [x] `serde` and `serde_json` for JSON.
  - [x] `crossterm` for terminal UI and keyboard input.
  - [x] `directories` for config file locations.
  - [x] `ratatui` for richer TUI.

- [x] Decide binary name:
  - [x] Internal binary: `ai-core`
  - [x] User-facing shell function: `ai`

---

## Phase 2 — Basic CLI Input

Goal: allow this type of command:

```powershell
ai what is running on port 3000
```

- [x] Implement argument parsing in Rust.
- [x] Accept unquoted prompt text.
- [x] Join all args after command into one prompt string.
- [x] Add `--shell-mode` flag.
- [x] Add `--debug` flag.
- [x] Add `--version` flag.
- [x] Handle empty prompt:

```txt
Usage: ai <what do you want to do?>
```

- [x] Keep stdout reserved for final machine-readable JSON in `--shell-mode`.
- [x] Send UI, errors, loading text, and debug logs to stderr.

---

## Phase 3 — Config System

Goal: store LLM settings locally.

- [x] Support environment variables:
  - [x] `LLM_API_URL`
  - [x] `LLM_API_KEY`
  - [x] `LLM_MODEL`

- [x] Support config file later:

Windows:

```txt
%APPDATA%/terminal-ai/config.json
```

Linux/macOS:

```txt
~/.config/terminal-ai/config.json
```

- [x] Config fields:

```json
{
  "api_url": "https://example.com/v1/chat/completions",
  "api_key": "your-key",
  "model": "your-model",
  "default_shell": "powershell",
  "max_options": 3
}
```

- [x] Add clear error if API URL/key/model is missing.
- [x] Add command to print current resolved config without exposing full API key.
- [x] Mask API key in logs.

---

## Phase 4 — LLM Prompt + API Call

Goal: generate 1-3 command options.

- [x] Build OpenAI-compatible chat completions request.
- [x] Send current OS and shell context.
- [x] Start with PowerShell as primary target on Windows.
- [x] Use low temperature.
- [x] Ask the model to return JSON only.
- [x] Parse JSON response into typed Rust structs.
- [x] Validate command options.
- [x] Limit options to 1-3.

Expected LLM response shape:

```json
{
  "options": [
    {
      "title": "Show process using port 3000",
      "command": "Get-NetTCPConnection -LocalPort 3000 | Select-Object LocalAddress,LocalPort,OwningProcess",
      "risk": "safe"
    }
  ]
}
```

- [x] Implement fallback parsing if model returns markdown/code fences.
- [x] Show helpful error if LLM response cannot be parsed.
- [x] Add retry only for malformed JSON, not for every error.

System prompt requirements:

- [x] Return JSON only.
- [x] Return 1-3 options.
- [x] Target the detected OS and shell.
- [x] Prefer PowerShell commands on Windows.
- [x] Prefer inspection commands before destructive commands.
- [x] Never invent unknown file paths, container names, branches, PIDs, or process names.
- [x] Mark risky commands as `dangerous`.
- [x] Keep explanations brief.

---

## Phase 5 — Command Option Picker UI

Goal: show terminal-native selection UI.

Example UI:

```txt
Select command

> Show process using port 3000
  Get-NetTCPConnection -LocalPort 3000 | Select-Object LocalAddress,LocalPort,OwningProcess

  Show owning process details
  Get-Process -Id <PID>

  Kill process using port 3000
  Stop-Process -Id <PID> -Force

↑/↓ or w/s = select | Enter = run | e = copy/edit | q = cancel
```

- [x] Use `crossterm` raw mode.
- [x] Render 1-3 options.
- [x] Support selection keys:
  - [x] Up arrow = previous option.
  - [x] Down arrow = next option.
  - [x] `w` = previous option.
  - [x] `s` = next option.
  - [x] `Enter` = run.
  - [x] `e` = edit/insert only.
  - [x] `q` = cancel.
  - [x] `Esc` = cancel.
  - [x] `Ctrl+C` = cancel and restore terminal.

- [x] Always restore terminal state on exit.
- [x] Clear picker UI before returning to shell.
- [x] Print only final JSON to stdout in shell mode.

Final stdout examples:

Run:

```json
{"action":"run","command":"Get-NetTCPConnection -LocalPort 3000 | Select-Object LocalAddress,LocalPort,OwningProcess"}
```

Edit:

```json
{"action":"edit","command":"Get-NetTCPConnection -LocalPort 3000 | Select-Object LocalAddress,LocalPort,OwningProcess"}
```

Cancel:

```json
{"action":"cancel"}
```

---

## Phase 6 — PowerShell Wrapper

Goal: provide the actual native terminal UX.

User command:

```powershell
ai what is running on port 3000
```

Implementation idea:

```powershell
function ai {
  $prompt = $args -join " "

  if (-not $prompt.Trim()) {
    Write-Host "Usage: ai <what do you want to do?>"
    return
  }

  $json = ai-core --shell-mode -- $prompt

  if (-not $json) {
    return
  }

  $result = $json | ConvertFrom-Json

  if ($result.action -eq "cancel") {
    return
  }

  if ($result.action -eq "edit") {
    [Microsoft.PowerShell.PSConsoleReadLine]::Insert($result.command)
    return
  }

  if ($result.action -eq "run") {
    [Microsoft.PowerShell.PSConsoleReadLine]::AddToHistory($result.command)
    Write-Host "> $($result.command)" -ForegroundColor DarkGray
    Invoke-Expression $result.command
    return
  }
}
```

- [x] Add wrapper to `shell/powershell.ps1`.
- [x] Document how to add it to PowerShell profile.
- [ ] Verify `e` inserts into the next prompt.
- [x] Verify `Enter` runs in the current PowerShell context.
- [x] Verify commands like `cd` affect the current session.
- [x] Verify environment changes persist where PowerShell allows it.
- [ ] Verify command is added to PSReadLine history.
- [x] Print the command before running for transparency.

---

## Phase 7 — Safety Layer

Goal: reduce accidental destructive commands.

- [ ] Implement basic command risk classification in Rust.
- [ ] Use LLM-provided `risk`.
- [ ] Add local regex-based risk override.
- [ ] Dangerous pattern examples:
  - [ ] `Remove-Item`
  - [ ] `rm -rf`
  - [ ] `del /s`
  - [ ] `format`
  - [ ] `diskpart`
  - [ ] `git reset --hard`
  - [ ] `git clean -fd`
  - [ ] `Stop-Process`
  - [ ] `taskkill`
  - [ ] `docker system prune`
  - [ ] `DROP DATABASE`
  - [ ] `kubectl delete`
  - [ ] `terraform destroy`

- [ ] Mark risky option visually:

```txt
[dangerous]
```

- [ ] Require second confirmation for dangerous commands.
- [ ] Prefer safe diagnostic command as option 1.
- [ ] Never auto-run dangerous commands without explicit confirmation.
- [ ] Add config option:

```json
{
  "dangerous_requires_confirm": true
}
```

---

## Phase 8 — Shell Context Awareness

Goal: improve LLM output using local context.

PowerShell context:

- [ ] Current directory.
- [ ] Current git branch.
- [ ] Recent commands.
- [ ] OS version.
- [ ] Shell name/version.
- [ ] Whether current directory is a git repo.
- [ ] Visible package manager hints:
  - [ ] `package.json`
  - [ ] `pnpm-lock.yaml`
  - [ ] `package-lock.json`
  - [ ] `yarn.lock`
  - [ ] `Cargo.toml`
  - [ ] `go.mod`
  - [ ] `Dockerfile`
  - [ ] `docker-compose.yml`
  - [ ] `.env.example`

Example context:

```txt
OS: Windows
Shell: PowerShell 7
Current directory: C:\Users\spare\project
Git branch: feat/proxy-routing
Detected files: package.json, docker-compose.yml
Recent commands:
npm run dev
docker compose ps
```

- [ ] Add context into LLM prompt.
- [ ] Avoid sending secrets.
- [ ] Do not send full `.env` contents.
- [ ] Do not send full file contents by default.
- [ ] Add config:

```json
{
  "send_context": true,
  "send_recent_commands": true,
  "max_recent_commands": 5
}
```

---

## Phase 9 — Better UX Polish

- [ ] Add loading state:

```txt
thinking...
```

- [ ] Show API errors cleanly.
- [ ] Show invalid JSON errors cleanly.
- [ ] Add timeout handling.
- [ ] Add cancel during LLM request.
- [ ] Add option to regenerate.
- [ ] Add option to copy selected command.
- [ ] Add keybindings:
  - [ ] `r` = regenerate.
  - [ ] `c` = copy command.
  - [ ] `?` = show help.

- [ ] Add color-coded risk labels.
- [ ] Add config for max options.
- [ ] Add command history file.
- [ ] Add prompt history file.
- [ ] Add ability to disable telemetry entirely if any telemetry is ever considered.
- [ ] Default to no telemetry.

---

## Phase 10 — Unix Shell Support

Goal: keep Rust core, add shell wrappers.

### zsh

- [ ] Add `shell/zsh.zsh`.
- [ ] Support run mode with `eval`.
- [ ] Support edit mode with:

```zsh
print -z "$command"
```

### fish

- [ ] Add `shell/fish.fish`.
- [ ] Support run mode with `eval`.
- [ ] Support edit mode with:

```fish
commandline -i "$command"
```

### bash

- [ ] Add `shell/bash.sh`.
- [ ] Support run mode with `eval`.
- [ ] Investigate edit/insert support.
- [ ] Consider keybinding-based approach for better bash insert support.
- [ ] Document bash limitations honestly.

### Shared Unix tasks

- [ ] Detect current shell.
- [ ] Send shell info to LLM.
- [ ] Prefer POSIX-safe commands where possible.
- [ ] Use shell-specific commands when needed.
- [ ] Add install docs for Linux/macOS.

---

## Phase 11 — Ctrl+I Keybinding Mode

Goal: add Copilot-like keybinding later.

PowerShell:

- [ ] Investigate `Set-PSReadLineKeyHandler`.
- [ ] Bind `Ctrl+i` to open prompt UI.
- [ ] Allow prompt from empty terminal line.
- [ ] Allow selected command to be inserted into current PSReadLine buffer.
- [ ] Revisit command-mode edit UX:
  - [ ] Desired behavior: `ai <prompt>` -> pick option -> `e` places selected command into a fresh editable prompt line without running it.
  - [ ] Current command-mode fallback: `e` copies the selected command to clipboard for manual paste/edit.
  - [ ] Investigate whether this is possible outside a PSReadLine key handler without terminal repaint glitches.
  - [ ] Prefer keybinding mode if it is the only reliable way to own the active PSReadLine buffer.
- [ ] Add config toggle for keybinding install.
- [ ] Avoid overriding existing keybinding without confirmation.

Future UX:

```txt
Ctrl+i -> prompt box -> options -> insert/run
```

- [ ] Consider separate mode:

```powershell
ai-keybind
```

- [ ] Keep normal command mode working:

```powershell
ai what is running on port 3000
```

---

## Phase 12 — Packaging and Install

Windows:

- [ ] Build release binary:

```powershell
cargo build --release
```

- [ ] Add `ai-core.exe` to PATH.
- [ ] Add PowerShell profile installer.
- [ ] Add uninstall script.
- [ ] Add upgrade instructions.

Possible install command later:

```powershell
irm https://example.com/install.ps1 | iex
```

Linux/macOS:

- [ ] Build binaries for common targets.
- [ ] Add install shell script.
- [ ] Add Homebrew formula later if useful.
- [ ] Add Arch/AUR package later if useful.

General:

- [ ] Add GitHub Releases.
- [ ] Add checksums.
- [ ] Add signed binaries later.
- [ ] Add README badges.
- [ ] Add example GIF/demo.

---

## Phase 13 — Testing

Rust unit tests:

- [ ] Config parsing.
- [ ] LLM response parsing.
- [ ] Markdown-stripping parser.
- [ ] Safety classification.
- [ ] Prompt construction.
- [ ] Shell detection.

Integration tests:

- [ ] Mock LLM API.
- [ ] Verify stdout contains only JSON in shell mode.
- [ ] Verify stderr contains UI/logs.
- [ ] Verify cancel action.
- [ ] Verify edit action.
- [ ] Verify run action.
- [ ] Verify dangerous confirmation.

Manual tests on Windows:

- [ ] `ai what is running on port 3000`
- [ ] `ai kill process on port 3000`
- [ ] `ai list docker containers`
- [ ] `ai show git branches`
- [ ] `ai checkout new branch called test`
- [ ] `ai find large files in this folder`
- [ ] `ai compress video with ffmpeg`
- [ ] `ai create env file from env example`
- [ ] `ai undo last git commit but keep changes`

Manual tests for native behavior:

- [ ] `cd` command changes current PowerShell session directory.
- [ ] Env var command affects current session.
- [ ] `e` inserts command and allows editing.
- [ ] `Enter` runs and prints command before output.
- [ ] `q` cancels cleanly.
- [ ] Terminal mode is restored after `Ctrl+C`.

---

## Phase 14 — Documentation

README sections:

- [ ] What this project is.
- [ ] Demo.
- [ ] Installation.
- [ ] Configuration.
- [ ] PowerShell setup.
- [ ] Usage examples.
- [ ] Keybindings.
- [ ] Safety behavior.
- [ ] Supported shells.
- [ ] Known limitations.
- [ ] Troubleshooting.
- [ ] Development setup.

Example usage:

```powershell
ai what is running on port 3000
ai show docker logs for backend
ai find which process is using this file
ai create a tar archive of this folder
ai undo last git commit but keep changes
ai show postgres containers
```

Known limitations to document:

- [ ] LLM commands can be wrong.
- [ ] Dangerous commands require confirmation.
- [ ] Bash edit insertion may be limited.
- [ ] API latency depends on selected LLM provider.
- [ ] PowerShell wrapper is required for native insert/run behavior.

---

## Phase 15 — Future Ideas

- [ ] Local model support.
- [ ] Multiple provider support.
- [ ] Provider fallback.
- [ ] Command explanation mode.
- [ ] Dry-run mode.
- [ ] Project-specific instructions file:

```txt
.terminal-ai.md
```

- [ ] Allow repo-specific command style.
- [ ] Add per-shell prompt templates.
- [ ] Add command memory.
- [ ] Add command aliases.
- [ ] Add “fix last command error” mode.
- [ ] Capture last command output optionally.
- [ ] Add terminal overlay app later.
- [ ] Add Windows Terminal/VS Code terminal hotkey integration later.
- [ ] Add plugin system for common tools:
  - [ ] Docker
  - [ ] Git
  - [ ] npm/pnpm
  - [ ] PostgreSQL
  - [ ] Kubernetes
  - [ ] ffmpeg

---

## MVP Definition

The MVP is complete when:

- [ ] User can type:

```powershell
ai what is running on port 3000
```

- [ ] Rust binary calls LLM and returns 1-3 command options.
- [ ] User can select with arrow keys or `a`/`s`.
- [ ] `Enter` runs selected command natively in PowerShell.
- [ ] `e` inserts selected command into the prompt for editing.
- [ ] `q` or `Esc` cancels.
- [ ] Dangerous commands require second confirmation.
- [ ] Terminal state is restored after every exit path.
- [ ] README explains setup and usage.
