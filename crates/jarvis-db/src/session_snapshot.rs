//! `session_snapshots`: immutable point-in-time captures of a session
//! (Section 15.7.4). Used for time-point replay and pre-merge baselines.

use chrono::{DateTime, Utc};
use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::time::now_iso;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::conn::Db;
use crate::error::DbResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub id: String,
    pub session_id: String,
    pub seq: i64,
    pub snapshot_reason: String,
    pub rolling_summary: Option<String>,
    pub active_entities_json: Option<String>,
    pub unresolved_json: Option<String>,
    pub resolved_json: Option<String>,
    pub task_tree_json: Option<String>,
    pub artifact_index_json: Option<String>,
    pub checksum: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SnapshotDraft<'a> {
    pub session_id: &'a str,
    pub reason: &'a str,
    pub rolling_summary: Option<&'a str>,
    pub active_entities_json: Option<&'a str>,
    pub unresolved_json: Option<&'a str>,
    pub resolved_json: Option<&'a str>,
    pub task_tree_json: Option<&'a str>,
    pub artifact_index_json: Option<&'a str>,
}

pub fn append(db: &Db, draft: SnapshotDraft<'_>) -> DbResult<SessionSnapshot> {
    let ts = now_iso();
    let id = new_id_with_prefix("snap");
    let mut hasher = Sha256::new();
    hasher.update(ts.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(draft.session_id.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(draft.reason.as_bytes());
    if let Some(s) = draft.rolling_summary {
        hasher.update(s.as_bytes());
    }
    let checksum = hex::encode(hasher.finalize());

    let seq: i64 = db.with(|conn| {
        let n: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1
                   FROM session_snapshots
                  WHERE session_id = ?1",
                params![draft.session_id],
                |row| row.get(0),
            )
            .unwrap_or(1);
        conn.execute(
            "INSERT INTO session_snapshots
                (id, session_id, seq, snapshot_reason,
                 rolling_summary, active_entities_json, unresolved_json,
                 resolved_json, task_tree_json, artifact_index_json,
                 checksum, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                id,
                draft.session_id,
                n,
                draft.reason,
                draft.rolling_summary,
                draft.active_entities_json,
                draft.unresolved_json,
                draft.resolved_json,
                draft.task_tree_json,
                draft.artifact_index_json,
                checksum,
                ts,
            ],
        )?;
        Ok(n)
    })?;

    Ok(SessionSnapshot {
        id,
        session_id: draft.session_id.to_string(),
        seq,
        snapshot_reason: draft.reason.to_string(),
        rolling_summary: draft.rolling_summary.map(str::to_string),
        active_entities_json: draft.active_entities_json.map(str::to_string),
        unresolved_json: draft.unresolved_json.map(str::to_string),
        resolved_json: draft.resolved_json.map(str::to_string),
        task_tree_json: draft.task_tree_json.map(str::to_string),
        artifact_index_json: draft.artifact_index_json.map(str::to_string),
        checksum,
        created_at: now_chrono(),
    })
}

fn now_chrono() -> DateTime<Utc> {
    Utc::now()
}

pub fn latest(db: &Db, session_id: &str) -> DbResult<Option<SessionSnapshot>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, session_id, seq, snapshot_reason,
                    rolling_summary, active_entities_json, unresolved_json,
                    resolved_json, task_tree_json, artifact_index_json,
                    checksum, created_at
               FROM session_snapshots
              WHERE session_id = ?1
              ORDER BY seq DESC
              LIMIT 1",
        )?;
        let r = stmt
            .query_row(params![session_id], parse_row)
            .optional()?;
        Ok(r)
    })
}

pub fn count(db: &Db, session_id: &str) -> DbResult<i64> {
    db.with(|conn| {
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM session_snapshots WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        Ok(n)
    })
}

fn parse_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionSnapshot> {
    let ts_str: String = row.get(11)?;
    let ts = DateTime::parse_from_rfc3339(&ts_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    Ok(SessionSnapshot {
        id: row.get(0)?,
        session_id: row.get(1)?,
        seq: row.get(2)?,
        snapshot_reason: row.get(3)?,
        rolling_summary: row.get(4)?,
        active_entities_json: row.get(5)?,
        unresolved_json: row.get(6)?,
        resolved_json: row.get(7)?,
        task_tree_json: row.get(8)?,
        artifact_index_json: row.get(9)?,
        checksum: row.get(10)?,
        created_at: ts,
    })
}
