# Kuro LLM

A local model server. Download GGUF weights, run them on your own machine, and
talk to them through a minimal web interface, a terminal, or the OpenAI API.

Models run locally by default, and nothing leaves the machine unless you switch on
something that says it will: web search, an MCP server, or a provider you hold the
key for. Each of those is a control you flip, never a default and never a decision
the model makes for you.

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
  sampling sliders. The raw knobs live in Settings, on sliders that also accept an
  exact typed value.
- **Web search that works on install.** DuckDuckGo needs no key; Brave, Tavily and
  a SearXNG instance are supported when you want a stable API. Turning the switch
  on searches *before* the model answers and puts the results in its context, so it
  works even on models too small to request a tool themselves. Sources are listed
  by the interface rather than left to the model, which is what stops a small model
  inventing plausible URLs.
- **MCP tool servers**, over stdio and streamable HTTP. A short recommended list
  is built in (Context7, DeepWiki, Exa, GitHub, Hugging Face, Filesystem, Fetch,
  Sequential Thinking); adding a server connects to it immediately and reports what
  it found rather than saving silently and failing later.
- **Memory.** `remember` and `recall` are real tools backed by SQLite, and saved
  facts are put in front of the model automatically so memory works without the
  model having to think to look.
- **Skills** — installable instruction packs for Rust, Python, TypeScript, Go,
  SQL, shell, code review, debugging, explaining and careful step-by-step
  reasoning. Prompt guidance only, which is what makes them safe to toggle, and the
  single highest-leverage improvement available on a small local model.
- **A brief for the model.** Every turn starts with a system prompt stating where
  it is running, whether web and memory are on *this turn*, which tools it may
  call, and that inventing a URL is not allowed. Without it a model answers
  questions about its own capabilities from training data, which is never about
  this deployment.
- **Providers** — bring your own key for OpenRouter, Anthropic, OpenAI, Groq,
  DeepSeek, Mistral, Together, a rented RunPod/Vast box, or any OpenAI-compatible
  URL. Their models appear in the same picker as local ones, marked as leaving the
  machine, and everything else works identically. Keys live in an owner-only file
  beside the database, never in it.
- **Hugging Face search** from inside the app, filtered to GGUF, with the
  quantizations each repository publishes and a fit estimate per result.
- **Projects** — standing instructions plus a grouping of conversations. What you
  write in a project is added to the model's brief for every chat in it, so "this is
  a Rust workspace on the 2021 edition" is said once rather than at the top of every
  conversation. Deleting a project releases its chats; it never deletes them.
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

Listed honestly, because the interface shows some of these as disabled rather than
hiding them:

- **Images, audio, video and PDF attachments.** Text and code files are read into
  the prompt; the other modalities need engine work Kuro does not have yet. The
  `+` menu says which model capability each one would need.
- **Files attached to a project.** Projects carry instructions today, not a
  document set.
- **Saved prompt templates.**
- **Folder access** without going through the Filesystem MCP server.
- **Multi-part GGUF weights.** Repositories that publish only shards are shown in
  search and marked, rather than downloaded into something that cannot load.
- **Authentication.** The server is loopback-only for exactly this reason; LAN
  serving waits on API keys.
- **Fine-tuning.** Deliberately a separate future application.

There is no Kuro-hosted cloud and no plan for one. "Providers" means *your*
account and *your* key, and the request goes straight from this machine to them.

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
| `crates/kuro-core` | Models, downloads, engine supervision, storage, hardware detection, tools, MCP client, providers, the model's system prompt. No HTTP, no CLI parsing. |
| `crates/kuro-server` | The Axum daemon: routing, streaming, static files. |
| `crates/kuro-cli` | The `kuro` command. A thin client over the same HTTP API the browser uses. |
| `web` | React + TypeScript interface. Plain CSS with monochrome design tokens. |

Data lives in `~/Library/Application Support/Kuro` — SQLite database, model
weights, engine builds and per-engine logs. Set `KURO_HOME` to move it.

API keys and bearer tokens are the one thing kept *out* of the database, in
`credentials.json` beside it with `0600` permissions. The database is what gets
copied, backed up and attached to bug reports; a provider key travelling with it
would be a bad default. The database stores only a reference to each entry.

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

337 tests, no network access required — the search parsers, the MCP protocol
handling and the tool loop are all tested against recorded payloads rather than
against live services, so the suite does not break when someone else's site
changes. What *does* need a live check is whether a provider's markup still parses;
Tools → Web search has a Test button for that.

Schema upgrades have their own tests that migrate a database with the *version 1*
shape and assert nothing is lost. That path is worth testing separately: a fresh
database gets every column from `CREATE TABLE`, so a mistake in the upgrade order
passes every other test and then stops the daemon from starting for anyone who
already had data.

## Licence

Not yet chosen.

The `llama.cpp` binaries Kuro downloads at runtime are MIT licensed and remain
the property of their authors; Kuro does not redistribute them. Model weights
carry their own licences, which you accept with the model publisher.
