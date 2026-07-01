#!/usr/bin/env bash
#
# Merge Windows and Linux release artifacts into a single site directory.
#
# Usage:
#   ./scripts/merge-release.sh <windows-dir> <linux-dir> <output-root>

set -euo pipefail

WINDOWS_DIR="${1:?Usage: $0 <windows-dir> <linux-dir> <output-root>}"
LINUX_DIR="${2:?}"
OUTPUT_ROOT="${3:?}"

# Copy everything from Windows dir to output
if [ -d "$WINDOWS_DIR" ]; then
    mkdir -p "$OUTPUT_ROOT"
    cp -a "$WINDOWS_DIR"/. "$OUTPUT_ROOT"/
fi

# Copy Linux release files into the output (merging, not nesting)
LINUX_RELEASES="$LINUX_DIR/releases"
if [ -d "$LINUX_RELEASES" ]; then
    mkdir -p "$OUTPUT_ROOT/releases"
    for item in "$LINUX_RELEASES"/*; do
        [ -e "$item" ] || continue
        cp -a "$item"/. "$OUTPUT_ROOT/releases/$(basename "$item")"
    done
fi

# Copy Linux install/uninstall scripts
for script in install.sh uninstall.sh; do
    if [ -f "$LINUX_DIR/$script" ]; then
        cp -a "$LINUX_DIR/$script" "$OUTPUT_ROOT/$script"
    fi
done

# Update version.json to include Linux platform entries
MANIFEST_PATH="$OUTPUT_ROOT/version.json"
if [ -f "$MANIFEST_PATH" ]; then
    VERSION=$(jq -r '.version' "$MANIFEST_PATH")

    LINUX_CHECKSUMS_PATH="$LINUX_DIR/releases/$VERSION/linux-x64/checksums.txt"
    if [ -f "$LINUX_CHECKSUMS_PATH" ]; then
        linux_entry=$(jq -n '{}')
        while IFS= read -r line; do
            [ -z "$line" ] && continue
            sha="${line%%  *}"
            path="${line#*  }"
            case "$path" in
                *ai-core)    asset="ai_core" ;;
                *bash.sh)    asset="bash_wrapper" ;;
                *zsh.zsh)    asset="zsh_wrapper" ;;
                *)           continue ;;
            esac
            url="/$path"
            linux_entry=$(echo "$linux_entry" | jq \
                --arg a "$asset" \
                --arg u "$url" \
                --arg s "$sha" \
                '.[$a] = {url: $u, sha256: $s}')
        done < "$LINUX_CHECKSUMS_PATH"

        jq --argjson linux "$linux_entry" '.linux_x64 = $linux' \
            "$MANIFEST_PATH" > "${MANIFEST_PATH}.tmp" && \
            mv "${MANIFEST_PATH}.tmp" "$MANIFEST_PATH"
    else
        echo "Warning: Linux checksums not found at: $LINUX_CHECKSUMS_PATH" >&2
    fi
fi

echo "Merged releases into: $OUTPUT_ROOT"
