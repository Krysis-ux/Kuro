-- Kuro LLM schema. Every statement is `IF NOT EXISTS`, so the file is safe to
-- re-run on an older database. Columns added to a shipped table go through
-- `add_column_if_missing` in `db/mod.rs` instead. Bump `SCHEMA_VERSION` when
-- this file changes.
--
-- `models` holds only models the user has pulled; the curated list is static
-- data in `catalog::curated`, merged in at the API layer.

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

-- A project groups conversations under standing instructions, appended to the
-- model's brief for every conversation in it.
CREATE TABLE IF NOT EXISTS projects (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    description  TEXT NOT NULL DEFAULT '',
    instructions TEXT NOT NULL DEFAULT '',
    -- Defaults for chats started here.
    model_id     TEXT,
    tool_groups  TEXT,                                -- JSON array, null means inherit
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_projects_updated
    ON projects (updated_at DESC);

-- A coding workspace: a local folder plus a mode limiting what a model may do
-- in it. It is the only thing that grants file access; plain chat has no file
-- tools.
CREATE TABLE IF NOT EXISTS workspaces (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    root_path  TEXT NOT NULL,
    model_id   TEXT,
    -- ask | plan | agent. `plan` is the default and cannot change anything.
    mode       TEXT NOT NULL DEFAULT 'plan',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workspaces_updated
    ON workspaces (updated_at DESC);

-- Every file a model changed, with the prior content, so agent-mode changes can
-- be undone. Deleting a workspace clears its history.
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
    -- Deleting a project releases its conversations rather than deleting them.
    project_id   TEXT REFERENCES projects (id) ON DELETE SET NULL,
    -- Set on a fork; deleting the original leaves the fork alone.
    forked_from_id TEXT REFERENCES conversations (id) ON DELETE SET NULL,
    -- Coding workspace (gives file access); deleting it releases the chat.
    workspace_id   TEXT REFERENCES workspaces (id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_conversations_updated
    ON conversations (archived, updated_at DESC);

-- The `project_id` index is not here: on upgrade the column does not exist yet
-- when this file runs. See LATE_INDEXES in `db/mod.rs`.

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
    -- Usage and timing come from llama-server's response (request inspector).
    usage_prompt_tokens     INTEGER,
    usage_completion_tokens INTEGER,
    -- Prompt tokens summed across all tool rounds; the column above is the last
    -- round only, which understates what an allowance paid for.
    usage_prompt_tokens_total INTEGER,
    -- Provider whose allowance this turn spent; NULL for a local model.
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
    -- Set when the server came from the recommended list.
    slug        TEXT,
    -- Tool count from the last successful handshake, cached for the list page.
    tool_count  INTEGER,
    -- Credential-store reference for a bearer token; the token is never stored here.
    auth_ref    TEXT
);

CREATE TABLE IF NOT EXISTS conversation_mcp_servers (
    conversation_id TEXT NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    mcp_server_id   TEXT NOT NULL REFERENCES mcp_servers (id) ON DELETE CASCADE,
    PRIMARY KEY (conversation_id, mcp_server_id)
);

-- Remote model providers the user holds a key for (anything speaking the OpenAI
-- API). Keys live in the credential store (`secrets.rs`); this table holds only
-- the lookup reference.
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
    -- Model ids from the endpoint's `/models`, cached from the last successful test.
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

-- Facts written by the `remember` tool and read by `recall`. Global, not
-- per-conversation, so they carry across chats.
CREATE TABLE IF NOT EXISTS memories (
    id         TEXT PRIMARY KEY,
    content    TEXT NOT NULL,
    tags       TEXT NOT NULL DEFAULT '[]',            -- JSON array
    source     TEXT,                                  -- conversation id, when known
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_memories_created
    ON memories (created_at DESC);

-- Skills the user added (a SKILL.md upload or a GitHub repository). Separate
-- from the built-in catalogue because these can be deleted and built-ins cannot.
-- Subagents the user wrote. The built-in ones live in code (`agents::AGENTS`).
CREATE TABLE IF NOT EXISTS user_agents (
    slug        TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    blurb       TEXT NOT NULL,
    when_to_use TEXT NOT NULL,
    brief       TEXT NOT NULL,
    -- JSON array of skill slugs loaded on top of the project's own.
    skills      TEXT NOT NULL DEFAULT '[]',
    writes      INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS user_skills (
    slug          TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    blurb         TEXT NOT NULL,
    category      TEXT NOT NULL,
    instructions  TEXT NOT NULL,
    approx_tokens INTEGER NOT NULL,
    -- `upload`, or the repository URL.
    source        TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

-- Saved prompts that run on a schedule. Each run's reply lands in `conversation_id`,
-- made on the first run and made again if the user deletes that chat.
CREATE TABLE IF NOT EXISTS user_automations (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    prompt          TEXT NOT NULL,
    model           TEXT,                             -- NULL: the server default
    -- `every:N` (minutes), `daily:HH:MM` or `weekdays:HH:MM`, in this Mac's local time.
    schedule        TEXT NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 1,
    last_run_at     TEXT,
    next_run_at     TEXT,                             -- NULL while switched off
    last_error      TEXT,
    conversation_id TEXT REFERENCES conversations (id) ON DELETE SET NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
