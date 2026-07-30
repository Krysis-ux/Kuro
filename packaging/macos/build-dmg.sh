#!/bin/bash
#
# Build Kuro.app and wrap it in a disk image.
#
#   packaging/macos/build-dmg.sh              universal (Apple Silicon + Intel)
#   packaging/macos/build-dmg.sh --host-only  this machine's architecture only
#
# The result is dist/Kuro-<version>.dmg: a drag-to-Applications image holding a
# self-contained app bundle. Nothing is installed on the build machine and
# nothing outside dist/ and build/ is written to.
#
# The bundle is not signed. See NOTARISATION at the bottom of this file.

set -euo pipefail

# ---------------------------------------------------------------------------
# Setup
# ---------------------------------------------------------------------------

# The script is run from anywhere; everything below is relative to the repo.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT"

# A double-clicked or CI shell may not have the toolchains on PATH. These are
# the standard install locations; a missing directory is harmless.
export PATH="$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

BUILD_DIR="$ROOT/build/macos"
DIST_DIR="$ROOT/dist"
APP="$BUILD_DIR/Kuro.app"
CONTENTS="$APP/Contents"

HOST_ONLY=0
[ "${1:-}" = "--host-only" ] && HOST_ONLY=1

bold() { printf '\033[1m%s\033[0m\n' "$1"; }
dim()  { printf '\033[2m%s\033[0m\n' "$1"; }
fail() { printf '\033[31m%s\033[0m\n' "$1" >&2; exit 1; }

need() {
  command -v "$1" >/dev/null 2>&1 || fail "$1 is required but not installed."
}

[ "$(uname -s)" = "Darwin" ] || fail "This builds a macOS app, and only runs on macOS."
need cargo
need npm
need lipo
need iconutil
need hdiutil

# The single source of truth for the version is the workspace manifest, so a
# release cannot ship a disk image labelled differently from the binary in it.
VERSION="$(
  awk '/^\[workspace\.package\]/ { inside = 1; next }
       /^\[/ { inside = 0 }
       inside && /^version[[:space:]]*=/ {
         gsub(/[^0-9A-Za-z.\-]/, "", $3); print $3; exit
       }' Cargo.toml
)"
[ -n "$VERSION" ] || fail "Could not read the version out of Cargo.toml."

bold "Building Kuro $VERSION"
dim "$ROOT"
printf '\n'

rm -rf "$BUILD_DIR"
# Payload lives under Resources, and Contents/MacOS holds nothing but the
# bundle executable. That is the conventional layout, and here it is also a
# correctness requirement: macOS filesystems are case-insensitive by default, so
# a launcher named `Kuro` sitting beside the `kuro` command-line binary is the
# same file, and whichever is written second silently replaces the other.
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources/bin" "$DIST_DIR"

# ---------------------------------------------------------------------------
# Binaries
# ---------------------------------------------------------------------------

HOST_TARGET="$(rustc -vV | awk '/^host: / { print $2 }')"

if [ "$HOST_ONLY" -eq 1 ]; then
  TARGETS=("$HOST_TARGET")
  dim "Building for $HOST_TARGET only."
else
  TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
  # Cross-compiling to the other Mac architecture needs its std library. This
  # is a download, so it is reported rather than done silently.
  for target in "${TARGETS[@]}"; do
    if ! rustup target list --installed 2>/dev/null | grep -qx "$target"; then
      bold "Adding the $target toolchain target"
      rustup target add "$target" || fail "Could not add $target. Use --host-only to skip it."
    fi
  done
fi

for target in "${TARGETS[@]}"; do
  bold "Compiling for $target"
  cargo build --release --target "$target" --bin kuro-server --bin kuro
done

# One binary that runs natively on both architectures, so a single disk image
# serves every Mac. Both land in Resources/bin and stay siblings there, which is
# where `kuro serve` looks for `kuro-server`.
for binary in kuro-server kuro; do
  slices=()
  for target in "${TARGETS[@]}"; do
    slices+=("target/$target/release/$binary")
  done
  lipo -create "${slices[@]}" -output "$CONTENTS/Resources/bin/$binary"
  chmod +x "$CONTENTS/Resources/bin/$binary"
