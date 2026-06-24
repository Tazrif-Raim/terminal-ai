# shell/bash.sh - Terminal AI Bash wrapper
# Source this from your ~/.bashrc:
#   source /path/to/terminal-ai/shell/bash.sh

_TERMINAL_AI_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd 2>/dev/null)"

# ---------------------------------------------------------------------------
# Help
# ---------------------------------------------------------------------------

_ai_help() {
    cat <<'HELP'
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
  ai see these files --files README.md docs/TODO.md
  ai --config
HELP
}

# ---------------------------------------------------------------------------
# Locate ai-core binary
# ---------------------------------------------------------------------------

_ai_find_core() {
    # 1. Repo debug build (development mode)
    local repo_manifest="${_TERMINAL_AI_ROOT}/ai-core/Cargo.toml"
    local repo_binary="${_TERMINAL_AI_ROOT}/ai-core/target/debug/ai-core"
    if [ -f "$repo_manifest" ] && [ -f "$repo_binary" ]; then
        printf '%s\n' "$repo_binary"
        return 0
    fi

    # 2. Installed alongside wrapper
    local installed_binary="${_TERMINAL_AI_ROOT}/bin/ai-core"
    if [ -f "$installed_binary" ]; then
        printf '%s\n' "$installed_binary"
        return 0
    fi

    # 3. On PATH
    local path_core
    path_core="$(command -v ai-core 2>/dev/null)" || return 1
    printf '%s\n' "$path_core"
    return 0
}

# ---------------------------------------------------------------------------
# JSON field extraction (jq -> sed fallback)
# ---------------------------------------------------------------------------

_ai_parse_json() {
    local json="$1"
    local field="$2"

    # jq — best option
    if command -v jq &>/dev/null; then
        local val
        val="$(printf '%s\n' "$json" | jq -r ".${field}" 2>/dev/null)"
        [ -n "$val" ] && [ "$val" != "null" ] && printf '%s\n' "$val" && return 0
    fi

    # sed fallback — extracts "field":"value" pairs
    case "$field" in
        action)
            printf '%s\n' "$json" | sed -n 's/.*"action"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p'
            ;;
        command)
            printf '%s\n' "$json" | sed -n 's/.*"command"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p'
            ;;
        title)
            printf '%s\n' "$json" | sed -n 's/.*"title"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p'
            ;;
    esac
}

# ---------------------------------------------------------------------------
# Environment setup / teardown
# ---------------------------------------------------------------------------

