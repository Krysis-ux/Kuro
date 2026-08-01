#!/bin/bash
#
# Start Kuro.
#
# Double-click this file in Finder, or run it from a terminal. It builds
# anything that is missing, starts the server, and opens the interface.
#
# Closing this window, or pressing Ctrl-C, stops the server.

set -uo pipefail

# Finder runs this from the user's home directory, so the project root has to be
# derived from the script's own location rather than from the working directory.
# The server also looks for `web/dist` relative to the working directory.
cd "$(dirname "${BASH_SOURCE[0]}")" || exit 1
ROOT="$(pwd)"

PORT="${KURO_PORT:-8420}"
URL="http://127.0.0.1:${PORT}"

bold() { printf '\033[1m%s\033[0m\n' "$1"; }
dim()  { printf '\033[2m%s\033[0m\n' "$1"; }
warn() { printf '\033[33m%s\033[0m\n' "$1"; }
fail() { printf '\033[31m%s\033[0m\n' "$1" >&2; }

# A GUI launch inherits a minimal PATH that has none of the toolchains on it.
# These are the standard install locations; a missing directory is harmless.
export PATH="$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

# Stop the server when this window closes or Ctrl-C is pressed, rather than
# leaving an orphan holding the port.
SERVER_PID=""
cleanup() {
  if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
    printf '\n'
    dim "Stopping Kuro…"
    kill "$SERVER_PID" 2>/dev/null
    wait "$SERVER_PID" 2>/dev/null
  fi
}
trap cleanup EXIT INT TERM

# Wait for the server to answer, and say whether it did.
wait_for_server() {
  for _ in $(seq 1 90); do
    if curl -fsS --max-time 2 "${URL}/api/health" >/dev/null 2>&1; then
      return 0
    fi
    # Stop waiting if the process we started has already died.
    if [ -n "$SERVER_PID" ] && ! kill -0 "$SERVER_PID" 2>/dev/null; then
      return 1
    fi
    sleep 1
  done
  return 1
}

printf '\n'
bold "Kuro"
dim "$ROOT"
printf '\n'

# An already-running instance is opened rather than fought with. Two servers
# cannot share the port, and the second would fail with a confusing bind error.
if curl -fsS --max-time 2 "${URL}/api/health" >/dev/null 2>&1; then
  bold "Kuro is already running."
  dim "Opening ${URL}"
  open "$URL" 2>/dev/null || true
  printf '\n'
  dim "This window started nothing, so closing it leaves Kuro running."
  dim "Press any key to close."
  read -r -n 1 -s
  trap - EXIT
  exit 0
fi

# ---------------------------------------------------------------------------
# Build whatever is missing
# ---------------------------------------------------------------------------

# Whether any source file is newer than something already built.
#
# The launcher used to check only whether the binary *existed*, which meant that
# after any change to the code it cheerfully started the previous build — and the
# symptom was not a build error but a running application whose newer interface
# called endpoints its older server had never heard of. Pages failed to load and
# nothing said why.
stale() {
  local built="$1"
  shift
  [ -e "$built" ] || return 0
  [ -n "$(find "$@" -type f -newer "$built" -print -quit 2>/dev/null)" ]
}

if stale "target/release/kuro-server" crates Cargo.toml Cargo.lock; then
  if ! command -v cargo >/dev/null 2>&1; then
    fail "Rust is not installed, and Kuro has not been built yet."
    printf '\n'
    echo "Install Rust from https://rustup.rs and run this again:"
    echo "    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    printf '\n'
    dim "Press any key to close."
    read -r -n 1 -s
    exit 1
  fi

  if [ -x "target/release/kuro-server" ]; then
    bold "The server has changed since it was last built. Rebuilding."
  else
    bold "Building the server."
    dim "First run only. This takes a few minutes."
  fi
  printf '\n'
  if ! cargo build --release; then
    printf '\n'
    fail "The build failed. The errors above say why."
    dim "Press any key to close."
    read -r -n 1 -s
    exit 1
  fi
  printf '\n'
fi

if stale "web/dist/index.html" web/src web/index.html web/package.json; then
  if ! command -v npm >/dev/null 2>&1; then
    warn "Node.js is not installed, so the web interface cannot be built."
    echo "The API will still start. Install Node 18+ from https://nodejs.org"
    echo "and run this again to get the interface."
    printf '\n'
  else
    if [ -f "web/dist/index.html" ]; then
      bold "The interface has changed since it was last built. Rebuilding."
    else
      bold "Building the interface."
    fi
    printf '\n'
    if [ ! -d "web/node_modules" ]; then
      (cd web && npm install) || {
        fail "npm install failed."
        dim "Press any key to close."
        read -r -n 1 -s
        exit 1
      }
    fi
    if ! (cd web && npm run build); then
      printf '\n'
      fail "The interface build failed. The errors above say why."
      dim "Press any key to close."
      read -r -n 1 -s
      exit 1
    fi
    printf '\n'
  fi
fi

# ---------------------------------------------------------------------------
# Start
# ---------------------------------------------------------------------------

bold "Starting Kuro…"

KURO_PORT="$PORT" ./target/release/kuro-server &
SERVER_PID=$!

if ! wait_for_server; then
  printf '\n'
  fail "Kuro did not start. The messages above say why."
  printf '\n'
  dim "Press any key to close."
  read -r -n 1 -s
  exit 1
fi

printf '\n'
bold "Kuro is running at ${URL}"
printf '\n'
open "$URL" 2>/dev/null || dim "Open ${URL} in a browser."
printf '\n'
dim "Leave this window open. Press Ctrl-C, or close it, to stop Kuro."
printf '\n'

# Hand the window over to the server's own output, so its logs are what is shown
# from here on.
wait "$SERVER_PID"
