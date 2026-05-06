//! Embedded migrations.
//!
//! For v0.1 we keep migrations as a single SQL batch and rely on
//! `IF NOT EXISTS`. Future versions can switch to a numbered table.

use rusqlite::Connection;

use crate::error::DbResult;

const SCHEMA_V1: &str = r#"
-- ─── core mutable tables ───────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    domain TEXT NOT NULL,
    topic TEXT NOT NULL,
    summary TEXT NOT NULL DEFAULT '',
    long_summary TEXT NOT NULL DEFAULT '',
    active_entities_json TEXT NOT NULL DEFAULT '[]',
    resolved_json TEXT NOT NULL DEFAULT '[]',
    unresolved_json TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_active_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_active
    ON sessions (status, last_active_at DESC);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    trace_id TEXT,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    token_count INTEGER NOT NULL DEFAULT 0,
    summary_id TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_session_time
    ON messages (session_id, created_at DESC);

-- ─── memory ────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    scope TEXT NOT NULL,

    content TEXT NOT NULL,
    entities_json TEXT NOT NULL DEFAULT '[]',

    confidence REAL NOT NULL,
    trust_score REAL NOT NULL,
    half_life_days INTEGER NOT NULL,
    retrieve_count INTEGER NOT NULL DEFAULT 0,
    last_retrieved_at TEXT,

    source_trace_id TEXT,
    source_type TEXT NOT NULL,

    conflict_ids_json TEXT NOT NULL DEFAULT '[]',

    status TEXT NOT NULL DEFAULT 'approved',

    emotion_energy REAL NOT NULL DEFAULT 0,
    emotion_polarity TEXT NOT NULL DEFAULT 'neutral',

    tier INTEGER NOT NULL DEFAULT 4,

    expires_at TEXT,
    cluster_member_ids_json TEXT NOT NULL DEFAULT '[]',

    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_memories_scope
    ON memories (scope, type, trust_score DESC);
CREATE INDEX IF NOT EXISTS idx_memories_status
    ON memories (status, type);
CREATE INDEX IF NOT EXISTS idx_memories_tier
    ON memories (tier, trust_score DESC);
CREATE INDEX IF NOT EXISTS idx_memories_emotion
    ON memories (emotion_energy DESC, emotion_polarity);

-- ─── immutable: raw_event_log ──────────────────────────────────────────
-- Section 15.7.2. Append-only. Triggers prevent UPDATE / DELETE.
-- checksum = sha256(ts || event_type || raw_content) computed in Rust.

CREATE TABLE IF NOT EXISTS raw_event_log (
    seq          INTEGER PRIMARY KEY AUTOINCREMENT,
    ts           TEXT NOT NULL,
    event_type   TEXT NOT NULL,
    session_id   TEXT,
    trace_id     TEXT,
    agent_id     TEXT,
    raw_content  TEXT NOT NULL,
    safe_content TEXT,
    immutable    INTEGER NOT NULL DEFAULT 1 CHECK (immutable = 1),
    checksum     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_raw_event_session
    ON raw_event_log (session_id, seq);
CREATE INDEX IF NOT EXISTS idx_raw_event_trace
    ON raw_event_log (trace_id, seq);

CREATE TRIGGER IF NOT EXISTS trg_raw_event_no_update
    BEFORE UPDATE ON raw_event_log
    BEGIN SELECT RAISE(ABORT, 'raw_event_log is immutable'); END;

CREATE TRIGGER IF NOT EXISTS trg_raw_event_no_delete
    BEFORE DELETE ON raw_event_log
    BEGIN SELECT RAISE(ABORT, 'raw_event_log is immutable'); END;

-- ─── immutable: memory_change_log ──────────────────────────────────────
-- Section 15.7.3.

CREATE TABLE IF NOT EXISTS memory_change_log (
    seq            INTEGER PRIMARY KEY AUTOINCREMENT,
    ts             TEXT NOT NULL,
    memory_id      TEXT NOT NULL,
    change_type    TEXT NOT NULL,
    before_json    TEXT,
    after_json     TEXT,
    source_module  TEXT,
    source_trace_id TEXT,
    source_agent_id TEXT,
    reason         TEXT
);

CREATE INDEX IF NOT EXISTS idx_memory_log_id
    ON memory_change_log (memory_id, seq);
CREATE INDEX IF NOT EXISTS idx_memory_log_ts
    ON memory_change_log (ts);

CREATE TRIGGER IF NOT EXISTS trg_memory_log_no_update
    BEFORE UPDATE ON memory_change_log
    BEGIN SELECT RAISE(ABORT, 'memory_change_log is immutable'); END;

CREATE TRIGGER IF NOT EXISTS trg_memory_log_no_delete
    BEFORE DELETE ON memory_change_log
    BEGIN SELECT RAISE(ABORT, 'memory_change_log is immutable'); END;

-- ─── immutable: session_snapshots ──────────────────────────────────────
-- Section 15.7.4. Stored as JSON blobs to keep the schema flat at v0.1.

CREATE TABLE IF NOT EXISTS session_snapshots (
    id                   TEXT PRIMARY KEY,
    session_id           TEXT NOT NULL,
    seq                  INTEGER NOT NULL,
    snapshot_reason      TEXT,
    rolling_summary      TEXT,
    active_entities_json TEXT,
    unresolved_json      TEXT,
    resolved_json        TEXT,
    task_tree_json       TEXT,
    artifact_index_json  TEXT,
    checksum             TEXT NOT NULL,
    created_at           TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_session_snapshot
    ON session_snapshots (session_id, seq DESC);

CREATE TRIGGER IF NOT EXISTS trg_session_snapshot_no_update
    BEFORE UPDATE ON session_snapshots
    BEGIN SELECT RAISE(ABORT, 'session_snapshots is immutable'); END;

CREATE TRIGGER IF NOT EXISTS trg_session_snapshot_no_delete
    BEFORE DELETE ON session_snapshots
    BEGIN SELECT RAISE(ABORT, 'session_snapshots is immutable'); END;

-- ─── routing example collection (Growth seed input) ────────────────────

CREATE TABLE IF NOT EXISTS routing_examples (
    id          TEXT PRIMARY KEY,
    user_input  TEXT NOT NULL,
    decision_json TEXT NOT NULL,
    corrected   INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL
);

-- ─── @mention log (Section 9.11.6) ─────────────────────────────────────

CREATE TABLE IF NOT EXISTS mention_log (
    id            TEXT PRIMARY KEY,
    session_id    TEXT NOT NULL,
    trace_id      TEXT,
    raw_text      TEXT NOT NULL,
    resolved_type TEXT,
    unresolved    INTEGER NOT NULL DEFAULT 0,
    mention_mode  TEXT NOT NULL,
    created_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mention_log_session
    ON mention_log (session_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_mention_unresolved
    ON mention_log (unresolved, raw_text);
"#;

pub fn run(conn: &Connection) -> DbResult<()> {
    conn.execute_batch(SCHEMA_V1)?;
    Ok(())
}