_ai_set_env() {
    _AI_SAVED_DOTENV="${TERMINAL_AI_DOTENV_PATH:-}"
    _AI_SAVED_SHELL_NAME="${TERMINAL_AI_SHELL_NAME:-}"
    _AI_SAVED_SHELL_VERSION="${TERMINAL_AI_SHELL_VERSION:-}"
    _AI_SAVED_OS_VERSION="${TERMINAL_AI_OS_VERSION:-}"
    _AI_SAVED_RECENT_COMMANDS="${TERMINAL_AI_RECENT_COMMANDS:-}"

    if [ -z "${TERMINAL_AI_DOTENV_PATH:-}" ]; then
        export TERMINAL_AI_DOTENV_PATH="${_TERMINAL_AI_ROOT}/.env"
    fi

    export TERMINAL_AI_SHELL_NAME="bash"
    export TERMINAL_AI_SHELL_VERSION="${BASH_VERSION:-}"

    local os=""
    if [ -f /etc/os-release ]; then
        os="$(sed -n 's/PRETTY_NAME="\([^"]*\)".*/\1/p' /etc/os-release 2>/dev/null)"
    fi
    [ -z "$os" ] && os="$(uname -s)"
    export TERMINAL_AI_OS_VERSION="$os"

    # Recent commands from history file (filter out ai invocations)
    local hist_file="${HISTFILE:-$HOME/.bash_history}"
    if [ -n "$hist_file" ] && [ -f "$hist_file" ] && [ -r "$hist_file" ]; then
        local recent
        recent="$(tail -n 30 "$hist_file" 2>/dev/null | grep -vE '^\s*ai\b' | tail -n 20)"
        if [ -n "$recent" ]; then
            export TERMINAL_AI_RECENT_COMMANDS="$recent"
        fi
    fi
}

_ai_restore_env() {
    if [ -n "$_AI_SAVED_DOTENV" ]; then
        export TERMINAL_AI_DOTENV_PATH="$_AI_SAVED_DOTENV"
    else
        unset TERMINAL_AI_DOTENV_PATH 2>/dev/null || true
    fi
    if [ -n "$_AI_SAVED_SHELL_NAME" ]; then
        export TERMINAL_AI_SHELL_NAME="$_AI_SAVED_SHELL_NAME"
    else
        unset TERMINAL_AI_SHELL_NAME 2>/dev/null || true
    fi
    if [ -n "$_AI_SAVED_SHELL_VERSION" ]; then
        export TERMINAL_AI_SHELL_VERSION="$_AI_SAVED_SHELL_VERSION"
    else
        unset TERMINAL_AI_SHELL_VERSION 2>/dev/null || true
    fi
    if [ -n "$_AI_SAVED_OS_VERSION" ]; then
        export TERMINAL_AI_OS_VERSION="$_AI_SAVED_OS_VERSION"
    else
        unset TERMINAL_AI_OS_VERSION 2>/dev/null || true
    fi
    if [ -n "$_AI_SAVED_RECENT_COMMANDS" ]; then
        export TERMINAL_AI_RECENT_COMMANDS="$_AI_SAVED_RECENT_COMMANDS"
    else
        unset TERMINAL_AI_RECENT_COMMANDS 2>/dev/null || true
    fi

    unset _AI_SAVED_DOTENV _AI_SAVED_SHELL_NAME _AI_SAVED_SHELL_VERSION \
          _AI_SAVED_OS_VERSION _AI_SAVED_RECENT_COMMANDS
}

# ---------------------------------------------------------------------------
# Action handlers
# ---------------------------------------------------------------------------

_ai_copy_command() {
    local cmd="$1"

    if command -v xclip &>/dev/null; then
        printf '%s' "$cmd" | xclip -selection clipboard
    elif command -v xsel &>/dev/null; then
        printf '%s' "$cmd" | xsel --clipboard --input
    elif command -v wl-copy &>/dev/null; then
        printf '%s' "$cmd" | wl-copy
    elif command -v clip.exe &>/dev/null; then
        printf '%s' "$cmd" | clip.exe
    else
        printf '%s\n' "Could not copy to clipboard (install xclip, xsel, or wl-clipboard)." >&2
        printf '%s\n' "$cmd"
        return 0
    fi

    printf '%s\n' "Copied command, paste it into the next prompt to edit or run:"
    printf '%s\n' "$cmd"
}

_ai_edit_command() {
    local cmd="$1"

    if [ ! -t 0 ]; then
        _ai_copy_command "$cmd"
        return $?
    fi

    printf '> '
    local edited
    IFS= read -r -e -i "$cmd" edited 2>/dev/null
    if [ -z "$edited" ]; then
        return 0
    fi

    eval "$edited"
}

_ai_run_command() {
    local cmd="$1"
    printf '> %s\n' "$cmd"
    eval "$cmd"
}

# ---------------------------------------------------------------------------
# Main ai function
# ---------------------------------------------------------------------------

ai() {
    local agent_mode=false
    local dry_run_mode=false
    local agent_logs_mode=false
    local prompt_args=()
    local files_args=()
    local parsing_files=false
    local files_done=false

    # Parse flags
    while [ $# -gt 0 ]; do
        case "$1" in
            --agent-logs)
                agent_logs_mode=true
                shift
                ;;
            --agent)
                agent_mode=true
                shift
                ;;
            --dry-run)
                dry_run_mode=true
                shift
                ;;
            --help)
                _ai_help
                return 0
                ;;
            --version)
                local core
                core="$(_ai_find_core)" || {
                    printf '%s\n' "ai-core was not found. Build ai-core and add it to PATH." >&2
                    return 1
                }
                local version
                version="$("$core" --version 2>/dev/null)" || return 1
                printf '%s\n' "${version/ai-core /terminal-ai }"
                return 0
                ;;
            --config)
                local core
                core="$(_ai_find_core)" || {
                    printf '%s\n' "ai-core was not found. Build ai-core and add it to PATH." >&2
                    return 1
                }
                _ai_set_env
                "$core" --config
                _ai_restore_env
                return 0
                ;;
            --files)
                parsing_files=true
                shift
                ;;
            --)
                if [ "$parsing_files" = true ] && [ "$files_done" = false ]; then
                    files_done=true
                    parsing_files=false
                fi
                shift
                ;;
            *)
                if [ "$parsing_files" = true ] && [ "$files_done" = false ]; then
                    files_args+=("$1")
                else
                    prompt_args+=("$1")
                fi
                shift
                ;;
        esac
    done

    # --agent-logs mode
    if [ "$agent_logs_mode" = true ]; then
        local core
        core="$(_ai_find_core)" || {
            printf '%s\n' "ai-core was not found. Build ai-core and add it to PATH." >&2
            return 1
        }
        local open_arg=""
        [ "${#prompt_args[@]}" -gt 0 ] && [ "${prompt_args[0]}" = "open" ] && open_arg="open"
        "$core" --agent-logs $open_arg
        return $?
    fi

    # Build prompt string
    local prompt=""
    if [ "${#prompt_args[@]}" -gt 0 ]; then
        local IFS=' '
        prompt="${prompt_args[*]}"
        unset IFS
    fi

    if [ -z "$prompt" ]; then
        _ai_help
        return 0
    fi

    local core
    core="$(_ai_find_core)" || {
        printf '%s\n' "ai-core was not found. Build ai-core and add it to PATH." >&2
        return 1
    }

    # Build ai-core arguments
    local ai_core_args=()

    if [ "$agent_mode" = true ]; then
        ai_core_args+=("--agent")
        [ "$dry_run_mode" = true ] && ai_core_args+=("--dry-run")
    else
        ai_core_args+=("--shell-mode")
    fi

    if [ "${#files_args[@]}" -gt 0 ]; then
        ai_core_args+=("--files" "${files_args[@]}")
    fi

    ai_core_args+=("--" "$prompt")

    # Set environment and run
    _ai_set_env

    if [ "$agent_mode" = true ]; then
        "$core" "${ai_core_args[@]}"
        local exit_code=$?
        _ai_restore_env
        return $exit_code
    fi

    # Shell mode: capture JSON output
    local json
    json="$("$core" "${ai_core_args[@]}")"
    local exit_code=$?
    _ai_restore_env

    if [ $exit_code -ne 0 ] || [ -z "$json" ]; then
        return $exit_code
    fi

    # Parse result
    local action command
    action="$(_ai_parse_json "$json" "action")"
    command="$(_ai_parse_json "$json" "command")"

    case "$action" in
        cancel)
            return 0
            ;;
        edit)
            [ -n "$command" ] && _ai_edit_command "$command"
            return $?
            ;;
        copy)
            [ -n "$command" ] && _ai_copy_command "$command"
            return $?
            ;;
        run)
            [ -n "$command" ] && _ai_run_command "$command"
            return $?
            ;;
        *)
            printf '%s\n' "ai-core returned an unknown action: $action" >&2
            return 1
            ;;
    esac
}
