#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 PolySaver contributors

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
TARGET_BIN_DIR="${ROOT_DIR}/src-tauri/resources/bin"

# Pinned exact versions
YTDLP_VERSION="2026.08.19"
FFMPEG_VERSION="9.0.1"

# Target platform selection (default: auto-detect)
PLATFORM="${1:-auto}"
if [ "${PLATFORM}" = "auto" ]; then
    OS="$(uname -s)"
    ARCH="$(uname -m)"
    case "${OS}" in
        Darwin)
            if [ "${ARCH}" = "x86_64" ]; then
                PLATFORM="macos-x86_64"
            else
                PLATFORM="macos-aarch64"
            fi
            ;;
        Linux)
            PLATFORM="linux-x64"
            ;;
        MINGW*|MSYS*|CYGWIN*|Windows_NT)
            PLATFORM="windows-x64"
            ;;
        *)
            echo "[ERROR] Unsupported OS: ${OS} (${ARCH})"
            exit 1
            ;;
    esac
fi

echo "========================================="
echo "PolySaver Sidecar Provisioning"
echo "Target Platform:  ${PLATFORM}"
echo "Target Directory: ${TARGET_BIN_DIR}"
echo "========================================="

# Clean staging directory
rm -rf "${TARGET_BIN_DIR}"
mkdir -p "${TARGET_BIN_DIR}"

verify_hash() {
    local file="$1"
    local expected_hash="$2"
    local actual_hash=""

    if [ ! -f "${file}" ]; then
        echo "[ERROR] File not found for verification: ${file}"
        return 1
    fi

    if command -v sha256sum >/dev/null 2>&1; then
        actual_hash="$(sha256sum "${file}" | awk '{print $1}')"
    elif command -v shasum >/dev/null 2>&1; then
        actual_hash="$(shasum -a 256 "${file}" | awk '{print $1}')"
    elif command -v node >/dev/null 2>&1; then
        actual_hash="$(node -e "const crypto = require('crypto'); const fs = require('fs'); console.log(crypto.createHash('sha256').update(fs.readFileSync(process.argv[1])).digest('hex'));" "${file}")"
    else
        echo "[ERROR] Neither sha256sum, shasum, nor node available for hash verification"
        return 1
    fi

    if [ "${actual_hash}" != "${expected_hash}" ]; then
        echo "[ERROR] Hash mismatch for ${file}!"
        echo "Expected: ${expected_hash}"
        echo "Actual:   ${actual_hash}"
        return 1
    fi
    echo "[OK] SHA-256 verified for $(basename "${file}")"
}

