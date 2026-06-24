#!/usr/bin/env bash
#
# terminal-ai — Linux uninstaller
#
# Usage:
#   curl -fsSL https://terminal-ai.lab-node.me/uninstall.sh | bash
#
# Environment variables:
#   TERMINAL_AI_INSTALL_DIR   Install directory (default: ~/.local/share/terminal-ai)
#   TERMINAL_AI_CONFIG_DIR    Config directory (default: ~/.config/terminal-ai)
#   TERMINAL_AI_PROFILE_PATH  Profile file to clean (default: ~/.bashrc)
#   TERMINAL_AI_SKIP_PATH     Set to "1" to skip PATH cleanup
#   TERMINAL_AI_UNINSTALL_ALL Set to "1" to remove config without asking
#   TERMINAL_AI_UNINSTALL_KEEP_CONFIG  Set to "1" to keep config
#

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

INSTALL_ROOT="${TERMINAL_AI_INSTALL_DIR:-${HOME}/.local/share/terminal-ai}"
BIN_DIR="${INSTALL_ROOT}/bin"
CONFIG_ROOT="${TERMINAL_AI_CONFIG_DIR:-${XDG_CONFIG_HOME:-${HOME}/.config}/terminal-ai}"
PROFILE_PATH="${TERMINAL_AI_PROFILE_PATH:-${HOME}/.bashrc}"

MARKER_START='# >>> terminal-ai >>>'
MARKER_END='# <<< terminal-ai <<<'

REMOVED=()

# ---------------------------------------------------------------------------
# Safety: verify a path belongs to terminal-ai
# ---------------------------------------------------------------------------

path_is_terminal_ai() {
    local path="$1"
    local parent="$2"

    if [ ! -e "$path" ]; then
        return 1
    fi

    local resolved_path
    resolved_path="$(realpath -m "$path" 2>/dev/null)"
    local resolved_parent
    resolved_parent="$(realpath -m "$parent" 2>/dev/null)"

    local leaf
    leaf="$(basename "$resolved_path")"

    [ "$leaf" = "terminal-ai" ] && case "$resolved_path" in
        "$resolved_parent"/*) return 0 ;;
    esac

    return 1
}

remove_directory() {
    local path="$1"
    local parent="$2"

    if [ ! -e "$path" ]; then
        return
    fi

    if ! path_is_terminal_ai "$path" "$parent"; then
        printf 'Refusing to remove unexpected path: %s\n' "$path" >&2
        return 1
    fi

    rm -rf "$path"
    REMOVED+=("$path")
}

# ---------------------------------------------------------------------------
# Remove profile block
# ---------------------------------------------------------------------------

remove_profile_block() {
    local profile_path="$1"

    if [ ! -f "$profile_path" ]; then
        return
    fi

    local content
    content="$(cat "$profile_path")"

    local escaped_start
    escaped_start="$(printf '%s\n' "$MARKER_START" | sed 's/[.[\*^$()+?{|]/\\&/g')"
    local escaped_end
    escaped_end="$(printf '%s\n' "$MARKER_END" | sed 's/[.[\*^$()+?{|]/\\&/g')"

    local updated
    updated="$(printf '%s\n' "$content" | sed "/${escaped_start}/,/${escaped_end}/d")"
    updated="$(printf '%s\n' "$updated" | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}')"

    if [ "$updated" != "$(printf '%s\n' "$content" | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}')" ]; then
        printf '%s\n' "$updated" > "$profile_path"
        REMOVED+=("profile block from ${profile_path}")
    fi
}

# ---------------------------------------------------------------------------
# Remove from PATH in current session
# ---------------------------------------------------------------------------

remove_from_path() {
    local path_entry="$1"

    local path_entry_norm
    path_entry_norm="$(realpath -m "$path_entry" 2>/dev/null || echo "$path_entry")"

    local new_path=""
    IFS=':' read -ra path_parts <<< "${PATH:-}"
    for p in "${path_parts[@]}"; do
        local p_norm
        p_norm="$(realpath -m "$p" 2>/dev/null || echo "$p")"
        if [ "$p_norm" != "$path_entry_norm" ]; then
            [ -n "$new_path" ] && new_path="${new_path}:"
            new_path="${new_path}${p}"
        fi
    done

    if [ "$new_path" != "${PATH:-}" ]; then
        export PATH="$new_path"
        REMOVED+=("PATH entry ${path_entry}")
    fi
}

# ---------------------------------------------------------------------------
# Ask about removing user data
# ---------------------------------------------------------------------------

should_remove_user_data() {
    if [ "${TERMINAL_AI_UNINSTALL_ALL:-}" = "1" ]; then
        return 0
    fi

    if [ "${TERMINAL_AI_UNINSTALL_KEEP_CONFIG:-}" = "1" ]; then
        return 1
    fi

    printf 'Remove terminal-ai config and local history at %s? [y/N] ' "$CONFIG_ROOT" >&2
    read -r answer
    case "$answer" in
        y|Y|yes|YES) return 0 ;;
        *) return 1 ;;
    esac
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

remove_profile_block "$PROFILE_PATH"

if [ "${TERMINAL_AI_SKIP_PATH:-}" != "1" ]; then
    remove_from_path "$BIN_DIR"
fi

remove_directory "$INSTALL_ROOT" "${HOME}/.local/share"

if should_remove_user_data; then
    local config_parent
    config_parent="$(dirname "$CONFIG_ROOT")"
    remove_directory "$CONFIG_ROOT" "$config_parent"
fi

if [ "${#REMOVED[@]}" -eq 0 ]; then
    printf 'terminal-ai was not installed, or it was already removed.\n'
else
    printf 'Removed:\n'
    for item in "${REMOVED[@]}"; do
        printf '  %s\n' "$item"
    done
fi