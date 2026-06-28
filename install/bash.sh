#!/usr/bin/env bash
#
# terminal-ai — Linux installer
#
# Usage:
#   curl -fsSL https://terminal-ai.lab-node.me/install.sh | bash
#
# Environment variables:
#   TERMINAL_AI_BASE_URL    Base URL for downloads (default: https://terminal-ai.lab-node.me)
#   TERMINAL_AI_INSTALL_DIR Install directory (default: ~/.local/share/terminal-ai)
#   TERMINAL_AI_PROFILE_PATH  Profile file to modify (default: ~/.bashrc)
#   TERMINAL_AI_SKIP_PATH   Set to "1" to skip PATH modification
#

# ---------------------------------------------------------------------------
# Validation helper — defined first so it is always in scope
# ---------------------------------------------------------------------------

require() {
    if [ -z "${2:-}" ]; then
        printf 'Invalid manifest: missing %s.\n' "$1" >&2
        exit 1
    fi
}

set -euo pipefail

# ---------------------------------------------------------------------------
# Preflight: require python3 or jq for JSON parsing
# ---------------------------------------------------------------------------

if ! command -v python3 >/dev/null 2>&1 && ! command -v jq >/dev/null 2>&1; then
    printf 'terminal-ai installer requires python3 or jq to parse the release manifest.\n' >&2
    printf 'Install one of them and re-run:\n' >&2
    printf '  sudo apt-get install -y python3\n' >&2
    printf '  sudo apt-get install -y jq\n' >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

TERMINAL_AI_BASE_URL="${TERMINAL_AI_BASE_URL:-https://terminal-ai.lab-node.me}"
TERMINAL_AI_BASE_URL="${TERMINAL_AI_BASE_URL%/}"

INSTALL_ROOT="${TERMINAL_AI_INSTALL_DIR:-${HOME}/.local/share/terminal-ai}"
BIN_DIR="${INSTALL_ROOT}/bin"
SHELL_DIR="${INSTALL_ROOT}/shell"
STATE_DIR="${INSTALL_ROOT}/state"
LOCAL_MANIFEST="${INSTALL_ROOT}/version.json"
WRAPPER_PATH="${SHELL_DIR}/bash.sh"
AI_CORE_PATH="${BIN_DIR}/ai-core"

PROFILE_PATH="${TERMINAL_AI_PROFILE_PATH:-${HOME}/.bashrc}"

MARKER_START='# >>> terminal-ai >>>'
MARKER_END='# <<< terminal-ai <<<'

# ---------------------------------------------------------------------------
# Helper functions
# ---------------------------------------------------------------------------

