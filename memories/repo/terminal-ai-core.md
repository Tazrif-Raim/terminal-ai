# Terminal AI Core (ai-core)

## Config System
- `FileConfig` - JSON file stored at `~/.config/terminal-ai/config.json`
- `AiMode` enum: `OpenAiCompatible` (default) or `CodexOAuth`
- Config resolution: environment variables > .env file > file config
- `ResolvedConfig` is the fully-resolved config ready for use

## AI Modes
### OpenAI Compatible API Key
- Standard flow: user provides api_url, api_key, model
- Used when `ai_mode = "openai-compatible"` in config

### Codex OAuth (Experimental)
- PKCE OAuth login via `codex-oauth` crate against `auth.openai.com`
- Token stored as JSON string in `codex_token` config field
- Auto-refreshes on load when expired via `codex_oauth::refresh()`
- Uses `tokio::runtime::Runtime::block_on` for async calls in sync context
- Default API URL: `https://api.openai.com/v1/chat/completions`
- Default model: `gpt-4o`
- Port 1455 must be free (hardcoded by OpenAI's app registration)

## Config Menu (`ai --config`)
- Interactive TUI with 3 options: OpenAI compatible, Codex OAuth, Exit
- Active mode marked with `●`
- Navigation: ↑/↓ arrows or j/k keys
- Enter to select, Esc/q to exit
- Codex OAuth model prompt: uses `prompt_single_value()` which now requires a non-empty value when there's no existing model (fixes silent "not saved" bug)
- Env/.env override warning: after interactive save, both Codex OAuth and OpenAI API key paths now detect when `LLM_MODEL`/`LLM_API_URL`/`LLM_API_KEY` env vars or .env values will override the saved value and print a clear warning

## Key Dependencies
- `codex-oauth = "=0.1.1"` - OAuth login for OpenAI Codex
- `tokio = { features = ["rt-multi-thread", "macros"] }` - async runtime for codex-oauth


## Linux/Bash Support (implemented 2026-06-24)

### Files created/modified:
- **shell/bash.sh** — Bash shell wrapper (`.bashrc`). Handles --help, --version, --config, --agent, --agent-logs, --dry-run, --files flags. Uses `jq`->`sed` for JSON, `xclip`/`xsel`/`wl-copy`/`clip.exe` for clipboard, `read -e -i` for editing.
- **shell/zsh.zsh** — Zsh shell wrapper (`.zshrc`). Handles --help, --version, --config, --agent, --agent-logs, --dry-run, --files flags. Uses `jq`->`python3` for JSON, `xclip`/`xsel`/`wl-copy`/`clip.exe` for clipboard, `vared` for editing.
- **install/bash.sh** — Linux installer (`curl ... | bash`). Detects user's shell (`$SHELL`), installs appropriate wrapper to appropriate profile (`~/.bashrc` or `~/.zshrc`), adds bin to PATH.
- **install/uninstall.sh** — Linux uninstaller. Detects user's shell, cleans profile block from both `~/.bashrc` and `~/.zshrc`, removes install dir, optionally removes config.
- **scripts/package-linux.sh** — Linux packaging. Builds release binary, creates `releases/<ver>/linux-x64/` with checksums for ai-core, bash.sh, and zsh.zsh.
- **scripts/merge-release.ps1** — PowerShell Core merge script. Merges Windows + Linux artifacts, updates version.json with `linux_x64` entry including bash_wrapper and zsh_wrapper.
- **.github/workflows/release.yml** — Added `build-linux` job, `package-site` merge job, curl smoke tests for `install.sh`/`uninstall.sh`/`zsh.zsh`.
- **web/src/components/InstallSection.tsx** — Side-by-side PowerShell/Bash install cards.
- **web/src/components/UninstallSection.tsx** — Side-by-side PowerShell/Bash uninstall cards.

### Review findings (2026-06-24 post-commit review):
- **All core logic is correct** — binary discovery, flag parsing, install/uninstall flow, CI pipeline, version.json merge.
- **Trailing newlines were missing** in 4 files (`install/bash.sh`, `install/uninstall.sh`, `scripts/package-linux.sh`, `scripts/merge-release.ps1`) — fixed by appending LF byte.
- **Web UI double-render** — `App.tsx` used a custom `useDetectedShell()` hook with `useEffect` + a second `useEffect` to sync state, causing a flash on Windows. Simplified to synchronous `getDefaultShell()` passed as `useState` initializer.
- **GNU sed dependency** — `install/bash.sh` and `install/uninstall.sh` use GNU sed label/branch syntax (`sed -e :a -e ...`). Linux-only target, so acceptable.
- **jq/python3 dependency for manifest parsing** — `install/bash.sh` uses jq/python3 for reliable JSON parsing instead of sed-based regex. Falls back to sed for `get_local_version` (local file check) with `head -1` guard.

### Bugfix 2026-06-27: merge-release.ps1 wildcard path resolution
- **Root cause**: `merge-release.ps1` used `Join-Path $LinuxDir 'releases' | Join-Path -ChildPath '*'` then searched for `*/checksums.txt` — the `*` only matched one directory level, but the actual path is `releases/<version>/linux-x64/checksums.txt` (two levels deep). Result: `$linuxChecksums` was always `$null`, so `linux_x64` was never added to `version.json`.
- **Fix**: Changed to `[System.IO.Path]::Combine($LinuxDir, 'releases', $version, 'linux-x64', 'checksums.txt')` using the version from the manifest. Also added a warning when the file is not found.

### Bugfix 2026-06-28: install/bash.sh sed-based manifest parsing
- **Root cause**: `parse_manifest()` used sed-based regex (`s/.*"version".../\\1/p`) to extract the top-level `version` field from version.json. This was fragile (line-based, no JSON structure awareness) and inconsistent with `extract_asset_field()` which already used jq/python3 proper parsers.
- **Fix**: Replaced `parse_manifest()` with `get_manifest_version()` using jq or python3 (same approach as `extract_asset_field`). Also updated `get_local_version()` to prefer jq/python3 first, with sed as a fallback with `head -1` guard.

### URL structure:
- `/install.sh` — Linux installer (from install/bash.sh)
- `/uninstall.sh` — Linux uninstaller (from install/uninstall.sh)
- `/releases/<version>/linux-x64/ai-core` — Linux binary
- `/releases/<version>/linux-x64/shell/bash.sh` — Bash wrapper
- `/releases/<version>/linux-x64/shell/zsh.zsh` — Zsh wrapper
- `/releases/<version>/linux-x64/checksums.txt` — SHA256 checksums

### version.json format:
```json
{ "version": "...", "channel": "stable",
  "windows_x64": { "ai_core": {...}, "powershell_wrapper": {...} },
  "linux_x64": { "ai_core": {...}, "bash_wrapper": {...}, "zsh_wrapper": {...} }
}
```

### manifest parsing (install/bash.sh):
Uses `get_manifest_version` (top-level) and `extract_asset_field <section> <field>` (nested) with jq/python3 to parse version.json. Falls back to sed for `get_local_version` (local file) when neither jq nor python3 is available, with `head -1` guard.