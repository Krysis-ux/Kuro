#!/bin/bash
#
# Build Kuro.app and wrap it in a disk image.
#
#   packaging/macos/build-dmg.sh              universal (Apple Silicon + Intel)
#   packaging/macos/build-dmg.sh --host-only  this Mac's architecture only
#
# Writes dist/Kuro-<version>.dmg: a drag-to-Applications image. The bundle is
# ad-hoc signed only; see NOTARISATION at the bottom.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT"

bold() { printf '\033[1m%s\033[0m\n' "$1"; }
dim()  { printf '\033[2m%s\033[0m\n' "$1"; }
fail() { printf '\033[31m%s\033[0m\n' "$1" >&2; exit 1; }

command -v hdiutil >/dev/null 2>&1 || fail "hdiutil is required (it ships with macOS)."

if [ "${1:-}" = "--host-only" ]; then
  "$SCRIPT_DIR/build-app.sh"
else
  "$SCRIPT_DIR/build-app.sh" --universal
fi

APP="$ROOT/build/macos/Kuro.app"
[ -f "$APP/Contents/Resources/web/index.html" ] \
  || fail "The release bundle needs the browser interface too. Install Node.js 18+ and run this again."

VERSION="$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$APP/Contents/Info.plist")"
STAGE="$ROOT/build/macos/dmg"
DMG="$ROOT/dist/Kuro-$VERSION.dmg"

bold "Building the disk image"
rm -rf "$STAGE"
mkdir -p "$STAGE" "$ROOT/dist"
cp -R "$APP" "$STAGE/Kuro.app"
ln -s /Applications "$STAGE/Applications"
cp "$ROOT/NOTICES.md" "$STAGE/" 2>/dev/null || true

rm -f "$DMG"
hdiutil create -volname "Kuro $VERSION" -srcfolder "$STAGE" -fs HFS+ -format UDZO -quiet "$DMG"
rm -rf "$STAGE"

bold "Built $DMG"
dim "$(du -h "$DMG" | cut -f1) · $(lipo -archs "$APP/Contents/MacOS/Kuro")"

# NOTARISATION
#
# Without a Developer ID, Gatekeeper refuses the first launch ("cannot be
# verified"); right-click the app and choose Open to get past it. With one,
# sign and notarise before shipping:
#
#   codesign --deep --force --options runtime --timestamp \
#     --sign "Developer ID Application: <name> (<team id>)" build/macos/Kuro.app
#   xcrun notarytool submit "$DMG" --apple-id <id> --team-id <team> \
#     --password <app-specific-password> --wait
#   xcrun stapler staple "$DMG"
dim "Not notarised: first launch needs right-click → Open. See NOTARISATION in this script."
