//! Cold-start snapshot (Section 9.5.5).
//!
//! When a session is idle for long enough or the user opens a new
//! window with `explicit_reference`, the orchestrator captures a
//! ColdStartSnapshot of the prior session state. The snapshot stays
//! pinned for `valid_for_turns` turns of the new session so subsequent
//! messages can't pollute the cold-start context.

use chrono::{DateTime, Utc};
use jarvis_core::ids::new_id_with_prefix;
use jarvis_core::time::now;
use jarvis_db::error::DbResult;
use jarvis_db::Db;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColdStartSnapshot {
    pub id: String,
    pub session_id: String,
    pub captured_at: DateTime<Utc>,
    pub rolling_summary: String,
    pub recent_messages: Vec<String>,
    pub memory_hits: Vec<String>,
    pub cross_session: Vec<String>,
    pub valid_for_turns: u32,
    pub used_turns: u32,
    pub created_at: DateTime<Utc>,
}

impl ColdStartSnapshot {
    pub fn is_active(&self) -> bool {
        self.used_turns < self.valid_for_turns
    }
}

#[derive(Debug, Clone)]
pub struct SnapshotInputs<'a> {
    pub session_id: &'a str,
    pub rolling_summary: &'a str,
    pub recent_messages: &'a [String],
    pub memory_hits: &'a [String],
    pub cross_session: &'a [String],
    pub valid_for_turns: u32,
}

#[derive(Clone)]
pub struct ColdStartStore {
    db: Db,
}

impl ColdStartStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn capture(&self, inputs: SnapshotInputs<'_>) -> DbResult<ColdStartSnapshot> {
        let id = new_id_with_prefix("cold");
        let n = now();
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO cold_start_snapshots
                    (id, session_id, captured_at, rolling_summary,
                     recent_messages_json, memory_hits_json,
                     cross_session_json, valid_for_turns, used_turns, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?3)",
                params![
                    id,
                    inputs.session_id,
                    n.to_rfc3339(),
                    inputs.rolling_summary,
                    serde_json::to_string(inputs.recent_messages)
                        .unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(inputs.memory_hits)
                        .unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(inputs.cross_session)
                        .unwrap_or_else(|_| "[]".into()),
                    inputs.valid_for_turns,
                ],
            )?;
            Ok(())
        })?;
        Ok(ColdStartSnapshot {
            id,
            session_id: inputs.session_id.to_string(),
            captured_at: n,
            rolling_summary: inputs.rolling_summary.to_string(),
            recent_messages: inputs.recent_messages.to_vec(),
            memory_hits: inputs.memory_hits.to_vec(),
            cross_session: inputs.cross_session.to_vec(),
            valid_for_turns: inputs.valid_for_turns,
            used_turns: 0,
            created_at: n,
        })
    }

    pub fn active_for(&self, session_id: &str) -> DbResult<Option<ColdStartSnapshot>> {
        let snap = self.db.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, captured_at, rolling_summary,
                        recent_messages_json, memory_hits_json,
                        cross_session_json, valid_for_turns,
                        used_turns, created_at
                   FROM cold_start_snapshots
                  WHERE session_id = ?1
                  ORDER BY created_at DESC
                  LIMIT 1",
            )?;
            let r = stmt
                .query_row(params![session_id], parse_snapshot)
                .optional()?;
            Ok(r)
        })?;
        Ok(snap.filter(|s| s.is_active()))
    }

    /// Increment `used_turns` once per turn while the snapshot is
    /// being injected. Snapshot retires automatically once it crosses
    /// `valid_for_turns`.
    pub fn touch(&self, id: &str) -> DbResult<()> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE cold_start_snapshots
                    SET used_turns = used_turns + 1
                  WHERE id = ?1",
                params![id],
            )?;
            Ok(())
        })
    }
}

fn parse_snapshot(row: &rusqlite::Row<'_>) -> rusqlite::Result<ColdStartSnapshot> {
    let parse_dt = |s: String| -> rusqlite::Result<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })
    };
    let recent_json: String = row.get(4)?;
    let memory_json: String = row.get(5)?;
    let cross_json: String = row.get(6)?;
    Ok(ColdStartSnapshot {
        id: row.get(0)?,
        session_id: row.get(1)?,
        captured_at: parse_dt(row.get(2)?)?,
        rolling_summary: row.get(3)?,
        recent_messages: serde_json::from_str(&recent_json).unwrap_or_default(),
        memory_hits: serde_json::from_str(&memory_json).unwrap_or_default(),
        cross_session: serde_json::from_str(&cross_json).unwrap_or_default(),
        valid_for_turns: row.get::<_, i64>(7)? as u32,
        used_turns: row.get::<_, i64>(8)? as u32,
        created_at: parse_dt(row.get(9)?)?,
    })
}
