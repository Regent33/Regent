//! Declarative schema + version-gated migrations (the split: column
//! adds are declarative via `IF NOT EXISTS`/reconcile, data/FTS changes go
//! through the numbered chain).

pub const SCHEMA_VERSION: i64 = 8;

/// Columns added after a table first shipped. Applied by reconcile on every
/// open (idempotent), so plain column adds never need a numbered migration.
pub const RECONCILE_COLUMNS: &[(&str, &str, &str)] = &[
    // (table, column, declaration) — v2: frozen system prompt per session.
    ("sessions", "system_prompt", "TEXT"),
];

pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    model TEXT,
    system_prompt TEXT,
    parent_session_id TEXT REFERENCES sessions(id),
    started_at REAL NOT NULL,
    ended_at REAL,
    end_reason TEXT,
    title TEXT,
    message_count INTEGER NOT NULL DEFAULT 0,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    api_call_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_sessions_started ON sessions(started_at DESC);
CREATE INDEX IF NOT EXISTS idx_sessions_parent ON sessions(parent_session_id);

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    role TEXT NOT NULL,
    content TEXT,
    tool_call_id TEXT,
    tool_calls TEXT,
    tool_name TEXT,
    reasoning TEXT,
    timestamp REAL NOT NULL,
    token_count INTEGER,
    finish_reason TEXT
);
CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id, timestamp);

CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    content, tool_name, tool_calls
);

CREATE TRIGGER IF NOT EXISTS messages_fts_insert AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, content, tool_name, tool_calls)
    VALUES (new.id, COALESCE(new.content, ''), COALESCE(new.tool_name, ''),
            COALESCE(new.tool_calls, ''));
END;

CREATE TRIGGER IF NOT EXISTS messages_fts_delete AFTER DELETE ON messages BEGIN
    DELETE FROM messages_fts WHERE rowid = old.id;
END;

CREATE TRIGGER IF NOT EXISTS messages_fts_update AFTER UPDATE ON messages BEGIN
    DELETE FROM messages_fts WHERE rowid = old.id;
    INSERT INTO messages_fts(rowid, content, tool_name, tool_calls)
    VALUES (new.id, COALESCE(new.content, ''), COALESCE(new.tool_name, ''),
            COALESCE(new.tool_calls, ''));
END;

