#!/bin/bash
#
# Put `kuro` on the PATH.
#
#   packaging/install-cli.sh [path-to-kuro-binary]
#
# Without an argument this links the binary from target/release. The app bundle
# passes its own copy from Contents/Resources/bin.
#
# Why this exists: `cargo build` produces a working `kuro` and puts it somewhere
# nothing looks. The README said "put target/release on your PATH", which is a
# sentence, not an install — so `kuro` was not a command, and the first thing
# anybody typed after building answered "command not found". Building software
# and installing it are two steps, and only one of them was happening.
#
# Nothing here needs a password. A directory that would require one is skipped
# rather than escalated to: an installer that asks for a root password to place
# a symlink has misjudged what it is doing.

set -uo pipefail

bold() { printf '\033[1m%s\033[0m\n' "$1"; }
dim()  { printf '\033[2m%s\033[0m\n' "$1"; }
warn() { printf '\033[33m%s\033[0m\n' "$1"; }
fail() { printf '\033[31m%s\033[0m\n' "$1" >&2; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

BINARY="${1:-$ROOT/target/release/kuro}"

if [ ! -x "$BINARY" ]; then
  fail "No kuro binary at $BINARY"
  dim "Build it first:  cargo build --release"
  exit 1
fi

# Resolved so the symlink survives being run from a relative path.
BINARY="$(cd "$(dirname "$BINARY")" && pwd)/$(basename "$BINARY")"

# Whether a directory is already somewhere the shell looks.
on_path() {
  case ":${PATH}:" in
    *":$1:"*) return 0 ;;
    *) return 1 ;;
  esac
}

# Where to put it.
#
# Ordered by how little explaining each one needs afterwards. A directory
# already on PATH and already writable means `kuro` works in the next shell with
# no further instructions, which is the only outcome that counts as installed.
TARGET=""
for candidate in "$HOME/.local/bin" "/usr/local/bin" "$HOME/bin"; do
  if [ -d "$candidate" ] && [ -w "$candidate" ] && on_path "$candidate"; then
    TARGET="$candidate"
    break
  fi
done

# Nothing suitable exists yet. `~/.local/bin` is the conventional place for a
# user-owned binary on both macOS and Linux, so it is created rather than
# reaching for a directory that needs root.
NEEDS_PATH_LINE=0
if [ -z "$TARGET" ]; then
  TARGET="$HOME/.local/bin"
  mkdir -p "$TARGET" || {
    fail "Could not create $TARGET"
    exit 1
  }
  on_path "$TARGET" || NEEDS_PATH_LINE=1
fi

LINK="$TARGET/kuro"

# An existing link to this same binary is not a problem worth reporting, and a
# *different* file of that name is not one to silently overwrite.
if [ -e "$LINK" ] && [ ! -L "$LINK" ]; then
  fail "$LINK already exists and is not a symlink."
  dim "Move it aside and run this again."
  exit 1
fi

ln -sf "$BINARY" "$LINK" || {
  fail "Could not link $LINK"
  exit 1
}

bold "kuro installed"
dim "$LINK -> $BINARY"

if [ "$NEEDS_PATH_LINE" = "1" ]; then
  printf '\n'
  warn "$TARGET is not on your PATH yet."
  echo "Add this line to your shell profile, then open a new terminal:"
  printf '\n'
  # Named for the shell actually in use rather than guessed at: telling a zsh
  # user to edit .bashrc produces a PATH that never changes and a bug report.
  case "${SHELL##*/}" in
    zsh)  echo "    echo 'export PATH=\"$TARGET:\$PATH\"' >> ~/.zshrc" ;;
    bash) echo "    echo 'export PATH=\"$TARGET:\$PATH\"' >> ~/.bash_profile" ;;
    fish) echo "    fish_add_path $TARGET" ;;
    *)    echo "    export PATH=\"$TARGET:\$PATH\"" ;;
  esac
  printf '\n'
else
  printf '\n'
  dim "Try it:  kuro status"
fi
