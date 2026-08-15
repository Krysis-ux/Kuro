#!/bin/bash

set -euo pipefail


SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT"

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
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources/bin" "$DIST_DIR"


HOST_TARGET="$(rustc -vV | awk '/^host: / { print $2 }')"

if [ "$HOST_ONLY" -eq 1 ]; then
  TARGETS=("$HOST_TARGET")
  dim "Building for $HOST_TARGET only."
else
  TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
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

for binary in kuro-server kuro; do
  slices=()
  for target in "${TARGETS[@]}"; do
    slices+=("target/$target/release/$binary")
  done
  lipo -create "${slices[@]}" -output "$CONTENTS/Resources/bin/$binary"
  chmod +x "$CONTENTS/Resources/bin/$binary"
done

printf '\n'


bold "Building the interface"
if [ -f web/package-lock.json ]; then
  npm --prefix web ci
else
  npm --prefix web install
fi
npm --prefix web run build

cp -R web/dist "$CONTENTS/Resources/web"

printf '\n'


bold "Rendering the icon"

ICONSET="$BUILD_DIR/Kuro.iconset"
MASTER="$BUILD_DIR/icon-master.png"
mkdir -p "$ICONSET"

qlmanage -t -s 1024 -o "$BUILD_DIR" "$SCRIPT_DIR/icon.svg" >/dev/null 2>&1
[ -f "$BUILD_DIR/icon.svg.png" ] || fail "Could not rasterise packaging/macos/icon.svg."
mv "$BUILD_DIR/icon.svg.png" "$MASTER"

for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$MASTER" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  sips -z $((size * 2)) $((size * 2)) "$MASTER" \
    --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done

iconutil -c icns "$ICONSET" -o "$CONTENTS/Resources/Kuro.icns"
rm -rf "$ICONSET" "$MASTER"

printf '\n'


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

cat > "$CONTENTS/MacOS/Kuro" <<'LAUNCHER'
#!/bin/bash

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESOURCES="$(cd "$HERE/../Resources" && pwd)"
PORT="${KURO_PORT:-8420}"
URL="http://127.0.0.1:${PORT}"

export KURO_WEB_DIR="$RESOURCES/web"
export KURO_SERVER_BIN="$RESOURCES/bin/kuro-server"

if ! command -v kuro >/dev/null 2>&1; then
  mkdir -p "$HOME/.local/bin" 2>/dev/null &&
    ln -sf "$RESOURCES/bin/kuro" "$HOME/.local/bin/kuro" 2>/dev/null
fi

say() {
  /usr/bin/osascript -e "display dialog \"$1\" with title \"Kuro\" buttons {\"OK\"} default button 1 with icon caution" >/dev/null 2>&1
}

if /usr/bin/curl -fsS --max-time 2 "${URL}/api/health" >/dev/null 2>&1; then
  /usr/bin/open "$URL"
  exit 0
fi

"$RESOURCES/bin/kuro-server" &
SERVER_PID=$!

cleanup() {
  if kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null
    wait "$SERVER_PID" 2>/dev/null
  fi
}
trap cleanup EXIT INT TERM

ready=0
for _ in $(seq 1 90); do
  if /usr/bin/curl -fsS --max-time 2 "${URL}/api/health" >/dev/null 2>&1; then
    ready=1
    break
  fi
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

head -c 2 "$CONTENTS/MacOS/Kuro" | grep -q '#!' \
  || fail "Contents/MacOS/Kuro is not the launcher script."
for binary in kuro kuro-server; do
  file -b "$CONTENTS/Resources/bin/$binary" | grep -q 'Mach-O' \
    || fail "Resources/bin/$binary is not a compiled binary — check for a name collision."
done

extra="$(find "$CONTENTS/MacOS" -mindepth 1 ! -name Kuro)"
[ -z "$extra" ] || fail "Unexpected files in Contents/MacOS:"$'\n'"$extra"


bold "Building the disk image"

STAGE="$BUILD_DIR/dmg"
DMG="$DIST_DIR/Kuro-$VERSION.dmg"

rm -rf "$STAGE"
mkdir -p "$STAGE"
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
dim "$(du -h "$DMG" | cut -f1) · $(lipo -archs "$CONTENTS/Resources/bin/kuro-server")"
printf '\n'

dim "Unsigned: first launch needs right-click → Open."