join_url() {
    local base="$1"
    local path="$2"
    if [[ "$path" =~ ^https?:// ]]; then
        echo "$path"
    else
        echo "${base%/}/${path#/}"
    fi
}

download() {
    local url="$1"
    local out="$2"
    curl \
        --fail \
        --show-error \
        --silent \
        --location \
        --retry 3 \
        --retry-delay 1 \
        "$url" \
        -o "$out"
}

verify_hash() {
    local file="$1"
    local expected="$2"
    local actual
    actual="$(sha256sum "$file" | cut -d' ' -f1)"
    if [ "$actual" != "$expected" ]; then
        printf 'Checksum mismatch for %s.\nExpected: %s\nActual:   %s\n' "$file" "$expected" "$actual" >&2
        return 1
    fi
}

check_hash() {
    local file="$1"
    local expected="$2"
    if [ ! -f "$file" ]; then
        return 1
    fi
    local actual
    actual="$(sha256sum "$file" | cut -d' ' -f1)"
    [ "$actual" = "$expected" ]
}

# ---------------------------------------------------------------------------
# JSON parsing — python3 first, jq fallback
# ---------------------------------------------------------------------------

# Parse a top-level string field from $MANIFEST_JSON.
# Usage: _parse_json_field <field>
_parse_json_field() {
    local field="$1"

    if command -v python3 >/dev/null 2>&1; then
        python3 -c "
import json, sys
m = json.loads(sys.argv[1])
v = m.get(sys.argv[2], '')
if v:
    print(v)
" "$MANIFEST_JSON" "$field" 2>/dev/null
        return
    fi

    # jq fallback
    printf '%s' "$MANIFEST_JSON" | jq -r --arg f "$field" '.[$f] // empty'
}

# Parse a nested field: manifest[asset_key][section][field]
# Usage: _parse_asset_field <section> <field>
_parse_asset_field() {
    local section="$1"
    local field="$2"

    if command -v python3 >/dev/null 2>&1; then
        python3 -c "
import json, sys
m = json.loads(sys.argv[1])
try:
    print(m[sys.argv[2]][sys.argv[3]][sys.argv[4]])
except KeyError:
    pass
" "$MANIFEST_JSON" "$ASSET_KEY" "$section" "$field" 2>/dev/null
        return
    fi

    # jq fallback
    printf '%s' "$MANIFEST_JSON" | \
        jq -r --arg a "$ASSET_KEY" \
              --arg s "$section" \
              --arg f "$field" \
              '.[$a][$s][$f] // empty'
}

# ---------------------------------------------------------------------------
# Local version reader (reads already-installed version.json, not manifest)
# ---------------------------------------------------------------------------

get_local_version() {
    if [ ! -f "$LOCAL_MANIFEST" ]; then
        echo ""
        return
    fi

    if command -v python3 >/dev/null 2>&1; then
        python3 -c "
import json, sys
with open(sys.argv[1]) as f:
    print(json.load(f).get('version', ''))
" "$LOCAL_MANIFEST" 2>/dev/null || echo ""
        return
    fi

    if command -v jq >/dev/null 2>&1; then
        jq -r '.version // empty' "$LOCAL_MANIFEST" 2>/dev/null || echo ""
        return
    fi

    echo ""
}

# ---------------------------------------------------------------------------
# PATH helpers
# ---------------------------------------------------------------------------

add_to_path() {
    local path_entry="$1"

    local already_in_path=false
    local path_entry_norm
    path_entry_norm="$(realpath -m "$path_entry" 2>/dev/null || echo "$path_entry")"

    IFS=':' read -ra path_parts <<< "${PATH:-}"
    for p in "${path_parts[@]}"; do
        local p_norm
        p_norm="$(realpath -m "$p" 2>/dev/null || echo "$p")"
        if [ "$p_norm" = "$path_entry_norm" ]; then
            already_in_path=true
            break
        fi
    done

    if [ "$already_in_path" = true ]; then
        return 1
    fi

    export PATH="${path_entry}:${PATH}"
    return 0
}

add_profile_block() {
    local profile_path="$1"
    local wrapper_path="$2"

    local profile_dir
    profile_dir="$(dirname "$profile_path")"
    if [ -n "$profile_dir" ]; then
        mkdir -p "$profile_dir"
    fi

    local content=""
    if [ -f "$profile_path" ]; then
        content="$(cat "$profile_path")"
    fi

    # Remove existing block
    local pattern_start
    pattern_start="$(printf '%s\n' "$MARKER_START" | sed 's/[.[\*^$()+?{|]/\\&/g')"
    local pattern_end
    pattern_end="$(printf '%s\n' "$MARKER_END" | sed 's/[.[\*^$()+?{|]/\\&/g')"
    content="$(printf '%s\n' "$content" | sed "/${pattern_start}/,/${pattern_end}/d")"
    content="$(printf '%s\n' "$content" | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}')"

    local block
    block="$(printf '%s\n' "$MARKER_START" \
        "source \"$wrapper_path\"" \
        "$MARKER_END")"

    if [ -z "$content" ]; then
        printf '%s\n' "$block" > "$profile_path"
    else
        printf '%s\n%s\n\n%s\n' "$content" "" "$block" > "$profile_path"
    fi
}

# ---------------------------------------------------------------------------
# Arch detection
# ---------------------------------------------------------------------------

ARCH="$(uname -m)"
case "$ARCH" in
    x86_64)
        ASSET_KEY="linux_x64"
        ;;
    aarch64|arm64)
        ASSET_KEY="linux_arm64"
        ;;
    *)
        printf 'terminal-ai Linux MVP currently supports x86_64 and aarch64. Detected: %s\n' "$ARCH" >&2
        exit 1
        ;;
esac

# ---------------------------------------------------------------------------
# Create directories
# ---------------------------------------------------------------------------

mkdir -p "$BIN_DIR" "$SHELL_DIR" "$STATE_DIR"

# ---------------------------------------------------------------------------
# Fetch manifest
# ---------------------------------------------------------------------------

MANIFEST_URL="$(join_url "$TERMINAL_AI_BASE_URL" "/version.json")"
if ! MANIFEST_JSON="$(curl -fsSL "$MANIFEST_URL")"; then
    printf 'Failed to download manifest: %s\n' "$MANIFEST_URL" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# Parse manifest fields