done

printf '\n'

# ---------------------------------------------------------------------------
# Web interface
# ---------------------------------------------------------------------------

bold "Building the interface"
if [ -f web/package-lock.json ]; then
  npm --prefix web ci
else
  npm --prefix web install
fi
npm --prefix web run build

# The launcher points KURO_WEB_DIR straight at this, so the server's relative
# search order never comes into it.
cp -R web/dist "$CONTENTS/Resources/web"

printf '\n'

# ---------------------------------------------------------------------------
# Icon
# ---------------------------------------------------------------------------

bold "Rendering the icon"

ICONSET="$BUILD_DIR/Kuro.iconset"
MASTER="$BUILD_DIR/icon-master.png"
mkdir -p "$ICONSET"

# qlmanage is macOS's own renderer and ships with the system, so generating the
# icon costs no build dependency. It writes <name>.png next to the -o directory.
qlmanage -t -s 1024 -o "$BUILD_DIR" "$SCRIPT_DIR/icon.svg" >/dev/null 2>&1
[ -f "$BUILD_DIR/icon.svg.png" ] || fail "Could not rasterise packaging/macos/icon.svg."
mv "$BUILD_DIR/icon.svg.png" "$MASTER"

# The sizes Finder, the Dock and Get Info actually ask for.
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$MASTER" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  sips -z $((size * 2)) $((size * 2)) "$MASTER" \
    --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done

iconutil -c icns "$ICONSET" -o "$CONTENTS/Resources/Kuro.icns"
rm -rf "$ICONSET" "$MASTER"

printf '\n'

# ---------------------------------------------------------------------------
# Bundle metadata and launcher
# ---------------------------------------------------------------------------

cat > "$CONTENTS/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>              <string>Kuro</string>
    <key>CFBundleDisplayName</key>       <string>Kuro</string>
    <key>CFBundleIdentifier</key>        <string>com.kuro.llm</string>
    <key>CFBundleVersion</key>           <string>$VERSION</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleExecutable</key>        <string>Kuro</string>
    <key>CFBundleIconFile</key>          <string>Kuro</string>
    <key>CFBundlePackageType</key>       <string>APPL</string>
    <key>LSMinimumSystemVersion</key>    <string>11.0</string>
    <key>NSHighResolutionCapable</key>   <true/>
</dict>
</plist>
PLIST

# The bundle executable. Starts the server, waits for it to answer, and opens
# the interface in the default browser — the same sequence as running Kuro from
# a terminal, with the errors routed somewhere a Finder launch can show them.
cat > "$CONTENTS/MacOS/Kuro" <<'LAUNCHER'
#!/bin/bash
#
# Kuro's bundle executable.
#
# This process *is* the app: it stays in the foreground for as long as the
# server runs, so quitting Kuro from the Dock stops the server rather than
# orphaning it.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESOURCES="$(cd "$HERE/../Resources" && pwd)"
PORT="${KURO_PORT:-8420}"
URL="http://127.0.0.1:${PORT}"

# Named explicitly rather than left to the search order, so a stray `web`
# directory in whatever the working directory happens to be cannot win.
export KURO_WEB_DIR="$RESOURCES/web"
# So `kuro serve`, if the user puts the command-line binary on their PATH,
# starts the server that shipped with this app rather than hunting for one.
export KURO_SERVER_BIN="$RESOURCES/bin/kuro-server"

# A Finder launch has no terminal, so a failure has to be shown in a dialog or
# it is invisible.
say() {
  /usr/bin/osascript -e "display dialog \"$1\" with title \"Kuro\" buttons {\"OK\"} default button 1 with icon caution" >/dev/null 2>&1
}

# Two servers cannot share the port, and the second fails with a bind error
# that says nothing useful. An instance that is already up is simply opened.
if /usr/bin/curl -fsS --max-time 2 "${URL}/api/health" >/dev/null 2>&1; then
  /usr/bin/open "$URL"
  exit 0
