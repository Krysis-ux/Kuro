#!/bin/bash
#
# Build Kuro.app: the native macOS app, with the server and the `kuro` command inside it.
#
#   packaging/macos/build-app.sh              this Mac's architecture
#   packaging/macos/build-app.sh --universal  Apple Silicon + Intel
#
# Writes build/macos/Kuro.app. Everything is incremental, so re-running after a
# change rebuilds only what changed. Needs Rust and the Xcode Command Line
# Tools; Node.js is optional (it builds the browser interface the server also serves).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT"

# A double-clicked shell may not have the toolchains on PATH.
export PATH="$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
for toolchain in "$HOME"/.rustup/toolchains/stable-*/bin; do
  [ -d "$toolchain" ] && export PATH="$PATH:$toolchain"
done

APP="$ROOT/build/macos/Kuro.app"
CONTENTS="$APP/Contents"
UNIVERSAL=0
[ "${1:-}" = "--universal" ] && UNIVERSAL=1

bold() { printf '\033[1m%s\033[0m\n' "$1"; }
dim()  { printf '\033[2m%s\033[0m\n' "$1"; }
warn() { printf '\033[33m%s\033[0m\n' "$1"; }
fail() { printf '\033[31m%s\033[0m\n' "$1" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || fail "$1 is required but not installed. $2"; }

[ "$(uname -s)" = "Darwin" ] || fail "Kuro.app can only be built on macOS."
need cargo "Install Rust from https://rustup.rs"
need swift "Install the Command Line Tools: xcode-select --install"
need lipo "Install the Command Line Tools: xcode-select --install"

# The workspace manifest is the single source of the version.
VERSION="$(
  awk '/^\[workspace\.package\]/ { inside = 1; next }
       /^\[/ { inside = 0 }
       inside && /^version[[:space:]]*=/ { gsub(/[^0-9A-Za-z.\-]/, "", $3); print $3; exit }' Cargo.toml
)"
[ -n "$VERSION" ] || fail "Could not read the version out of Cargo.toml."

if [ "$UNIVERSAL" -eq 1 ]; then
  ARCHES=(arm64 x86_64)
else
  ARCHES=("$(uname -m)")
fi

rust_target() { [ "$1" = "arm64" ] && echo aarch64-apple-darwin || echo x86_64-apple-darwin; }

bold "Building Kuro $VERSION for ${ARCHES[*]}"

# ---------------------------------------------------------------------------
# Server and command line (Rust)
# ---------------------------------------------------------------------------

for arch in "${ARCHES[@]}"; do
  target="$(rust_target "$arch")"
  if [ "$UNIVERSAL" -eq 1 ] && command -v rustup >/dev/null 2>&1 \
    && ! rustup target list --installed 2>/dev/null | grep -qx "$target"; then
    bold "Adding the $target toolchain target"
    rustup target add "$target" || fail "Could not add $target. Build without --universal to skip it."
  fi
  bold "Compiling the server ($target)"
  cargo build --release --target "$target" --bin kuro-server --bin kuro
done

# ---------------------------------------------------------------------------
# The app (Swift)
# ---------------------------------------------------------------------------

for arch in "${ARCHES[@]}"; do
  bold "Compiling the app ($arch)"
  swift build -c release --package-path macos --triple "$arch-apple-macosx14.0" --product Kuro
done

# ---------------------------------------------------------------------------
# Browser interface (optional)
# ---------------------------------------------------------------------------

WEB_BUILT=0
if command -v npm >/dev/null 2>&1; then
  if [ ! -f web/dist/index.html ] || [ -n "$(find web/src web/index.html web/package.json -newer web/dist/index.html -print -quit)" ]; then
    bold "Building the browser interface"
    [ -d web/node_modules ] || npm --prefix web ci
    npm --prefix web run build
  fi
  WEB_BUILT=1
else
  warn "Node.js is not installed, so the browser interface at http://127.0.0.1:8420 is left out. The app does not need it."
fi

# ---------------------------------------------------------------------------
# Assemble the bundle
# ---------------------------------------------------------------------------

bold "Assembling Kuro.app"
rm -rf "$APP"
# Contents/MacOS holds only the app. The `kuro` command lives in Resources/bin:
# the filesystem is case-insensitive, so `Kuro` and `kuro` side by side would be one file.
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources/bin"

lipo_into() {
  local output="$1"
  shift
  lipo -create "$@" -output "$output"
  chmod +x "$output"
}

slices() {
  local binary="$1"
  for arch in "${ARCHES[@]}"; do echo "target/$(rust_target "$arch")/release/$binary"; done
}

# shellcheck disable=SC2046
lipo_into "$CONTENTS/Resources/bin/kuro-server" $(slices kuro-server)
# shellcheck disable=SC2046
lipo_into "$CONTENTS/Resources/bin/kuro" $(slices kuro)
app_slices=()
for arch in "${ARCHES[@]}"; do
  app_slices+=("$(swift build -c release --package-path macos --triple "$arch-apple-macosx14.0" --show-bin-path)/Kuro")
done
lipo_into "$CONTENTS/MacOS/Kuro" "${app_slices[@]}"

[ "$WEB_BUILT" -eq 1 ] && cp -R web/dist "$CONTENTS/Resources/web"
# Model and provider logos, shared with the browser interface.
cp -R web/src/assets/logos "$CONTENTS/Resources/logos"

# The icon, rendered from the SVG with macOS's own Quick Look renderer.
ICON_WORK="$ROOT/build/macos/icon"
rm -rf "$ICON_WORK"
mkdir -p "$ICON_WORK/Kuro.iconset"
qlmanage -t -s 1024 -o "$ICON_WORK" "$SCRIPT_DIR/icon.svg" >/dev/null 2>&1
[ -f "$ICON_WORK/icon.svg.png" ] || fail "Could not rasterise packaging/macos/icon.svg."
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$ICON_WORK/icon.svg.png" --out "$ICON_WORK/Kuro.iconset/icon_${size}x${size}.png" >/dev/null
  sips -z $((size * 2)) $((size * 2)) "$ICON_WORK/icon.svg.png" --out "$ICON_WORK/Kuro.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICON_WORK/Kuro.iconset" -o "$CONTENTS/Resources/Kuro.icns"
rm -rf "$ICON_WORK"

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
    <key>LSMinimumSystemVersion</key>    <string>14.0</string>
    <key>LSApplicationCategoryType</key> <string>public.app-category.developer-tools</string>
    <key>NSHighResolutionCapable</key>   <true/>
    <key>NSAppTransportSecurity</key>
    <dict>
        <!-- The app talks to its own server, and previews dev servers, over plain HTTP on this Mac. -->
        <key>NSAllowsLocalNetworking</key><true/>
    </dict>
</dict>
</plist>
PLIST

# Check the layout rather than assume it.
for required in Info.plist Resources/Kuro.icns Resources/logos MacOS/Kuro Resources/bin/kuro Resources/bin/kuro-server; do
  [ -e "$CONTENTS/$required" ] || fail "The bundle is missing Contents/$required."
done
for binary in MacOS/Kuro Resources/bin/kuro Resources/bin/kuro-server; do
  file -b "$CONTENTS/$binary" | grep -q 'Mach-O' || fail "Contents/$binary is not a compiled binary."
done

# Ad-hoc signature, so the bundle's contents are sealed. Distribution still
# needs a Developer ID; see NOTARISATION in build-dmg.sh.
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || warn "Could not ad-hoc sign the app; it will still run locally."

bold "Built $APP"
dim "$(lipo -archs "$CONTENTS/MacOS/Kuro")"