case "${PLATFORM}" in
    macos-aarch64|macos-x86_64)
        YTDLP_URL="https://github.com/yt-dlp/yt-dlp/releases/download/${YTDLP_VERSION}/yt-dlp_macos"
        YTDLP_SHA256="0f192b7ec147ab6288885d6351d9ab67367640029b4377576ef46dd79cf7b202"
        FFMPEG_BIN_SHA256="5c2f76b122c6c5bfae448d3fe1aa3ce9f272952d3ec574e079594fa161101935"
        FFPROBE_BIN_SHA256="d4d3cbd541eac6c005bdb60decd4ee1a7a7d68df9eb5c0daab9da5e6fb1c6360"

        # 1. yt-dlp (Universal Mach-O binary containing x86_64 & arm64)
        echo "[DOWNLOADING] yt-dlp ${YTDLP_VERSION} for macOS..."
        YTDLP_TARGET="${TARGET_BIN_DIR}/yt-dlp"
        curl -fsSL -o "${YTDLP_TARGET}" "${YTDLP_URL}"
        verify_hash "${YTDLP_TARGET}" "${YTDLP_SHA256}"
        chmod +x "${YTDLP_TARGET}"

        # 2. FFmpeg & FFprobe
        FFMPEG_TARGET="${TARGET_BIN_DIR}/ffmpeg"
        FFPROBE_TARGET="${TARGET_BIN_DIR}/ffprobe"

        # Check local prebuilt or build
        CACHE_PREFIX="/tmp/polysaver_${PLATFORM}"
        if [ -f "${CACHE_PREFIX}_ffmpeg" ] && [ -f "${CACHE_PREFIX}_ffprobe" ] && \
           "${CACHE_PREFIX}_ffmpeg" -version 2>&1 | grep -q "9.0.1" && \
           "${CACHE_PREFIX}_ffprobe" -version 2>&1 | grep -q "9.0.1"; then
            echo "[SKIP] Using cached static FFmpeg & FFprobe 9.0.1 for ${PLATFORM}..."
            cp "${CACHE_PREFIX}_ffmpeg" "${FFMPEG_TARGET}"
            cp "${CACHE_PREFIX}_ffprobe" "${FFPROBE_TARGET}"
        else
            echo "[BUILDING] Compiling reproducible static FFmpeg & FFprobe ${FFMPEG_VERSION} for ${PLATFORM}..."
            BUILD_TMP="$(mktemp -d /tmp/ffmpeg_build_XXXXXX)"
            STATIC_LIBS_DIR="${BUILD_TMP}/static_libs"
            PKG_CONFIG_BIN="${BUILD_TMP}/pkg_bin"
            mkdir -p "${STATIC_LIBS_DIR}" "${PKG_CONFIG_BIN}"

            DAV1D_VERSION="1.5.3"
            DAV1D_SOURCE_URL="https://code.videolan.org/videolan/dav1d/-/archive/${DAV1D_VERSION}/dav1d-${DAV1D_VERSION}.tar.gz"
            LAME_VERSION="3.100"
            LAME_SOURCE_URL="https://downloads.sourceforge.net/project/lame/lame/${LAME_VERSION}/lame-${LAME_VERSION}.tar.gz"
            LAME_SOURCE_SHA256="ddfe36cab873794038ae2c1210557ad34857a4b6bdc515785d1da9e175b1da1e"
            FFMPEG_SOURCE_URL="https://ffmpeg.org/releases/ffmpeg-9.0.1.tar.xz"
            FFMPEG_SOURCE_SHA256="cf38e0e28c7e5605942c4a77755349b0145804a397af37eb1fb4c77cb237f635"

            DAV1D_BUILD_DIR="${BUILD_TMP}/dav1d"
            mkdir -p "${DAV1D_BUILD_DIR}"
            curl -fsSL "${DAV1D_SOURCE_URL}" | tar -xz -C "${DAV1D_BUILD_DIR}"
            cd "${DAV1D_BUILD_DIR}/dav1d-${DAV1D_VERSION}"
            python3 -m venv "${BUILD_TMP}/venv"
            "${BUILD_TMP}/venv/bin/pip" install --quiet meson ninja
            PATH="${BUILD_TMP}/venv/bin:$PATH" meson setup build --default-library=static --buildtype=release -Denable_asm=false -Denable_examples=false -Denable_tests=false
            PATH="${BUILD_TMP}/venv/bin:$PATH" ninja -C build
            cp -f "${DAV1D_BUILD_DIR}/dav1d-${DAV1D_VERSION}/build/src/libdav1d.a" "${STATIC_LIBS_DIR}/"

            LAME_ARCHIVE="${BUILD_TMP}/lame.tar.gz"
            curl -fsSL -o "${LAME_ARCHIVE}" "${LAME_SOURCE_URL}"
            verify_hash "${LAME_ARCHIVE}" "${LAME_SOURCE_SHA256}"
            tar -xzf "${LAME_ARCHIVE}" -C "${BUILD_TMP}"
            LAME_SOURCE_DIR="${BUILD_TMP}/lame-${LAME_VERSION}"
            cd "${LAME_SOURCE_DIR}"
            ./configure \
                --prefix="${BUILD_TMP}/lame-dist" \
                --disable-shared \
                --enable-static \
                --disable-frontend
            make -j8
            make install
            LAME_PREFIX="${BUILD_TMP}/lame-dist"
            cp -f "${LAME_PREFIX}/lib/libmp3lame.a" "${STATIC_LIBS_DIR}/"

            X264_PREFIX="$(brew --prefix x264)"
            if [ ! -f "${X264_PREFIX}/lib/libx264.a" ]; then
                echo "[ERROR] Static x264 library not found under ${X264_PREFIX}"
                exit 1
            fi
            cp -f "${X264_PREFIX}/lib/libx264.a" "${STATIC_LIBS_DIR}/"

            cat << EOF > "${PKG_CONFIG_BIN}/pkg-config"
