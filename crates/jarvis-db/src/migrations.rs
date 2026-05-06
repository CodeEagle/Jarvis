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

-- ─── growth events / artifacts (Section 21.7 / 21.8) ───────────────────

CREATE TABLE IF NOT EXISTS growth_events (
    id            TEXT PRIMARY KEY,
    ts            TEXT NOT NULL,
    trace_id      TEXT,
    source_module TEXT NOT NULL,
    event_type    TEXT NOT NULL,
    payload_json  TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_growth_events_type
    ON growth_events (event_type, ts DESC);

CREATE TABLE IF NOT EXISTS growth_artifacts (
    id                  TEXT PRIMARY KEY,
    type                TEXT NOT NULL,
    status              TEXT NOT NULL DEFAULT 'candidate',
    version             INTEGER NOT NULL DEFAULT 1,
    scope               TEXT,
    confidence          REAL NOT NULL DEFAULT 0.0,
    evidence_trace_ids  TEXT NOT NULL DEFAULT '[]',
    payload_json        TEXT NOT NULL DEFAULT '{}',
    created_at          TEXT NOT NULL,
    promoted_at         TEXT
);

CREATE INDEX IF NOT EXISTS idx_growth_artifacts_status
    ON growth_artifacts (status, type);

-- ─── compression: turn / rolling summaries (Section 21.10 / 21.11) ─────

CREATE TABLE IF NOT EXISTS turn_summaries (
    id                  TEXT PRIMARY KEY,
    session_id          TEXT NOT NULL,
    trace_id            TEXT,
    task_id             TEXT,
    user_goal           TEXT NOT NULL DEFAULT '',
    actions_taken_json  TEXT NOT NULL DEFAULT '[]',
    result              TEXT NOT NULL DEFAULT '',
    entities_json       TEXT NOT NULL DEFAULT '[]',
    unresolved_json     TEXT NOT NULL DEFAULT '[]',
    source_message_ids_json TEXT NOT NULL DEFAULT '[]',
    token_count         INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_turn_summaries_session
    ON turn_summaries (session_id, created_at DESC);

CREATE TABLE IF NOT EXISTS rolling_summary_versions (
    id                       TEXT PRIMARY KEY,
    session_id               TEXT NOT NULL,
    version                  INTEGER NOT NULL,
    content                  TEXT NOT NULL,
    token_count              INTEGER NOT NULL DEFAULT 0,
    trigger_reason           TEXT NOT NULL,
    source_turn_summary_ids_json TEXT NOT NULL DEFAULT '[]',
    created_at               TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_rolling_versions_session
    ON rolling_summary_versions (session_id, version);

-- ─── orchestrator: task_trees / task_nodes / artifacts (Section 21.13-15)

CREATE TABLE IF NOT EXISTS task_trees (
    id           TEXT PRIMARY KEY,
    root_task_id TEXT NOT NULL,
    session_id   TEXT NOT NULL,
    trace_id     TEXT,
    status       TEXT NOT NULL DEFAULT 'running',
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_task_trees_session
    ON task_trees (session_id, created_at DESC);

CREATE TABLE IF NOT EXISTS task_nodes (
    id                       TEXT PRIMARY KEY,
    tree_id                  TEXT NOT NULL,
    parent_id                TEXT,
    task_id                  TEXT NOT NULL,
    title                    TEXT NOT NULL,
    intent                   TEXT NOT NULL,
    status                   TEXT NOT NULL DEFAULT 'pending',
    assigned_agent_type      TEXT NOT NULL,
    assigned_agent_id        TEXT,
    depends_on_json          TEXT NOT NULL DEFAULT '[]',
    result_summary           TEXT,
    result_artifact_ids_json TEXT NOT NULL DEFAULT '[]',
    error_summary            TEXT,
    created_at               TEXT NOT NULL,
    completed_at             TEXT
);

CREATE INDEX IF NOT EXISTS idx_task_nodes_tree
    ON task_nodes (tree_id);
CREATE INDEX IF NOT EXISTS idx_task_nodes_status
    ON task_nodes (tree_id, status);

CREATE TABLE IF NOT EXISTS artifacts (
    id             TEXT PRIMARY KEY,
    task_node_id   TEXT,
    session_id     TEXT NOT NULL,
    trace_id       TEXT,
    type           TEXT NOT NULL,
    title          TEXT NOT NULL,
    summary        TEXT NOT NULL DEFAULT '',
    content_ref    TEXT NOT NULL,
    size_bytes     INTEGER,
    token_estimate INTEGER,
    created_at     TEXT NOT NULL,
    expires_at     TEXT
);

CREATE INDEX IF NOT EXISTS idx_artifacts_session
    ON artifacts (session_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_artifacts_task_node
    ON artifacts (task_node_id);

-- ─── conversation bus (Section 8.11.10) ────────────────────────────────

CREATE TABLE IF NOT EXISTS conversation_ownerships (
    id               TEXT PRIMARY KEY,
    session_id       TEXT NOT NULL,
    agent_id         TEXT NOT NULL,
    agent_type       TEXT NOT NULL,
    sub_task_id      TEXT,
    interaction_mode TEXT NOT NULL,
    acquired_at      TEXT NOT NULL,
    released_at      TEXT
);

CREATE INDEX IF NOT EXISTS idx_ownership_session_active
    ON conversation_ownerships (session_id, released_at);

CREATE TABLE IF NOT EXISTS sub_channels (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL,
    sub_task_id     TEXT NOT NULL,
    agent_id        TEXT,
    agent_type      TEXT,
    status          TEXT NOT NULL,
    can_interrupt   INTEGER NOT NULL DEFAULT 1,
    interrupt_cost  TEXT NOT NULL DEFAULT 'low',
    tentacle_path   TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sub_channels_session
    ON sub_channels (session_id, status);

CREATE TABLE IF NOT EXISTS sub_task_checkpoints (
    id                       TEXT PRIMARY KEY,
    sub_task_id              TEXT NOT NULL,
    agent_id                 TEXT,
    completed_steps_json     TEXT NOT NULL DEFAULT '[]',
    working_set_snapshot     TEXT NOT NULL DEFAULT '',
    pinned_findings_json     TEXT NOT NULL DEFAULT '[]',
    artifact_ids_so_far_json TEXT NOT NULL DEFAULT '[]',
    resume_hint              TEXT NOT NULL DEFAULT '',
    created_at               TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_checkpoints_sub_task
    ON sub_task_checkpoints (sub_task_id, created_at DESC);

CREATE TABLE IF NOT EXISTS pending_user_messages (
    id                 TEXT PRIMARY KEY,
    session_id         TEXT NOT NULL,
    content            TEXT NOT NULL,
    routing_decision   TEXT,
    resolved           INTEGER NOT NULL DEFAULT 0,
    created_at         TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_pending_messages_session
    ON pending_user_messages (session_id, resolved, created_at);

-- ─── steer signals (Section 9.11.6) ────────────────────────────────────

CREATE TABLE IF NOT EXISTS steer_signals (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL,
    sub_task_id     TEXT NOT NULL,
    trace_id        TEXT,
    content         TEXT NOT NULL,
    scope           TEXT NOT NULL,
    inject_at       TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending',
    created_at      TEXT NOT NULL,
    injected_at     TEXT,
    acknowledged_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_steer_sub_task
    ON steer_signals (sub_task_id, status);

-- ─── walkthrough docs / verifier checks (Section 8.16.5) ───────────────

CREATE TABLE IF NOT EXISTS walkthrough_docs (
    id                  TEXT PRIMARY KEY,
    sub_task_id         TEXT NOT NULL,
    session_id          TEXT NOT NULL,
    trace_id            TEXT,
    title               TEXT NOT NULL,
    generated_at        TEXT NOT NULL,
    agent_type          TEXT NOT NULL,
    sections_json       TEXT NOT NULL DEFAULT '[]',
    verification_status TEXT NOT NULL DEFAULT 'pending',
    verification_notes  TEXT,
    verified_at         TEXT,
    approval_status     TEXT NOT NULL DEFAULT 'pending',
    approved_by         TEXT,
    approved_at         TEXT
);

CREATE INDEX IF NOT EXISTS idx_walkthrough_session
    ON walkthrough_docs (session_id, generated_at DESC);
CREATE INDEX IF NOT EXISTS idx_walkthrough_approval
    ON walkthrough_docs (approval_status, verification_status);

CREATE TABLE IF NOT EXISTS verifier_checks (
    id                  TEXT PRIMARY KEY,
    walkthrough_doc_id  TEXT NOT NULL,
    section_id          TEXT NOT NULL,
    check_type          TEXT NOT NULL,
    expected            TEXT NOT NULL,
    actual              TEXT,
    match_result        INTEGER,
    discrepancy         TEXT,
    executed_at         TEXT
);

CREATE INDEX IF NOT EXISTS idx_verifier_doc
    ON verifier_checks (walkthrough_doc_id);

-- ─── activity cards (collaboration panel data layer) ───────────────────

CREATE TABLE IF NOT EXISTS activity_cards (
    id                  TEXT PRIMARY KEY,
    session_id          TEXT NOT NULL,
    sub_task_id         TEXT,
    trace_id            TEXT,
    agent_type          TEXT NOT NULL,
    agent_display_name  TEXT NOT NULL,
    agent_avatar_emoji  TEXT NOT NULL,
    status              TEXT NOT NULL,
    title               TEXT NOT NULL,
    current_action      TEXT,
    progress_text       TEXT,
    result_summary      TEXT,
    error_message       TEXT,
    interaction_json    TEXT,
    artifacts_json      TEXT,
    is_expanded         INTEGER NOT NULL DEFAULT 1,
    can_interrupt       INTEGER NOT NULL DEFAULT 1,
    interrupt_cost      TEXT NOT NULL DEFAULT 'low',
    started_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    completed_at        TEXT
);

CREATE INDEX IF NOT EXISTS idx_activity_cards_session
    ON activity_cards (session_id, started_at DESC);

-- ─── session snapshots / cold-start snapshots (Section 9.5.5 / 15.7.4) ─

CREATE TABLE IF NOT EXISTS cold_start_snapshots (
    id                   TEXT PRIMARY KEY,
    session_id           TEXT NOT NULL,
    captured_at          TEXT NOT NULL,
    rolling_summary      TEXT NOT NULL DEFAULT '',
    recent_messages_json TEXT NOT NULL DEFAULT '[]',
    memory_hits_json     TEXT NOT NULL DEFAULT '[]',
    cross_session_json   TEXT NOT NULL DEFAULT '[]',
    valid_for_turns      INTEGER NOT NULL DEFAULT 5,
    used_turns           INTEGER NOT NULL DEFAULT 0,
    created_at           TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cold_start_session
    ON cold_start_snapshots (session_id, created_at DESC);

-- ─── FTS5 index over memories.content (Section 13.4 v0.1 → 21.3) ───────
-- We model the FTS index as a side table whose rowid mirrors the
-- `memories.rowid`, so callers can correlate hits without depending
-- on FTS internals. Population/clean-up happens in trigger pairs so
-- writes anywhere in `memories` keep the index in sync.

CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    content,
    entities,
    content_id UNINDEXED
);

CREATE TRIGGER IF NOT EXISTS trg_memories_fts_ai
    AFTER INSERT ON memories
    BEGIN
        INSERT INTO memories_fts (rowid, content, entities, content_id)
        VALUES (new.rowid, new.content, new.entities_json, new.id);
    END;

CREATE TRIGGER IF NOT EXISTS trg_memories_fts_ad
    AFTER DELETE ON memories
    BEGIN
        DELETE FROM memories_fts WHERE rowid = old.rowid;
    END;

CREATE TRIGGER IF NOT EXISTS trg_memories_fts_au
    AFTER UPDATE OF content, entities_json ON memories
    BEGIN
        DELETE FROM memories_fts WHERE rowid = old.rowid;
        INSERT INTO memories_fts (rowid, content, entities, content_id)
        VALUES (new.rowid, new.content, new.entities_json, new.id);
    END;

-- ─── regression reports (Section 8.16.5 / 21) ──────────────────────────

CREATE TABLE IF NOT EXISTS regression_reports (
    id              TEXT PRIMARY KEY,
    session_id      TEXT,
    triggered_at    TEXT NOT NULL,
    total_checks    INTEGER NOT NULL DEFAULT 0,
    passed          INTEGER NOT NULL DEFAULT 0,
    expected_changes INTEGER NOT NULL DEFAULT 0,
    potential_bugs  INTEGER NOT NULL DEFAULT 0,
    items_json      TEXT NOT NULL DEFAULT '[]'
);

CREATE INDEX IF NOT EXISTS idx_regression_reports_time
    ON regression_reports (triggered_at DESC);

-- ─── command runner (Section 8.17.6) ───────────────────────────────────

CREATE TABLE IF NOT EXISTS command_executions (
    id            TEXT PRIMARY KEY,
    session_id    TEXT NOT NULL,
    command_id    TEXT NOT NULL,
    command_label TEXT NOT NULL,
    status        TEXT NOT NULL,
    triggered_by  TEXT NOT NULL DEFAULT 'user_button',
    steps_json    TEXT NOT NULL DEFAULT '[]',
    started_at    TEXT NOT NULL,
    completed_at  TEXT
);

CREATE INDEX IF NOT EXISTS idx_command_exec_session
    ON command_executions (session_id, started_at DESC);

-- ─── skills (Section 21.9) ─────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS skills (
    id                TEXT PRIMARY KEY,
    name              TEXT NOT NULL UNIQUE,
    description       TEXT NOT NULL DEFAULT '',
    trigger_intents_json TEXT NOT NULL DEFAULT '[]',
    trigger_entities_json TEXT NOT NULL DEFAULT '[]',
    required_tools_json TEXT NOT NULL DEFAULT '[]',
    risk_level        TEXT NOT NULL DEFAULT 'low',
    steps_json        TEXT NOT NULL DEFAULT '[]',
    verification_json TEXT NOT NULL DEFAULT '[]',
    success_count     INTEGER NOT NULL DEFAULT 0,
    failure_count     INTEGER NOT NULL DEFAULT 0,
    status            TEXT NOT NULL DEFAULT 'candidate',
    evidence_trace_ids_json TEXT NOT NULL DEFAULT '[]',
    version           INTEGER NOT NULL DEFAULT 1,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_skills_status
    ON skills (status, name);
"#;

pub fn run(conn: &Connection) -> DbResult<()> {
    conn.execute_batch(SCHEMA_V1)?;
    Ok(())
}
