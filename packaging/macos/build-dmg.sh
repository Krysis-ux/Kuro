#!/bin/bash

set -euo pipefail


SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT"

DIST_DIR="$ROOT/dist"
APP="$ROOT/build/macos/Kuro.app"

bold() { printf '\033[1m%s\033[0m\n' "$1"; }
dim()  { printf '\033[2m%s\033[0m\n' "$1"; }
fail() { printf '\033[31m%s\033[0m\n' "$1" >&2; exit 1; }

command -v hdiutil >/dev/null 2>&1 || fail "hdiutil is required (it ships with macOS)."
command -v npm >/dev/null 2>&1 || fail "npm is required: the release carries the browser interface too."

# The native app, with the server, the `kuro` command, the browser interface and
# the logos inside it, is assembled and checked by build-app.sh.
if [ "${1:-}" = "--host-only" ]; then
  "$SCRIPT_DIR/build-app.sh"
else
  "$SCRIPT_DIR/build-app.sh" --universal
fi

[ -f "$APP/Contents/Resources/web/index.html" ] \
  || fail "The bundle is missing the browser interface."

VERSION="$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$APP/Contents/Info.plist")"


bold "Building the disk image"

STAGE="$ROOT/build/macos/dmg"
DMG="$DIST_DIR/Kuro-$VERSION.dmg"

rm -rf "$STAGE"
mkdir -p "$STAGE" "$DIST_DIR"
cp -R "$APP" "$STAGE/Kuro.app"
ln -s /Applications "$STAGE/Applications"
cp "$ROOT/THIRD_PARTY_NOTICES.md" "$STAGE/THIRD_PARTY_NOTICES.md" 2>/dev/null || true

rm -f "$DMG"
hdiutil create \
  -volname "Kuro $VERSION" \
  -srcfolder "$STAGE" \
  -fs HFS+ \
  -format UDZO \
  -quiet \
  "$DMG"

rm -rf "$STAGE"

printf '\n'
bold "Built $DMG"
dim "$(du -h "$DMG" | cut -f1) · $(lipo -archs "$APP/Contents/MacOS/Kuro")"
printf '\n'

dim "Unsigned: first launch needs right-click → Open."
