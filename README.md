# terminal-ai

Terminal-native AI command assistant for Bash and PowerShell.

## Install / Update (Windows)

```powershell
irm https://terminal-ai.lab-node.me/powershell.ps1 | iex
```

Uninstall:

```powershell
irm https://terminal-ai.lab-node.me/uninstall.ps1 | iex
```

## Install / Update (Linux)

```bash
curl -fsSL https://terminal-ai.lab-node.me/install.sh | bash
```

Uninstall:

```bash
curl -fsSL https://terminal-ai.lab-node.me/uninstall.sh | bash
```

## Usage

```powershell
# Ask a question / run a command
ai what is running on port 3000

# Run in agent mode (multi-step autonomous execution)
ai --agent setup a new React project with TypeScript

# Dry-run agent mode (preview steps without executing)
ai --agent --dry-run refactor the auth module

# Include file contents in context
ai --agent how to run the app --files README.md

# View recent agent audit logs
ai --agent-logs
```

## PowerShell Setup

If you installed via the Linux installer, the `ai` command is available after sourcing the wrapper or adding it to your PATH. The wrapper is located at `~/.local/share/terminal-ai/shell/bash.sh`.


Build the Rust binary:

```powershell
cargo build --manifest-path .\ai-core\Cargo.toml
```

Load the wrapper in the current PowerShell session:

```powershell
. .\shell\powershell.ps1
```

To load automatically, add to your PowerShell profile:

```powershell
. E:\personal\terminal-ai\shell\powershell.ps1
```

## Picker Controls

| Key | Action |
|-----|--------|
| `Enter` | Run selected command |
| `e` | Edit command before running |
| `c` | Copy command to clipboard |
| `r` | Regenerate options |
| `q` / `Esc` / `Ctrl+C` | Cancel |

## Agent Mode

- `--agent` enables multi-step autonomous execution
- `--dry-run` previews steps without executing
- `--agent-logs` lists recent runs; `open` opens the latest log
- Background processes (dev servers, watchers) are tracked and you're prompted to keep/kill them on exit

## Development

Create a local `.env` from the root template to keep provider settings near the
project while developing:

```powershell
Copy-Item .env.example .env
```

The PowerShell wrapper points `ai-core` at this root `.env` automatically. Real
process environment variables still take priority over `.env` values.

The Rust binary reads these values:

```txt
LLM_API_URL
LLM_API_KEY
LLM_MODEL
```

Optional config-file fields include `max_options`, `request_timeout_seconds`,
`hide_descriptions`, `send_context`, `send_recent_commands`, and
`max_recent_commands`.

```powershell
cd ai-core
cargo check
```
