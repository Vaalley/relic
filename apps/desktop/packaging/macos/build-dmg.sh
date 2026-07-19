#!/usr/bin/env bash
# Relic Desktop — build a macOS .dmg from the release binary.
#
# Assembles relic-desktop.app/Contents/{MacOS,Info.plist} from
#   target/release/relic-desktop
# and the Info.plist in this directory, then uses hdiutil to produce
#   target/dmg/relic-desktop-0.1.0.dmg
#
# Usage (from repo root):
#   bash apps/desktop/packaging/macos/build-dmg.sh
#
# Requires: cargo, hdiutil (macOS only). Run on macOS.
set -euo pipefail

# Resolve repo root relative to this script.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
APP_NAME="relic-desktop"
BUNDLE_NAME="relic-desktop.app"
VERSION="${RELIC_VERSION:-0.1.0}"

STAGE_DIR="$REPO_ROOT/target/dmg/stage"
DMG_DIR="$REPO_ROOT/target/dmg"
DMG_PATH="$DMG_DIR/relic-desktop-${VERSION}.dmg"

echo ">> Building release binary"
cd "$REPO_ROOT"
cargo build --release -p "$APP_NAME"

BIN_PATH="$REPO_ROOT/target/release/$APP_NAME"
if [[ ! -x "$BIN_PATH" ]]; then
  echo "error: $BIN_PATH not found after cargo build" >&2
  exit 1
fi

echo ">> Assembling $BUNDLE_NAME bundle"
rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR/$BUNDLE_NAME/Contents/MacOS"
mkdir -p "$STAGE_DIR/$BUNDLE_NAME/Contents/Resources"
cp "$BIN_PATH" "$STAGE_DIR/$BUNDLE_NAME/Contents/MacOS/$APP_NAME"
cp "$SCRIPT_DIR/Info.plist" "$STAGE_DIR/$BUNDLE_NAME/Contents/Info.plist"

# Symlink /Applications into the staging dir so the dmg opens with the
# classic "drag app to Applications" affordance.
ln -s /Applications "$STAGE_DIR/Applications"

echo ">> Creating .dmg at $DMG_PATH"
mkdir -p "$DMG_DIR"
rm -f "$DMG_PATH"
hdiutil create \
  -volname "Relic Desktop $VERSION" \
  -srcfolder "$STAGE_DIR" \
  -ov \
  -format UDZO \
  "$DMG_PATH"

echo ">> Done: $DMG_PATH"
