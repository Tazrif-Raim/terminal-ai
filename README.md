# terminal-ai

Terminal-native AI command assistant.

The intended user-facing command is:

```powershell
ai what is running on port 3000
```

The shell wrapper will own native terminal behavior such as inserting or running
the selected command in the current shell session. The Rust binary, `ai-core`,
will own argument parsing, LLM calls, command option parsing, safety checks, and
the picker UI.

Primary target for the first MVP is Windows Terminal with PowerShell. The project
is structured so bash, zsh, fish, Linux, and macOS support can be added without
rewriting the Rust core.

## Current State

Phase 6 PowerShell flow is in progress:

- `ai-core/` contains the Rust binary crate.
- `shell/powershell.ps1` contains the current PowerShell wrapper.
- Other shell wrappers are placeholders for later phases.
- `docs/TODO.md` tracks the implementation phases.

## PowerShell Setup

Build the Rust binary:

```powershell
cargo build --manifest-path .\ai-core\Cargo.toml
```

Load the wrapper in the current PowerShell session:

```powershell
. .\shell\powershell.ps1
```

To load it automatically, add this line to your PowerShell profile:

```powershell
. E:\personal\terminal-ai\shell\powershell.ps1
```

Usage:

```powershell
ai what is running on port 3000
ai summarize these files --files README.md docs\TODO.md
```

`Enter` in the picker runs the selected command in the current PowerShell
session. `e` copies the selected command to your clipboard so you can paste,
edit, and run it manually. `q` and `Esc` cancel.

By default, `ai-core` sends lightweight local context to the LLM: current
directory, shell/OS metadata, git branch/repo status, recent commit hashes,
detected project files, and recent commands with likely secrets filtered out.
File contents are not sent unless explicitly passed with `--files`; obvious
secret files such as `.env` are skipped.

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

```powershell
cd ai-core
cargo check
```
