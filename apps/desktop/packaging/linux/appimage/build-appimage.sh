#!/usr/bin/env bash
# Relic Desktop — build a Linux AppImage from target/release/relic-desktop.
#
# Uses appimagetool directly (no linuxdeploy plugin chain needed: Relic
# ships as a single self-contained binary). Stages an AppDir with the
# binary, the .desktop file, an AppRun shim, and (optionally) an icon,
# then runs appimagetool to produce
#   target/appimage/relic-desktop-0.1.0-x86_64.AppImage
#
# Usage (from repo root, on a Linux host):
#   bash apps/desktop/packaging/linux/appimage/build-appimage.sh
#
# Requires: cargo, appimagetool on $PATH (or set APPIMAGETOOL=/path/to/it),
# and the Linux Slint build deps installed (see .github/workflows/ci.yml
# for the apt-get list). Run on Linux.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
APP_NAME="relic-desktop"
VERSION="${RELIC_VERSION:-0.1.0}"
ARCH="${RELIC_ARCH:-$(uname -m)}"

APPDIR="$REPO_ROOT/target/appimage/AppDir"
OUT_DIR="$REPO_ROOT/target/appimage"
OUT_PATH="$OUT_DIR/relic-desktop-${VERSION}-${ARCH}.AppImage"

echo ">> Building release binary"
cd "$REPO_ROOT"
cargo build --release -p "$APP_NAME"

BIN_PATH="$REPO_ROOT/target/release/$APP_NAME"
if [[ ! -x "$BIN_PATH" ]]; then
  echo "error: $BIN_PATH not found after cargo build" >&2
  exit 1
fi

echo ">> Staging AppDir at $APPDIR"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/bin" "$APPDIR/share/applications"
cp "$BIN_PATH" "$APPDIR/bin/$APP_NAME"
cp "$SCRIPT_DIR/relic-desktop.desktop" "$APPDIR/$APP_NAME.desktop"
cp "$SCRIPT_DIR/AppRun" "$APPDIR/AppRun"
chmod +x "$APPDIR/AppRun" "$APPDIR/bin/$APP_NAME"

# Optional icon: if a 256x256 org.relic.Relic.png is shipped next to this
# script, install it; otherwise leave a placeholder name so the AppImage
# still builds (no icon resource).
if [[ -f "$SCRIPT_DIR/org.relic.Relic.png" ]]; then
  mkdir -p "$APPDIR/share/icons/hicolor/256x256/apps"
  cp "$SCRIPT_DIR/org.relic.Relic.png" \
     "$APPDIR/share/icons/hicolor/256x256/apps/org.relic.Relic.png"
  cp "$SCRIPT_DIR/org.relic.Relic.png" "$APPDIR/org.relic.Relic.png"
fi

echo ">> Locating appimagetool"
APPIMAGETOOL_BIN="${APPIMAGETOOL:-appimagetool}"
if ! command -v "$APPIMAGETOOL_BIN" >/dev/null 2>&1; then
  echo "error: appimagetool not found on PATH." >&2
  echo "       Download it from https://github.com/AppImage/AppImageKit/releases" >&2
  echo "       or set APPIMAGETOOL=/path/to/appimagetool." >&2
  exit 1
fi

echo ">> Building AppImage at $OUT_PATH"
mkdir -p "$OUT_DIR"
rm -f "$OUT_PATH"
"$APPIMAGETOOL_BIN" "$APPDIR" "$OUT_PATH"

echo ">> Done: $OUT_PATH"
