-- Kuro LLM schema, version 5.
--
-- Every statement is `IF NOT EXISTS`, so the whole file is safe to re-run
-- against a database created by an earlier version. Columns added after a table
-- shipped cannot be expressed that way; those go through `add_column_if_missing`
-- in `db/mod.rs` instead.
--
-- Note: `models` holds only models the user has actually pulled. The curated
-- "recommended models" list is static data in `catalog::curated` and is merged
-- with these rows at the API layer, so refreshing recommendations in a new
-- release never requires a database migration.

CREATE TABLE IF NOT EXISTS models (
    id                TEXT PRIMARY KEY,
    display_name      TEXT NOT NULL,
    source            TEXT NOT NULL,                  -- curated | huggingface | local
    hf_repo           TEXT,
    hf_file           TEXT,
    quant             TEXT,
    param_count       TEXT,
    family            TEXT,
    capabilities      TEXT NOT NULL DEFAULT '[]',     -- JSON array: ["tools","vision","reasoning"]
    context_length    INTEGER,
    file_path         TEXT,
    file_size_bytes   INTEGER,
    sha256            TEXT,
    status            TEXT NOT NULL DEFAULT 'downloading',  -- downloading | ready | error
    error             TEXT,
    added_at          TEXT NOT NULL,
    last_used_at      TEXT
);