#!/bin/bash
case "\$*" in
    *"--exists"*) exit 0 ;;
    *"--modversion"*)
        if [[ "\$*" == *"libmp3lame"* ]]; then echo "${LAME_VERSION}"
        elif [[ "\$*" == *"x264"* ]]; then echo "0.165.3222"
        else echo "${DAV1D_VERSION}"
        fi
        ;;
    *"--cflags"*"--libs"*|*"--libs"*"--cflags"*)
        printf '%s ' "-I${LAME_PREFIX}/include" "-I${X264_PREFIX}/include" "-I${DAV1D_BUILD_DIR}/dav1d-${DAV1D_VERSION}/include" "-I${DAV1D_BUILD_DIR}/dav1d-${DAV1D_VERSION}/build/include"
        if [[ "\$*" == *"dav1d"* ]]; then echo "-L${STATIC_LIBS_DIR} -ldav1d -lm -lpthread"
        elif [[ "\$*" == *"x264"* ]]; then echo "-L${STATIC_LIBS_DIR} -lx264 -lm -lpthread"
        elif [[ "\$*" == *"mp3lame"* ]]; then echo "-L${STATIC_LIBS_DIR} -lmp3lame -lm"
        else echo "-L${STATIC_LIBS_DIR}"
        fi
        ;;
    *"--cflags"*) echo "-I${LAME_PREFIX}/include -I${X264_PREFIX}/include -I${DAV1D_BUILD_DIR}/dav1d-${DAV1D_VERSION}/include -I${DAV1D_BUILD_DIR}/dav1d-${DAV1D_VERSION}/build/include" ;;
    *"--libs"*)
        if [[ "\$*" == *"dav1d"* ]]; then echo "-L${STATIC_LIBS_DIR} -ldav1d -lm -lpthread"
        elif [[ "\$*" == *"x264"* ]]; then echo "-L${STATIC_LIBS_DIR} -lx264 -lm -lpthread"
        elif [[ "\$*" == *"mp3lame"* ]]; then echo "-L${STATIC_LIBS_DIR} -lmp3lame -lm"
        else echo "-L${STATIC_LIBS_DIR}"
        fi
        ;;
    *) exit 0 ;;
