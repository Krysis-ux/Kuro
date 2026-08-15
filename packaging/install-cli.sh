#!/bin/bash

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

BINARY="$(cd "$(dirname "$BINARY")" && pwd)/$(basename "$BINARY")"

on_path() {
  case ":${PATH}:" in
    *":$1:"*) return 0 ;;
    *) return 1 ;;
  esac
}

TARGET=""
for candidate in "$HOME/.local/bin" "/usr/local/bin" "$HOME/bin"; do
  if [ -d "$candidate" ] && [ -w "$candidate" ] && on_path "$candidate"; then
    TARGET="$candidate"
    break
  fi
done

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
