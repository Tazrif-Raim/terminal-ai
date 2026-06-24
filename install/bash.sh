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

set -euo pipefail

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
    curl -fsSL "$url" -o "$out"
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

get_local_version() {
    if [ ! -f "$LOCAL_MANIFEST" ]; then
        echo ""
        return
    fi
    sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$LOCAL_MANIFEST" 2>/dev/null || echo ""
}

add_to_path() {
    local path_entry="$1"

    # Check if already in PATH (both current session and profile)
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

    # Add to PATH for current session
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
    content="$(printf '%s\n' "$content" | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}')"  # trim trailing blank lines

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
        RELEASE_DIR="linux-x64"
        ;;
    aarch64|arm64)
        ASSET_KEY="linux_arm64"
        RELEASE_DIR="linux-arm64"
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
MANIFEST_JSON="$(curl -fsSL "$MANIFEST_URL")"

# Simple JSON field extraction (no jq dependency)
# Extracts a top-level string field from the manifest
parse_manifest() {
    printf '%s\n' "$MANIFEST_JSON" | sed -n "s/.*\"${1}\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p"
}

MANIFEST_VERSION="$(parse_manifest "version")"
if [ -z "$MANIFEST_VERSION" ]; then
    printf 'Failed to parse version from manifest at %s\n' "$MANIFEST_URL" >&2
    exit 1
fi

# Extract a specific field from a specific sub-section within ASSET_KEY
# Usage: extract_asset_field <section> <field>
# e.g. extract_asset_field "ai_core" "url"
extract_asset_field() {
    local section="$1"
    local field="$2"
    printf '%s\n' "$MANIFEST_JSON" | \
        sed -n "/\"${ASSET_KEY}\"[[:space:]]*:/,/^[[:space:]]*}/{
            /\"${section}\"[[:space:]]*:/,/^[[:space:]]*}/{
                s/.*\"${field}\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p
            }
        }"
}

AI_CORE_URL="$(extract_asset_field "ai_core" "url")"
AI_CORE_SHA="$(extract_asset_field "ai_core" "sha256")"
BASH_WRAPPER_URL="$(extract_asset_field "bash_wrapper" "url")"
BASH_WRAPPER_SHA="$(extract_asset_field "bash_wrapper" "sha256")"

if [ -z "$AI_CORE_URL" ] || [ -z "$AI_CORE_SHA" ]; then
    printf 'Manifest does not include %s assets.\n' "$ASSET_KEY" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# Check if current
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

    # Cleanup on exit
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