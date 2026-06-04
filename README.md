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

Phase 1 scaffold is in progress:

- `ai-core/` contains the Rust binary crate.
- `shell/` contains shell wrapper placeholders.
- `docs/TODO.md` tracks the implementation phases.

## Development

```powershell
cd ai-core
cargo check
```