-- A project groups conversations under standing instructions. The instructions
-- are appended to the model's brief for every conversation in the project, which
-- is the whole feature: "always answer as if reviewing production Rust" said once
-- rather than at the top of every chat.
CREATE TABLE IF NOT EXISTS projects (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    description  TEXT NOT NULL DEFAULT '',
    instructions TEXT NOT NULL DEFAULT '',
    -- Model and tool defaults for chats started here, so a project can be
    -- "the one where web search is always on".
    model_id     TEXT,
    tool_groups  TEXT,                                -- JSON array, null means inherit
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_projects_updated
    ON projects (updated_at DESC);

-- A coding workspace is a folder on this machine plus a mode saying how much a
-- model may do inside it. It is the only thing in Kuro that grants file access:
-- the chat surface has no file tools at all, whatever is enabled elsewhere.
CREATE TABLE IF NOT EXISTS workspaces (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    root_path  TEXT NOT NULL,
    model_id   TEXT,
    -- ask | plan | agent. Defaults to plan, which cannot change anything.
    mode       TEXT NOT NULL DEFAULT 'plan',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workspaces_updated
    ON workspaces (updated_at DESC);

-- Every file a model changed, with what was there before it. This is what makes
-- agent mode reasonable to offer: a change that can be put back is one the user
-- can afford to let happen without being asked first.
--
-- Deleting a workspace clears its history, because the history is only useful
-- against the folder it was recorded for.
CREATE TABLE IF NOT EXISTS workspace_changes (
    id              TEXT PRIMARY KEY,
    workspace_id    TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    conversation_id TEXT REFERENCES conversations (id) ON DELETE SET NULL,
    -- Relative to the workspace root.
    path            TEXT NOT NULL,
    kind            TEXT NOT NULL,                    -- edit | write
    -- Null when the file did not exist, or when it was too large to snapshot.
    before_content  TEXT,
    after_content   TEXT,
    undone          INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workspace_changes
    ON workspace_changes (workspace_id, created_at DESC);

CREATE TABLE IF NOT EXISTS conversations (
    id           TEXT PRIMARY KEY,
    title        TEXT NOT NULL DEFAULT 'New chat',
    title_mode   TEXT NOT NULL DEFAULT 'first_line',  -- first_line | llm_generated | manual
    model_id     TEXT,
    pinned       INTEGER NOT NULL DEFAULT 0,
    archived     INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    -- Set when the conversation lives in a project. Deleting a project releases
    -- its conversations rather than destroying them: the chats are the work, the
    -- project is only a grouping.
    project_id   TEXT REFERENCES projects (id) ON DELETE SET NULL,
    -- Set when this conversation was branched off another one. Deleting the
    -- original leaves the branch alone: a fork is a chat in its own right from
    -- the moment it is made.
    forked_from_id TEXT REFERENCES conversations (id) ON DELETE SET NULL,
    -- Set when the conversation belongs to a coding workspace, which is what
    -- gives it access to files at all. Deleting the workspace releases the
    -- chat rather than destroying it.
    workspace_id   TEXT REFERENCES workspaces (id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_conversations_updated
    ON conversations (archived, updated_at DESC);

-- Note: the index on `project_id` is NOT here. It cannot be, because on an
-- upgrade the column does not exist yet when this file runs — see LATE_INDEXES
-- in `db/mod.rs`.

CREATE TABLE IF NOT EXISTS messages (
    id                      TEXT PRIMARY KEY,
    conversation_id         TEXT NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    role                    TEXT NOT NULL,            -- user | assistant | system | tool
    content                 TEXT NOT NULL DEFAULT '',
    reasoning_content       TEXT,
    tool_calls              TEXT,                     -- JSON array
    tool_call_id            TEXT,
    attachments             TEXT,                     -- JSON array
    used_web_search         INTEGER NOT NULL DEFAULT 0,
    web_sources             TEXT,                     -- JSON array of {title,url}
    model_id                TEXT,
    -- Usage and timing come straight from llama-server's response, which is
    -- what powers the per-message request inspector.
    usage_prompt_tokens     INTEGER,
    usage_completion_tokens INTEGER,
    -- Prompt tokens across every tool round, where the column above holds only
    -- the last round's — which is what the inspector wants and understates
    -- what an allowance actually paid for.
    usage_prompt_tokens_total INTEGER,
    -- Which provider's allowance this turn spent. NULL for a local model.
    provider_slug           TEXT,
    timing_ttft_ms          INTEGER,
    timing_total_ms         INTEGER,
    timing_tokens_per_sec   REAL,
    finish_reason           TEXT,
    created_at              TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_conversation
    ON messages (conversation_id, created_at);

CREATE TABLE IF NOT EXISTS mcp_servers (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    transport   TEXT NOT NULL,                        -- stdio | http
    command     TEXT,
    args        TEXT,                                 -- JSON array
    env         TEXT,                                 -- JSON object
    url         TEXT,
    headers     TEXT,                                 -- JSON object
    enabled     INTEGER NOT NULL DEFAULT 1,
    status      TEXT NOT NULL DEFAULT 'disconnected', -- connected | disconnected | error
    last_error  TEXT,
    created_at  TEXT NOT NULL,
    -- Set when the server came from the recommended list, so the store can show
    -- what is already installed without matching on names.
    slug        TEXT,
    -- Tools reported by the last successful handshake. Cached so the list page
    -- does not have to connect to every server to render a count.
    tool_count  INTEGER,
    -- Reference into the credential store for a bearer token. The token itself
    -- is never written to this database.
    auth_ref    TEXT
);

CREATE TABLE IF NOT EXISTS conversation_mcp_servers (
    conversation_id TEXT NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    mcp_server_id   TEXT NOT NULL REFERENCES mcp_servers (id) ON DELETE CASCADE,
    PRIMARY KEY (conversation_id, mcp_server_id)
);

-- Remote model providers the user holds the key for: OpenRouter, OpenAI,
-- Anthropic, a rented GPU box, anything speaking the OpenAI API.
--
-- Credentials live in the separate credential store (see `secrets.rs`); this
-- table holds only the lookup reference, so a copy of the database never carries
-- a key with it.
CREATE TABLE IF NOT EXISTS cloud_connectors (
    id             TEXT PRIMARY KEY,
    provider       TEXT NOT NULL,                     -- openrouter | openai | anthropic | groq | ... | custom
    label          TEXT NOT NULL,
    keychain_ref   TEXT NOT NULL,
    base_url       TEXT,
    status         TEXT NOT NULL DEFAULT 'untested',  -- untested | ok | error
    last_tested_at TEXT,
    last_error     TEXT,
    created_at     TEXT NOT NULL,
    enabled        INTEGER NOT NULL DEFAULT 1,
    -- Model ids reported by the endpoint's own `/models`, cached from the last
    -- successful test so the picker does not have to call out on every render.
    models         TEXT NOT NULL DEFAULT '[]'         -- JSON array
);

CREATE TABLE IF NOT EXISTS settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,                         -- JSON-encoded
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS downloads (
    id               TEXT PRIMARY KEY,
    kind             TEXT NOT NULL,                   -- model | engine_binary
    target_id        TEXT NOT NULL,
    label            TEXT NOT NULL DEFAULT '',
    url              TEXT NOT NULL,
    dest_path        TEXT NOT NULL,
    total_bytes      INTEGER,
    downloaded_bytes INTEGER NOT NULL DEFAULT 0,
    sha256_expected  TEXT,
    status           TEXT NOT NULL DEFAULT 'queued',  -- queued | downloading | paused | verifying | completed | failed | cancelled
    error            TEXT,
    started_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_downloads_status
    ON downloads (status, updated_at DESC);

CREATE TABLE IF NOT EXISTS engine_runtimes (
    id            TEXT PRIMARY KEY,                   -- release tag
    version       TEXT NOT NULL,
    asset_name    TEXT NOT NULL,
    path          TEXT NOT NULL,                      -- absolute path to llama-server
    sha256        TEXT NOT NULL,
    backend       TEXT NOT NULL,                      -- metal | cpu
    downloaded_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS prompt_templates (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    body       TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- What the model has been asked to remember, written by the `remember` tool and
-- read back by `recall`. Kept in the database rather than in the prompt so it
-- survives a conversation ending, and scoped to nothing so a fact learned in one
-- chat is available in the next.
CREATE TABLE IF NOT EXISTS memories (
    id         TEXT PRIMARY KEY,
    content    TEXT NOT NULL,
    tags       TEXT NOT NULL DEFAULT '[]',            -- JSON array
    source     TEXT,                                  -- conversation id, when known
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_memories_created
    ON memories (created_at DESC);

-- Skills the user added themselves: uploaded as a SKILL.md, or pulled out of a
-- GitHub repository. Kept apart from the built-in catalogue rather than merged
-- into it, because the two have different rules — a built-in is part of the
-- build and cannot be deleted, and one of these can be removed the moment it
-- turns out to be wrong.
CREATE TABLE IF NOT EXISTS user_skills (
    slug          TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    blurb         TEXT NOT NULL,
    category      TEXT NOT NULL,
    instructions  TEXT NOT NULL,
    approx_tokens INTEGER NOT NULL,
    -- Where it came from, shown on the card: `upload`, or the repository URL.
    source        TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);