CREATE TABLE IF NOT EXISTS turns (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    model TEXT,
    api_calls INTEGER NOT NULL DEFAULT 0,
    outcome TEXT NOT NULL,
    error TEXT,
    started_at REAL NOT NULL,
    ended_at REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_turns_session ON turns(session_id, started_at);

-- Graph memory (v3): entities, facts, preferences, episodes — one graph,
-- provenance-tagged, FTS-indexed. See docs/proposal §5 and ADR-006.
CREATE TABLE IF NOT EXISTS nodes (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    content TEXT NOT NULL,
    provenance TEXT NOT NULL,
    trust REAL NOT NULL DEFAULT 0.5,
    session_id TEXT,
    created_at REAL NOT NULL,
    updated_at REAL NOT NULL,
    ttl_expires_at REAL,
    access_count INTEGER NOT NULL DEFAULT 0,
    last_accessed_at REAL,
    content_hash TEXT NOT NULL UNIQUE
);
CREATE INDEX IF NOT EXISTS idx_nodes_kind ON nodes(kind, created_at);
CREATE INDEX IF NOT EXISTS idx_nodes_kind_name ON nodes(kind, name);

CREATE TABLE IF NOT EXISTS edges (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    src TEXT NOT NULL REFERENCES nodes(id),
    dst TEXT NOT NULL REFERENCES nodes(id),
    relation TEXT NOT NULL,
    weight REAL NOT NULL DEFAULT 1.0,
    provenance TEXT NOT NULL DEFAULT 'agent_inferred',
    created_at REAL NOT NULL,
    UNIQUE(src, dst, relation)
);
CREATE INDEX IF NOT EXISTS idx_edges_src ON edges(src);
CREATE INDEX IF NOT EXISTS idx_edges_dst ON edges(dst);

-- Vector lane (v4): one embedding per node, keyed by the model that produced
-- it. Brute-force cosine in Rust (regent-store::vector_search) — superior to a
-- C ANN index at personal-agent scale; swappable to vec0 later. ON DELETE
-- CASCADE keeps embeddings in lockstep with node lifecycle. See ADR-013.
CREATE TABLE IF NOT EXISTS node_embeddings (
    node_id TEXT PRIMARY KEY REFERENCES nodes(id) ON DELETE CASCADE,
    model_id TEXT NOT NULL,
    dim INTEGER NOT NULL,
    vector BLOB NOT NULL,
    created_at REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_node_embeddings_model ON node_embeddings(model_id);

CREATE VIRTUAL TABLE IF NOT EXISTS nodes_fts USING fts5(name, content);

CREATE TRIGGER IF NOT EXISTS nodes_fts_insert AFTER INSERT ON nodes BEGIN
    INSERT INTO nodes_fts(rowid, name, content) VALUES (new.rowid, new.name, new.content);
END;

CREATE TRIGGER IF NOT EXISTS nodes_fts_delete AFTER DELETE ON nodes BEGIN
    DELETE FROM nodes_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER IF NOT EXISTS nodes_fts_update AFTER UPDATE ON nodes BEGIN
    DELETE FROM nodes_fts WHERE rowid = old.rowid;
    INSERT INTO nodes_fts(rowid, name, content) VALUES (new.rowid, new.name, new.content);
END;

-- Write-approval staging (v5): long-term memory writes proposed by the agent
-- wait here until a human approves them (security §10.2/§10.5). Each row holds
-- everything add_node needs to commit on approval. See docs/p4-memory-retrieval-design §4.
CREATE TABLE IF NOT EXISTS pending_memory_writes (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    content TEXT NOT NULL,
    provenance TEXT NOT NULL,
    trust REAL NOT NULL,
    session_id TEXT,
    ttl_secs REAL,
    created_at REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_pending_writes_created ON pending_memory_writes(created_at);

-- Kanban board (v6): the shared work board for multi-agent orchestration
-- (P6). A dispatcher hands `todo` tasks to worker profiles, which claim them
-- atomically. Board-scoped so tenants/projects stay isolated. See next-steps §P6.
CREATE TABLE IF NOT EXISTS kanban_tasks (
    id TEXT PRIMARY KEY,
    board TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'todo',
    assignee TEXT,
    created_at REAL NOT NULL,
    updated_at REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_kanban_board_status ON kanban_tasks(board, status, created_at);

-- Board config (v7): each board declares how finished work reaches `done` —
-- 'human' (a person approves), 'agent' (a reviewer agent judges), or 'auto'
-- (self-approve). Boards with no row default to 'human', so existing tasks are
-- unaffected. `reviewer_agent` names the profile used when policy = 'agent'.
-- See next-steps §P6 and the review-before-done flow.
CREATE TABLE IF NOT EXISTS boards (
    board TEXT PRIMARY KEY,
    review_policy TEXT NOT NULL DEFAULT 'human',
    reviewer_agent TEXT,
    created_at REAL NOT NULL
);

-- Conversation→session map (v8): binds a platform conversation key
-- (e.g. `slack:C123`, `discord:456`) to a Regent session, so a chat surface
-- keeps one continuous session across messages instead of starting fresh each
-- time. See P5 webhook ingress.
CREATE TABLE IF NOT EXISTS conversation_sessions (
    conversation_key TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    created_at REAL NOT NULL
);

-- Persona (v9): the agent's soul + the user's profile, stored in the DB rather
-- than plaintext files under $REGENT_HOME (security). `key` is 'soul' or
-- 'about'; both rows are seeded empty on open so they always exist + are
-- editable via `regent soul` / `regent about` and a future agent tool.
CREATE TABLE IF NOT EXISTS persona (
    key TEXT PRIMARY KEY,
    content TEXT NOT NULL DEFAULT '',
    updated_at REAL NOT NULL
);

CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
"#;
