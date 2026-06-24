#!/usr/bin/env bash
#
# Package the Linux release of terminal-ai.
#
# Usage:
#   ./scripts/package-linux.sh [--base-url URL] [--output-root DIR] [--skip-build]
#
# Output structure (dist/site):
#   releases/<version>/linux-x64/ai-core
#   releases/<version>/linux-x64/shell/bash.sh
#   releases/<version>/linux-x64/checksums.txt
#   install.sh          (Linux installer)
#   uninstall.sh        (Linux uninstaller)

set -euo pipefail

BASE_URL="${BASE_URL:-https://terminal-ai.lab-node.me}"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT_ROOT="${OUTPUT_ROOT:-${REPO_ROOT}/dist/site}"
SKIP_BUILD="${SKIP_BUILD:-false}"

CARGO_TOML="${REPO_ROOT}/ai-core/Cargo.toml"
WRAPPER_SOURCE="${REPO_ROOT}/shell/bash.sh"
INSTALLER_SOURCE="${REPO_ROOT}/install/bash.sh"
UNINSTALLER_SOURCE="${REPO_ROOT}/install/uninstall.sh"
RELEASE_BINARY="${REPO_ROOT}/ai-core/target/release/ai-core"

# ---------------------------------------------------------------------------
# Get version from Cargo.toml
# ---------------------------------------------------------------------------

get_version() {
    local toml="$1"
    sed -n 's/^version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$toml" | head -1
}

# ---------------------------------------------------------------------------
# SHA256 helper
# ---------------------------------------------------------------------------

get_sha256() {
    local path="$1"
    sha256sum "$path" | cut -d' ' -f1
}

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------

VERSION="$(get_version "$CARGO_TOML")"
if [ -z "$VERSION" ]; then
    printf 'Could not read version from %s\n' "$CARGO_TOML" >&2
    exit 1
fi

if [ "$SKIP_BUILD" != "true" ]; then
    cargo build --manifest-path "$CARGO_TOML" --release
fi

if [ ! -f "$RELEASE_BINARY" ]; then
    printf 'Release binary not found: %s\n' "$RELEASE_BINARY" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# Create output directories
# ---------------------------------------------------------------------------

RELEASE_DIR="${OUTPUT_ROOT}/releases/${VERSION}/linux-x64"
RELEASE_SHELL_DIR="${RELEASE_DIR}/shell"
mkdir -p "$OUTPUT_ROOT" "$RELEASE_DIR" "$RELEASE_SHELL_DIR"

# ---------------------------------------------------------------------------
# Copy assets
# ---------------------------------------------------------------------------

AI_CORE_OUT="${RELEASE_DIR}/ai-core"
WRAPPER_OUT="${RELEASE_SHELL_DIR}/bash.sh"
INSTALLER_OUT="${OUTPUT_ROOT}/install.sh"
UNINSTALLER_OUT="${OUTPUT_ROOT}/uninstall.sh"
CHECKSUMS_OUT="${RELEASE_DIR}/checksums.txt"

cp "$RELEASE_BINARY" "$AI_CORE_OUT"
cp "$WRAPPER_SOURCE" "$WRAPPER_OUT"
cp "$INSTALLER_SOURCE" "$INSTALLER_OUT"
cp "$UNINSTALLER_SOURCE" "$UNINSTALLER_OUT"
chmod +x "$AI_CORE_OUT" "$INSTALLER_OUT" "$UNINSTALLER_OUT"

# ---------------------------------------------------------------------------
# Compute checksums
# ---------------------------------------------------------------------------

AI_CORE_SHA="$(get_sha256 "$AI_CORE_OUT")"
WRAPPER_SHA="$(get_sha256 "$WRAPPER_OUT")"

printf '%s  releases/%s/linux-x64/ai-core\n' "$AI_CORE_SHA" "$VERSION" > "$CHECKSUMS_OUT"
printf '%s  releases/%s/linux-x64/shell/bash.sh\n' "$WRAPPER_SHA" "$VERSION" >> "$CHECKSUMS_OUT"

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------

printf 'Packaged terminal-ai %s (Linux x64)\n' "$VERSION"
printf 'Output: %s\n' "$OUTPUT_ROOT"
printf 'Install URL: %s/install.sh\n' "${BASE_URL%/}"
printf 'Uninstall URL: %s/uninstall.sh\n' "${BASE_URL%/}"