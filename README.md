# Kuro

Run AI models on your Mac. Download a model, chat with it, or let it work on your code.
Local models stay on your machine. Cloud providers and free pools are there when you want them.

## Install

1. Download `Kuro-<version>.dmg` from the [latest release](https://github.com/Krysis-ux/Kuro/releases/latest).
2. Open it and drag Kuro into Applications.
3. First launch: right-click Kuro → **Open** (the app is not signed by Apple yet).

Needs macOS 14 or later. Quitting Kuro stops the server and unloads models.
The same interface is also at http://127.0.0.1:8420 in a browser.

## What it does

- **Chat** with local models or your own provider keys.
- **Code**: open a folder, pick a mode (Ask, Plan, Agent, Bypass) and let the model read, edit and run it.
- **Models**: recommended picks for your Mac, plus any GGUF repo on Hugging Face.
- **Providers**: OpenRouter, OpenAI, Groq and more, or the free pool.
- **Skills, subagents, `/` commands, MCP servers, web search and automations.**

## Command line

```bash
kuro serve                     # start the server
kuro pull qwen3-4b             # download a model
kuro run qwen3-4b "hello"      # chat
kuro list                      # installed models
kuro launch codex              # run another coding tool on a Kuro model
kuro help                      # everything else
```

Any OpenAI client works too: `http://127.0.0.1:8420/v1`.

## Build from source

Needs [Rust](https://rustup.rs), [Node.js](https://nodejs.org) 20+ and the Xcode Command Line Tools.

```bash
packaging/macos/build-app.sh              # build/macos/Kuro.app
packaging/macos/build-dmg.sh              # dist/Kuro-<version>.dmg (Apple Silicon + Intel)
packaging/macos/build-dmg.sh --host-only  # faster, this Mac only
```

Server only:

```bash
cargo build --release
npm --prefix apps/web ci && npm --prefix apps/web run build
./target/release/kuro serve
```

## Layout

| Folder | What |
| --- | --- |
| `apps/macos` | The Mac app (SwiftUI) |
| `apps/web` | The browser interface (React) |
| `crates/kuro-core` | Models, engines, storage, tools, providers |
| `crates/kuro-server` | The local server (`/api`, `/v1`, web UI) |
| `crates/kuro-cli` | The `kuro` command |
| `packaging` | Build and install scripts |

Data lives in `~/Library/Application Support/Kuro` (set `KURO_HOME` to move it).
API keys are kept in `credentials.json` there, not in the database.
The server listens on port 8420, loopback only (`KURO_PORT` to change).

## Tests

```bash
cargo test --workspace
swift run --package-path apps/macos KuroChecks
npm --prefix apps/web run typecheck
```

## Notices

See [NOTICES.md](NOTICES.md).
