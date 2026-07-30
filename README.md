# Kuro LLM

A local model server. Download GGUF weights, run them on your own machine, and
talk to them through a minimal web interface, a terminal, or the OpenAI API.

Everything runs locally. Nothing is sent anywhere.

```
kuro serve                    # start the server
kuro pull qwen3-4b            # download a model
kuro run qwen3-4b "hello"     # talk to it
```

Then open <http://127.0.0.1:8420>.

## What it does today

- **Runs GGUF models** through `llama.cpp`, one supervised child process per
  loaded model. The engine binary is downloaded automatically on first use — no
  manual setup, no compiling.
- **Finds models for you.** A curated list of recommended models, each labelled
  with how well it fits this machine's memory, plus any Hugging Face repository
  you paste. Downloads resume after an interruption and are checksum-verified
  when the source publishes one.
- **Chat interface** that starts centred and moves to the bottom once you begin,
  with markdown, streaming, and a per-message inspector showing token counts,
  tokens/second and time to first token.
- **An effort control** (low / balanced / high / max) instead of a wall of
  sampling sliders. The raw knobs live in Settings.
- **OpenAI-compatible API**, so existing tools work by changing a base URL:
  ```
  OPENAI_BASE_URL=http://127.0.0.1:8420/v1
  OPENAI_API_KEY=not-needed
  ```
- **A real CLI** — `serve`, `pull`, `list`, `run`, `ps`, `status`, `stop`, `rm`,
  `preview`, `recommended`.
- **Automatic hardware defaults.** Context size, GPU layers and thread count are
  derived from the machine and overridable in Settings.
- **Idle unloading** so a model you have stopped using gives its memory back.

## Not built yet

Listed honestly, because the interface shows placeholders for some of it:

- MCP tool servers, and the web-search toggle in the composer.
- Attaching folders, saved prompt templates.
- Cloud connectors for your own RunPod / Vast.ai / Lambda Labs account.
- Kuro's own hosted cloud (the "coming soon" card in Settings has no backend).
- Fine-tuning. That is deliberately a separate future application.

## Requirements

- macOS on Apple Silicon or Intel. Other platforms are structured for but not
  yet built — see `asset_pattern` in `crates/kuro-core/src/engine/bootstrap.rs`.
- [Rust](https://rustup.rs) and [Node.js](https://nodejs.org) 18+ to build.

## Building

```bash
# Backend
cargo build --release

# Frontend
cd web && npm install && npm run build && cd ..

# Run
./target/release/kuro serve
```

Put `target/release` on your `PATH` to use `kuro` from anywhere.

If Rust was installed through Homebrew's `rustup`, its binaries are not on the
default `PATH`:

```bash
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
```

### Developing the frontend

```bash
cd web && npm run dev
```

Vite serves on port 5173 and proxies `/api` and `/v1` to the running daemon, so
frontend changes do not require a Rust rebuild.

## How it works

```
      browser / CLI / any OpenAI client
                     |
                 kuro-server                     one process
      ┌──────────────┴──────────────┐
      |  /api/*        native API   |
      |  /v1/*         OpenAI API   |
      |  /             web UI       |
      └──────────────┬──────────────┘
                     |
               EngineManager                     load, supervise, unload
      ┌──────────────┼──────────────┐
  llama-server   llama-server   llama-server     one per loaded model
   (port 392xx)   (port 392xx)   (port 392xx)
```

Kuro does not implement inference. It manages models, decides what runs where,
supervises the engine processes, and presents one coherent interface over them.
Running each model out-of-process means a bad GGUF or an out-of-memory kill takes
down that engine only — never the server. A crashed engine is dropped from the
running set and the next request starts a fresh one.

### Layout

| Path | What lives there |
| --- | --- |
| `crates/kuro-core` | Models, downloads, engine supervision, storage, hardware detection. No HTTP, no CLI parsing. |
| `crates/kuro-server` | The Axum daemon: routing, streaming, static files. |
| `crates/kuro-cli` | The `kuro` command. A thin client over the same HTTP API the browser uses. |
| `web` | React + TypeScript interface. Plain CSS with monochrome design tokens. |

Data lives in `~/Library/Application Support/Kuro` — SQLite database, model
weights, engine builds and per-engine logs. Set `KURO_HOME` to move it.

### Ports

| Port | Purpose |
| --- | --- |
| 8420 | Kuro. Loopback only; there is no authentication yet, so it must not be exposed to a network. |
| 39200–39299 | Internal engine processes. Never addressed directly. |

Override Kuro's port with `KURO_PORT`.

## Testing

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd web && npm run typecheck
```

## Licence

Not yet chosen.

The `llama.cpp` binaries Kuro downloads at runtime are MIT licensed and remain
the property of their authors; Kuro does not redistribute them. Model weights
carry their own licences, which you accept with the model publisher.