# ---------------------------------------------------------------------------

MANIFEST_VERSION="$(_parse_json_field "version")"
require "version" "$MANIFEST_VERSION"

AI_CORE_URL="$(_parse_asset_field "ai_core" "url")"
AI_CORE_SHA="$(_parse_asset_field "ai_core" "sha256")"
BASH_WRAPPER_URL="$(_parse_asset_field "bash_wrapper" "url")"
BASH_WRAPPER_SHA="$(_parse_asset_field "bash_wrapper" "sha256")"

require "linux_x64.ai_core.url" "$AI_CORE_URL"
require "linux_x64.ai_core.sha256" "$AI_CORE_SHA"
require "linux_x64.bash_wrapper.url" "$BASH_WRAPPER_URL"
require "linux_x64.bash_wrapper.sha256" "$BASH_WRAPPER_SHA"

# ---------------------------------------------------------------------------
# Check if already up to date
# ---------------------------------------------------------------------------

LOCAL_VERSION="$(get_local_version)"
IS_CURRENT=false

if [ -n "$LOCAL_VERSION" ] && [ "$LOCAL_VERSION" = "$MANIFEST_VERSION" ]; then
    if [ -f "$AI_CORE_PATH" ] && [ -f "$WRAPPER_PATH" ]; then
        if check_hash "$AI_CORE_PATH" "$AI_CORE_SHA" && check_hash "$WRAPPER_PATH" "$BASH_WRAPPER_SHA"; then
            IS_CURRENT=true
        fi
    fi
fi

# ---------------------------------------------------------------------------
# Download and install
# ---------------------------------------------------------------------------

if [ "$IS_CURRENT" = false ]; then
    TEMP_DIR="$(mktemp -d "/tmp/terminal-ai-install-XXXXXX")"

    cleanup() {
        rm -rf "$TEMP_DIR"
    }
    trap cleanup EXIT

    AI_CORE_DOWNLOAD="${TEMP_DIR}/ai-core"
    WRAPPER_DOWNLOAD="${TEMP_DIR}/bash.sh"

    download "$(join_url "$TERMINAL_AI_BASE_URL" "$AI_CORE_URL")" "$AI_CORE_DOWNLOAD"
    download "$(join_url "$TERMINAL_AI_BASE_URL" "$BASH_WRAPPER_URL")" "$WRAPPER_DOWNLOAD"

    verify_hash "$AI_CORE_DOWNLOAD" "$AI_CORE_SHA"
    verify_hash "$WRAPPER_DOWNLOAD" "$BASH_WRAPPER_SHA"

    chmod +x "$AI_CORE_DOWNLOAD"
    cp "$AI_CORE_DOWNLOAD" "$AI_CORE_PATH"
    cp "$WRAPPER_DOWNLOAD" "$WRAPPER_PATH"
    printf '%s\n' "$MANIFEST_JSON" > "$LOCAL_MANIFEST"

    trap - EXIT
    rm -rf "$TEMP_DIR"
fi

# ---------------------------------------------------------------------------
# Add to profile and PATH
# ---------------------------------------------------------------------------

PATH_CHANGED=false
if [ "${TERMINAL_AI_SKIP_PATH:-}" != "1" ]; then
    if add_to_path "$BIN_DIR"; then
        PATH_CHANGED=true
    fi
fi

add_profile_block "$PROFILE_PATH" "$WRAPPER_PATH"

# ---------------------------------------------------------------------------
# Completion message
# ---------------------------------------------------------------------------

if [ "$IS_CURRENT" = true ]; then
    printf 'terminal-ai %s is already up to date.\n' "$MANIFEST_VERSION"
elif [ -z "$LOCAL_VERSION" ]; then
    printf 'Installed terminal-ai %s.\n' "$MANIFEST_VERSION"
else
    printf 'Updated terminal-ai from %s to %s.\n' "$LOCAL_VERSION" "$MANIFEST_VERSION"
fi

printf 'Install root: %s\n' "$INSTALL_ROOT"
printf 'Profile: %s\n' "$PROFILE_PATH"
if [ "$PATH_CHANGED" = true ]; then
    printf 'PATH updated. Open a new terminal if ai-core is not found in this one.\n'
fi
printf 'Reload your profile with: source %s\n' "$PROFILE_PATH"
printf 'Next: ai --config\n'