esac
EOF
            chmod +x "${PKG_CONFIG_BIN}/pkg-config"

            curl -fsSL -o "${BUILD_TMP}/ffmpeg.tar.xz" "${FFMPEG_SOURCE_URL}"
            verify_hash "${BUILD_TMP}/ffmpeg.tar.xz" "${FFMPEG_SOURCE_SHA256}"
            tar -xf "${BUILD_TMP}/ffmpeg.tar.xz" -C "${BUILD_TMP}"

            SRC_DIR="$(find "${BUILD_TMP}" -maxdepth 1 -name "ffmpeg-*" -type d | head -n 1)"
            cd "${SRC_DIR}"
            PATH="${PKG_CONFIG_BIN}:$PATH" ./configure \
                --prefix="${BUILD_TMP}/dist" \
                --enable-gpl \
                --enable-version3 \
                --enable-libmp3lame \
                --enable-libx264 \
                --enable-libdav1d \
                --enable-videotoolbox \
                --enable-audiotoolbox \
                --disable-shared \
                --enable-static \
                --disable-lzma \
                --disable-xlib \
                --disable-libxcb \
                --disable-sdl2 \
                --extra-cflags="-I${LAME_PREFIX}/include -I${X264_PREFIX}/include -I${DAV1D_BUILD_DIR}/dav1d-${DAV1D_VERSION}/include -I${DAV1D_BUILD_DIR}/dav1d-${DAV1D_VERSION}/build/include" \
                --extra-ldflags="-L${STATIC_LIBS_DIR}" \
                --cc=clang

            PATH="${PKG_CONFIG_BIN}:$PATH" make -j8
            cp -f "${SRC_DIR}/ffmpeg" "${FFMPEG_TARGET}"
            cp -f "${SRC_DIR}/ffprobe" "${FFPROBE_TARGET}"
            chmod +x "${FFMPEG_TARGET}" "${FFPROBE_TARGET}"

            # Cache locally for subsequent fast builds
            cp -f "${FFMPEG_TARGET}" "${CACHE_PREFIX}_ffmpeg" 2>/dev/null || true
            cp -f "${FFPROBE_TARGET}" "${CACHE_PREFIX}_ffprobe" 2>/dev/null || true
            rm -rf "${BUILD_TMP}"
        fi
        chmod +x "${FFMPEG_TARGET}" "${FFPROBE_TARGET}"
        ;;

    windows-x64)
        YTDLP_URL="https://github.com/yt-dlp/yt-dlp/releases/download/${YTDLP_VERSION}/yt-dlp.exe"
        YTDLP_SHA256="66674953fe251b89f4d08c5f0e35e0728679bd67ab3d7d05c0562af101dd3e7a"
        FFMPEG_ZIP_URL="https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-22-12-58/ffmpeg-n9.0.1-6-g9d4ca21220-win64-gpl-9.0.zip"
        FFMPEG_ZIP_SHA256="7b777da65c0f93a3f9f524997b2852612d984e39249ca241b63da193ce7e4435"

        # 1. yt-dlp.exe
        echo "[DOWNLOADING] yt-dlp.exe ${YTDLP_VERSION} for Windows x64..."
        YTDLP_TARGET="${TARGET_BIN_DIR}/yt-dlp.exe"
        curl -fsSL -o "${YTDLP_TARGET}" "${YTDLP_URL}"
        verify_hash "${YTDLP_TARGET}" "${YTDLP_SHA256}"

        # 2. FFmpeg & FFprobe .exe
        echo "[DOWNLOADING] FFmpeg ${FFMPEG_VERSION} package for Windows x64..."
        TEMP_DIR="$(mktemp -d)"
        FFMPEG_ZIP="${TEMP_DIR}/ffmpeg_win64.zip"
        curl -fsSL -o "${FFMPEG_ZIP}" "${FFMPEG_ZIP_URL}"
        verify_hash "${FFMPEG_ZIP}" "${FFMPEG_ZIP_SHA256}"

        unzip -q "${FFMPEG_ZIP}" -d "${TEMP_DIR}"
        EXTRACTED_BIN_DIR="$(find "${TEMP_DIR}" -type d -name "bin" | head -n 1)"
        FFMPEG_TARGET="${TARGET_BIN_DIR}/ffmpeg.exe"
        FFPROBE_TARGET="${TARGET_BIN_DIR}/ffprobe.exe"

        mv -f "${EXTRACTED_BIN_DIR}/ffmpeg.exe" "${FFMPEG_TARGET}"
        mv -f "${EXTRACTED_BIN_DIR}/ffprobe.exe" "${FFPROBE_TARGET}"
        rm -rf "${TEMP_DIR}"
        ;;

    linux-x64)
        YTDLP_URL="https://github.com/yt-dlp/yt-dlp/releases/download/${YTDLP_VERSION}/yt-dlp_linux"
        YTDLP_SHA256="58162f9bfdc27458ea47bfcb311cf47028f17d8154a8bf7d689861d46399230a"
        FFMPEG_TAR_URL="https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-08-22-12-58/ffmpeg-n9.0.1-6-g9d4ca21220-linux64-gpl-9.0.tar.xz"
        FFMPEG_TAR_SHA256="b32f5844b258b4367896b1aa6839ee72ee96d5c7c9136873f66b3c4a1fc2c1df"

        # 1. yt-dlp
        echo "[DOWNLOADING] yt-dlp ${YTDLP_VERSION} for Linux x64..."
        YTDLP_TARGET="${TARGET_BIN_DIR}/yt-dlp"
        curl -fsSL -o "${YTDLP_TARGET}" "${YTDLP_URL}"
        verify_hash "${YTDLP_TARGET}" "${YTDLP_SHA256}"
        chmod +x "${YTDLP_TARGET}"

        # 2. FFmpeg & FFprobe
        echo "[DOWNLOADING] FFmpeg ${FFMPEG_VERSION} package for Linux x64..."
        TEMP_DIR="$(mktemp -d)"
        FFMPEG_TAR="${TEMP_DIR}/ffmpeg_linux64.tar.xz"
        curl -fsSL -o "${FFMPEG_TAR}" "${FFMPEG_TAR_URL}"
        verify_hash "${FFMPEG_TAR}" "${FFMPEG_TAR_SHA256}"

        tar -xf "${FFMPEG_TAR}" -C "${TEMP_DIR}"
        EXTRACTED_BIN_DIR="$(find "${TEMP_DIR}" -type d -name "bin" | head -n 1)"
        FFMPEG_TARGET="${TARGET_BIN_DIR}/ffmpeg"
        FFPROBE_TARGET="${TARGET_BIN_DIR}/ffprobe"

        mv -f "${EXTRACTED_BIN_DIR}/ffmpeg" "${FFMPEG_TARGET}"
        mv -f "${EXTRACTED_BIN_DIR}/ffprobe" "${FFPROBE_TARGET}"
        chmod +x "${FFMPEG_TARGET}" "${FFPROBE_TARGET}"
        rm -rf "${TEMP_DIR}"
        ;;

    *)
        echo "[ERROR] Unknown platform: ${PLATFORM}"
        exit 1
        ;;
esac

echo "========================================="
echo "[SUCCESS] Sidecars provisioned for ${PLATFORM}."

# If executing on native runner, verify versions
if [[ "${PLATFORM}" =~ ^macos-(aarch64|x86_64)$ ]] && [ "$(uname -s)" = "Darwin" ]; then
    echo "Running native verification:"
    "${YTDLP_TARGET}" --version
    "${FFMPEG_TARGET}" -version | head -n 1
    "${FFPROBE_TARGET}" -version | head -n 1
elif [ "${PLATFORM}" = "linux-x64" ] && [ "$(uname -s)" = "Linux" ]; then
    echo "Running native verification:"
    "${YTDLP_TARGET}" --version
    "${FFMPEG_TARGET}" -version | head -n 1
    "${FFPROBE_TARGET}" -version | head -n 1
elif [ "${PLATFORM}" = "windows-x64" ] && [[ "$(uname -s)" =~ (MINGW|MSYS|CYGWIN|Windows_NT) ]]; then
    echo "Running native verification:"
    "${YTDLP_TARGET}" --version
    "${FFMPEG_TARGET}" -version | head -n 1
    "${FFPROBE_TARGET}" -version | head -n 1
fi
echo "========================================="
