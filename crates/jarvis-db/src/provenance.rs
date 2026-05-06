//! Provenance / time-point queries (Section 15.7.5).
//!
//! Three lookups callers ask repeatedly:
//!
//! 1. Time-point replay — given a `(session_id, ts)`, return the most
//!    recent `session_snapshots` row before `ts` together with every
//!    `raw_event_log` row that has happened since.
//! 2. Memory provenance — full `memory_change_log` history for a single
//!    `memory_id`.
//! 3. Trace execution — every `raw_event_log` row tagged with a given
//!    `trace_id`, in chronological order. Used by Walkthrough →
//!    Verifier sanity checks.

use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};

use crate::conn::Db;
use crate::error::DbResult;
use crate::memory_change_log::MemoryChangeEntry;
use crate::raw_event_log::RawEvent;
use crate::session_snapshot::SessionSnapshot;

/// Return type for `replay_session_at`.
#[derive(Debug, Clone)]
pub struct ReplayWindow {
    pub baseline: Option<SessionSnapshot>,
    pub events: Vec<RawEvent>,
}

pub fn replay_session_at(
    db: &Db,
    session_id: &str,
    at: DateTime<Utc>,
) -> DbResult<ReplayWindow> {
    let baseline = nearest_baseline(db, session_id, at)?;
    let from = baseline
        .as_ref()
        .map(|b| b.created_at)
        .unwrap_or_else(|| chrono::DateTime::<Utc>::from(std::time::UNIX_EPOCH));
    let events = events_between(db, session_id, from, at)?;
    Ok(ReplayWindow { baseline, events })
}

fn nearest_baseline(
    db: &Db,
    session_id: &str,
    at: DateTime<Utc>,
) -> DbResult<Option<SessionSnapshot>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, session_id, seq, snapshot_reason,
                    rolling_summary, active_entities_json, unresolved_json,
                    resolved_json, task_tree_json, artifact_index_json,
                    checksum, created_at
               FROM session_snapshots
              WHERE session_id = ?1 AND created_at <= ?2
              ORDER BY created_at DESC
              LIMIT 1",
        )?;
        let r = stmt
            .query_row(params![session_id, at.to_rfc3339()], parse_snapshot)
            .optional()?;
        Ok(r)
    })
}

fn events_between(
    db: &Db,
    session_id: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> DbResult<Vec<RawEvent>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT seq, ts, event_type, session_id, trace_id, agent_id,
                    raw_content, safe_content, checksum
               FROM raw_event_log
              WHERE session_id = ?1
                AND ts >= ?2 AND ts <= ?3
              ORDER BY seq ASC",
        )?;
        let rows = stmt
            .query_map(
                params![session_id, from.to_rfc3339(), to.to_rfc3339()],
                parse_raw_event,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

/// Memory provenance: full change-log history for `memory_id`.
pub fn memory_history(db: &Db, memory_id: &str) -> DbResult<Vec<MemoryChangeEntry>> {
    crate::memory_change_log::history_for(db, memory_id)
}

/// Walk every raw event tagged with `trace_id`. Useful when the caller
/// wants to compare a Walkthrough's claims against what actually
/// happened during execution.
pub fn trace_events(db: &Db, trace_id: &str) -> DbResult<Vec<RawEvent>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT seq, ts, event_type, session_id, trace_id, agent_id,
                    raw_content, safe_content, checksum
               FROM raw_event_log
              WHERE trace_id = ?1
              ORDER BY seq ASC",
        )?;
        let rows = stmt
            .query_map(params![trace_id], parse_raw_event)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

fn parse_snapshot(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionSnapshot> {
    let ts_str: String = row.get(11)?;
    let created_at = DateTime::parse_from_rfc3339(&ts_str)
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
        created_at,
    })
}

fn parse_raw_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawEvent> {
    let ts_str: String = row.get(1)?;
    let ts = DateTime::parse_from_rfc3339(&ts_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    Ok(RawEvent {
        seq: row.get(0)?,
        ts,
        event_type: row.get(2)?,
        session_id: row.get(3)?,
        trace_id: row.get(4)?,
        agent_id: row.get(5)?,
        raw_content: row.get(6)?,
        safe_content: row.get(7)?,
        checksum: row.get(8)?,
    })
}
