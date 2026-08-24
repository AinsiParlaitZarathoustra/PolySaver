#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 PolySaver contributors

set -euo pipefail

echo "========================================="
echo "PolySaver Architectural Boundary Check"
echo "========================================="

FAILURES=0

# Rule 1: polysaver-core must NOT depend on Tauri, React, yt-dlp, ffmpeg, or other adapters
echo "Checking polysaver-core isolation..."
if grep -qE "tauri|yt-dlp|ffmpeg-sidecar|polysaver-ytdlp|polysaver-ffmpeg|polysaver-storage|polysaver-binres" crates/polysaver-core/Cargo.toml; then
    echo "[ERROR] polysaver-core contains forbidden dependencies in Cargo.toml!"
    FAILURES=$((FAILURES + 1))
fi

if grep -rnE "tauri|polysaver_ytdlp|polysaver_ffmpeg|polysaver_storage|polysaver_binres" crates/polysaver-core/src/; then
    echo "[ERROR] polysaver-core source imports peripheral adapters, binary resolver, or Tauri!"
    FAILURES=$((FAILURES + 1))
fi

# Rule 2: Peripheral adapters must NOT depend on each other
echo "Checking peripheral adapter cross-dependencies..."
for adapter in polysaver-ytdlp polysaver-ffmpeg polysaver-storage; do
    for other in polysaver-ytdlp polysaver-ffmpeg polysaver-storage; do
        if [ "$adapter" != "$other" ]; then
            if grep -q "$other" "crates/$adapter/Cargo.toml" 2>/dev/null; then
                echo "[ERROR] $adapter depends on $other!"
                FAILURES=$((FAILURES + 1))
            fi
        fi
    done
done

# Rule 3: polysaver-binres must NOT depend on polysaver-core or Tauri
echo "Checking polysaver-binres leaf crate isolation..."
if grep -qE "polysaver-core|polysaver-ytdlp|polysaver-ffmpeg|polysaver-storage|tauri" crates/polysaver-binres/Cargo.toml 2>/dev/null; then
    echo "[ERROR] polysaver-binres contains forbidden dependencies in Cargo.toml!"
    FAILURES=$((FAILURES + 1))
fi

# Rule 4: React components must NOT import @tauri-apps directly outside src/ipc
echo "Checking UI component isolation from Tauri IPC..."
DIRECT_TAURI_IMPORTS=$(grep -rnE "@tauri-apps/(api|plugin)" src/ | grep -v "src/ipc/" || true)
if [ -n "$DIRECT_TAURI_IMPORTS" ]; then
    echo "[ERROR] Found direct @tauri-apps imports outside src/ipc/:"
    echo "$DIRECT_TAURI_IMPORTS"
    FAILURES=$((FAILURES + 1))
fi

# Rule 5: No tauri commands outside src-tauri
echo "Checking tauri commands location..."
NON_TAURI_COMMANDS=$(grep -rn "#\[tauri::command\]" crates/ || true)
if [ -n "$NON_TAURI_COMMANDS" ]; then
    echo "[ERROR] Found #[tauri::command] outside src-tauri/:"
    echo "$NON_TAURI_COMMANDS"
    FAILURES=$((FAILURES + 1))
fi

# Rule 6: Verify Cargo workspace structure via cargo metadata
echo "Verifying cargo metadata dependency hierarchy..."
cargo metadata --format-version 1 --no-deps > /dev/null

if [ "$FAILURES" -gt 0 ]; then
    echo "========================================="
    echo "[FAILED] $FAILURES architectural boundary violation(s) detected."
    echo "========================================="
    exit 1
else
    echo "========================================="
    echo "[PASSED] All architectural boundaries are strictly respected."
    echo "========================================="
    exit 0
fi
