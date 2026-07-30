-- Kuro LLM schema, version 1.
--
-- The full V1 shape is created up front, including tables that only later
-- phases write to (MCP servers, cloud connectors, prompt templates), so that
-- adding those features needs no migration.
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

CREATE TABLE IF NOT EXISTS conversations (
    id           TEXT PRIMARY KEY,
    title        TEXT NOT NULL DEFAULT 'New chat',
    title_mode   TEXT NOT NULL DEFAULT 'first_line',  -- first_line | llm_generated | manual
    model_id     TEXT,
    pinned       INTEGER NOT NULL DEFAULT 0,
    archived     INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_conversations_updated
    ON conversations (archived, updated_at DESC);

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
    transport   TEXT NOT NULL,                        -- stdio | http_sse
    command     TEXT,
    args        TEXT,                                 -- JSON array
    env         TEXT,                                 -- JSON object
    url         TEXT,
    headers     TEXT,                                 -- JSON object
    enabled     INTEGER NOT NULL DEFAULT 1,
    status      TEXT NOT NULL DEFAULT 'disconnected', -- connected | disconnected | error
    last_error  TEXT,
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS conversation_mcp_servers (
    conversation_id TEXT NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    mcp_server_id   TEXT NOT NULL REFERENCES mcp_servers (id) ON DELETE CASCADE,
    PRIMARY KEY (conversation_id, mcp_server_id)
);

-- Credentials themselves live in the macOS Keychain; this table only stores the
-- lookup key so that a database copy never leaks a secret.
CREATE TABLE IF NOT EXISTS cloud_connectors (
    id             TEXT PRIMARY KEY,
    provider       TEXT NOT NULL,                     -- runpod | vastai | lambdalabs | custom_openai
    label          TEXT NOT NULL,
    keychain_ref   TEXT NOT NULL,
    base_url       TEXT,
    status         TEXT NOT NULL DEFAULT 'untested',  -- untested | ok | error
    last_tested_at TEXT,
    last_error     TEXT,
    created_at     TEXT NOT NULL
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