fi

"$RESOURCES/bin/kuro-server" &
SERVER_PID=$!

# Stop the server when the app is quit, rather than leaving it holding the port.
cleanup() {
  if kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null
    wait "$SERVER_PID" 2>/dev/null
  fi
}
trap cleanup EXIT INT TERM

# Wait for it to answer before opening a browser, so the first thing the user
# sees is the interface and not a connection error.
ready=0
for _ in $(seq 1 90); do
  if /usr/bin/curl -fsS --max-time 2 "${URL}/api/health" >/dev/null 2>&1; then
    ready=1
    break
  fi
  # Stop waiting if the process we started has already died.
  kill -0 "$SERVER_PID" 2>/dev/null || break
  sleep 1
done

if [ "$ready" -ne 1 ]; then
  say "Kuro could not start. Port ${PORT} may be in use by another program."
  exit 1
fi

/usr/bin/open "$URL"

wait "$SERVER_PID"
LAUNCHER

chmod +x "$CONTENTS/MacOS/Kuro"

# ---------------------------------------------------------------------------
# Check the bundle before shipping it
# ---------------------------------------------------------------------------
#
# Everything above writes files by path, and a path that lands in the wrong
# place produces a bundle that looks fine in a listing and fails on the user's
# machine. The first version of this script wrote the launcher over the
# command-line binary — two names that differ only in case, which this
# filesystem treats as one file — and nothing complained. So the layout is
# asserted rather than assumed.

for required in \
  "$CONTENTS/Info.plist" \
  "$CONTENTS/Resources/Kuro.icns" \
  "$CONTENTS/Resources/web/index.html" \
  "$CONTENTS/Resources/bin/kuro" \
  "$CONTENTS/Resources/bin/kuro-server" \
  "$CONTENTS/MacOS/Kuro"
do
  [ -e "$required" ] || fail "The bundle is missing ${required#"$APP/"}."
done

# The launcher is a script; the two binaries must not be.
head -c 2 "$CONTENTS/MacOS/Kuro" | grep -q '#!' \
  || fail "Contents/MacOS/Kuro is not the launcher script."
for binary in kuro kuro-server; do
  file -b "$CONTENTS/Resources/bin/$binary" | grep -q 'Mach-O' \
    || fail "Resources/bin/$binary is not a compiled binary — check for a name collision."
done

# Nothing but the launcher belongs in MacOS, which is what keeps the collision
# above impossible rather than merely fixed.
extra="$(find "$CONTENTS/MacOS" -mindepth 1 ! -name Kuro)"
[ -z "$extra" ] || fail "Unexpected files in Contents/MacOS:"$'\n'"$extra"

# ---------------------------------------------------------------------------
# Disk image
# ---------------------------------------------------------------------------

bold "Building the disk image"

STAGE="$BUILD_DIR/dmg"
DMG="$DIST_DIR/Kuro-$VERSION.dmg"

rm -rf "$STAGE"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/Kuro.app"
# The half of "drag this into that" the user is expected to do.
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
dim "$(du -h "$DMG" | cut -f1) · $(lipo -archs "$CONTENTS/Resources/bin/kuro-server")"
printf '\n'

# NOTARISATION
#
# The bundle is unsigned, so Gatekeeper will refuse to open it on first launch
# with "Kuro cannot be opened because the developer cannot be verified". The
# user's way past that is to right-click the app and choose Open, which offers
# the same warning with an Open button on it.
#
# Signing and notarising needs a paid Apple Developer ID, which this build does
# not assume. With one, the two steps to add here are:
#
#   codesign --deep --force --options runtime --timestamp \
#     --sign "Developer ID Application: <name> (<team id>)" "$APP"
#   xcrun notarytool submit "$DMG" --apple-id <id> --team-id <team> \
#     --password <app-specific-password> --wait
#   xcrun stapler staple "$DMG"
dim "Unsigned: first launch needs right-click → Open. See NOTARISATION in this script."
